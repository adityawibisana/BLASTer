import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { FileArchive, Folder, FolderOpen, Loader2, Trash2, Upload } from "lucide-react";
import { toast } from "sonner";
import { api, errorText } from "../api";
import { useApp } from "../state";
import { shortDate } from "../lib/format";
import type { RecentProject } from "../types";

const FASTA_EXTENSIONS = ["fasta", "fas", "fa", "fna", "ffn", "seq"];

export default function StartScreen() {
  const { t } = useTranslation();
  const { setProject, go, select } = useApp();
  const [recent, setRecent] = useState<RecentProject[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [dragging, setDragging] = useState(false);
  const busyRef = useRef(false);
  busyRef.current = !!busy;

  const loadRecent = () => api.recentProjects().then(setRecent).catch(() => setRecent([]));
  useEffect(() => {
    loadRecent();
  }, []);

  const opened = (p: Awaited<ReturnType<typeof api.openProject>>, screen: "import" | "results") => {
    setProject(p);
    select(p.reads[0]?.id ?? null);
    go(screen);
  };

  const importPaths = async (paths: string[]) => {
    if (paths.length === 0) return;
    setBusy("Reading sequences, finding primers and trimming…");
    try {
      const p = await api.createProject(paths);
      toast.success(`${p.reads.length} samples imported`);
      opened(p, "import");
    } catch (e) {
      toast.error(`Import failed: ${errorText(e)}`);
    } finally {
      setBusy(null);
    }
  };

  const chooseFiles = async () => {
    const picked = await open({
      multiple: true,
      filters: [
        { name: "Zip, .ab1 or FASTA", extensions: ["zip", "ab1", ...FASTA_EXTENSIONS] },
        { name: "Zip of .ab1 files", extensions: ["zip"] },
        { name: ".ab1 traces", extensions: ["ab1"] },
        { name: "FASTA", extensions: FASTA_EXTENSIONS },
      ],
    });
    if (picked) importPaths(Array.isArray(picked) ? picked : [picked]);
  };

  const chooseFolder = async () => {
    const dir = await open({ directory: true, multiple: false });
    if (dir) importPaths([dir]);
  };

  // Files and folders dropped on the window start a new project.
  const dropRef = useRef(importPaths);
  dropRef.current = importPaths;
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let gone = false;
    getCurrentWebview()
      .onDragDropEvent((e) => {
        if (e.payload.type === "enter" || e.payload.type === "over") setDragging(!busyRef.current);
        else if (e.payload.type === "leave") setDragging(false);
        else if (e.payload.type === "drop") {
          setDragging(false);
          if (!busyRef.current) dropRef.current(e.payload.paths);
        }
      })
      .then((f) => (gone ? f() : (unlisten = f)))
      .catch(() => {});
    return () => {
      gone = true;
      unlisten?.();
    };
  }, []);

  const openPath = async (path: string) => {
    setBusy("Opening project…");
    try {
      const p = await api.openProject(path);
      opened(p, p.groups.some((g) => g.status === "ready") ? "results" : "import");
    } catch (e) {
      toast.error(`Cannot open project: ${errorText(e)}`);
      loadRecent();
    } finally {
      setBusy(null);
    }
  };

  const openProject = async () => {
    const path = await open({ multiple: false, filters: [{ name: "COI Barcode BLAST project", extensions: ["coiproj"] }] });
    if (path) openPath(path);
  };

  return (
    <div className="relative flex h-full overflow-auto">
      {dragging && (
        <div className="pointer-events-none absolute inset-3 z-10 flex flex-col items-center justify-center gap-2 rounded-lg border-2 border-dashed border-accent bg-panel/90">
          <Upload className="text-accent" size={28} />
          <div className="font-semibold">Drop to start a new project</div>
          <div className="label">A .zip, .ab1 traces, FASTA files or a folder</div>
        </div>
      )}
      <div className="mx-auto w-full max-w-[760px] px-6 py-12">
        <h1 className="m-0 text-[26px] font-semibold tracking-tight">{t("start.title")}</h1>
        <p className="mt-1 mb-8 text-muted">{t("start.subtitle")}</p>

        <div className="grid grid-cols-2 gap-3">
          <div className="card flex flex-col">
            <button className="flex flex-1 items-start gap-3 p-4 text-left hover:text-accent" onClick={chooseFiles} disabled={!!busy}>
              <FileArchive className="mt-0.5 shrink-0 text-accent" size={22} />
              <div>
                <div className="font-semibold">{t("start.newProject")}</div>
                <div className="label mt-0.5">
                  A .zip, one or more .ab1 traces, or FASTA files: up to 1000 samples. Primers are found and reads trimmed
                  automatically.
                </div>
              </div>
            </button>
            <div className="flex items-center gap-2 border-t border-line px-4 py-2">
              <button className="btn btn-sm" onClick={chooseFolder} disabled={!!busy}>
                <Folder size={13} /> {t("start.folder")}
              </button>
              <span className="label whitespace-nowrap">or drag &amp; drop files here</span>
            </div>
          </div>
          <button className="card flex items-start gap-3 p-4 text-left hover:border-accent" onClick={openProject} disabled={!!busy}>
            <FolderOpen className="mt-0.5 shrink-0 text-accent" size={22} />
            <div>
              <div className="font-semibold">{t("start.open")}</div>
              <div className="label mt-0.5">A saved .coiproj file. Opens offline, with all trims and results.</div>
            </div>
          </button>
        </div>

        <div className="label mt-2 px-1">
          .ab1 traces give the most complete analysis: each base's quality score drives trimming and QC, and the
          chromatogram lets you check doubtful bases. A FASTA file holds only the sequence, so every base is taken as correct:
          primers are still removed, but there is no quality trimming, % HQ or chromatogram.
        </div>

        {busy && (
          <div className="card mt-4 flex items-center gap-2 p-3 text-muted">
            <Loader2 size={16} className="animate-spin text-accent" /> {busy}
          </div>
        )}

        <h2 className="mt-10 mb-2 text-[14px] font-semibold">{t("start.recent")}</h2>
        <div className="card divide-y divide-line">
          {recent.length === 0 && <div className="p-4 text-muted">{t("start.none")}</div>}
          {recent.map((r) => (
            <div key={r.path} className="flex items-center gap-3 px-4 py-2.5">
              <button
                className="min-w-0 flex-1 text-left disabled:cursor-default"
                disabled={!r.exists || !!busy}
                onClick={() => openPath(r.path)}
              >
                <div className={r.exists ? "font-medium hover:text-accent" : "font-medium text-muted line-through"}>{r.name}</div>
                <div className="label truncate">
                  {r.exists ? r.path : `${t("start.missing")}: ${r.path}`} · {shortDate(r.openedAt)}
                </div>
              </button>
              <button
                className="btn btn-sm"
                title={t("start.forget")}
                onClick={() => api.forgetRecent(r.path).then(loadRecent)}
              >
                <Trash2 size={13} />
              </button>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
