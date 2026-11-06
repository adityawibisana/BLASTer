import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";
import { FileArchive, FolderOpen, Loader2, Trash2 } from "lucide-react";
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

  const loadRecent = () => api.recentProjects().then(setRecent).catch(() => setRecent([]));
  useEffect(() => {
    loadRecent();
  }, []);

  const opened = (p: Awaited<ReturnType<typeof api.openProject>>, screen: "import" | "results") => {
    setProject(p);
    select(p.reads[0]?.id ?? null);
    go(screen);
  };

  const newProject = async () => {
    const zip = await open({
      multiple: false,
      filters: [
        { name: "Zip of .ab1 files, or FASTA", extensions: ["zip", ...FASTA_EXTENSIONS] },
        { name: "Zip of .ab1 files", extensions: ["zip"] },
        { name: "FASTA", extensions: FASTA_EXTENSIONS },
      ],
    });
    if (!zip) return;
    setBusy("Reading sequences, finding primers and trimming…");
    try {
      const p = await api.createProject(zip);
      toast.success(`${p.reads.length} samples imported`);
      opened(p, "import");
    } catch (e) {
      toast.error(`Import failed: ${errorText(e)}`);
    } finally {
      setBusy(null);
    }
  };

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
    <div className="flex h-full overflow-auto">
      <div className="mx-auto w-full max-w-[760px] px-6 py-12">
        <h1 className="m-0 text-[26px] font-semibold tracking-tight">{t("start.title")}</h1>
        <p className="mt-1 mb-8 text-muted">{t("start.subtitle")}</p>

        <div className="grid grid-cols-2 gap-3">
          <button className="card flex items-start gap-3 p-4 text-left hover:border-accent" onClick={newProject} disabled={!!busy}>
            <FileArchive className="mt-0.5 text-accent" size={22} />
            <div>
              <div className="font-semibold">{t("start.newProject")}</div>
              <div className="label mt-0.5">Up to 1000 .ab1 traces or FASTA sequences. Primers are found and reads trimmed automatically.</div>
            </div>
          </button>
          <button className="card flex items-start gap-3 p-4 text-left hover:border-accent" onClick={openProject} disabled={!!busy}>
            <FolderOpen className="mt-0.5 text-accent" size={22} />
            <div>
              <div className="font-semibold">{t("start.open")}</div>
              <div className="label mt-0.5">A saved .coiproj file. Opens offline, with all trims and results.</div>
            </div>
          </button>
        </div>

        <div className="label mt-2 px-1">
          A .zip of .ab1 traces gives the most complete analysis: each base's quality score drives trimming and QC, and the
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
