import { useEffect, useState } from "react";
import clsx from "clsx";
import { listen } from "@tauri-apps/api/event";
import { Toaster } from "sonner";
import { useTranslation } from "react-i18next";
import { save } from "@tauri-apps/plugin-dialog";
import {
  Activity,
  Dna,
  FileDown,
  FolderOpen,
  HelpCircle,
  Home,
  ListChecks,
  Loader2,
  Search,
  Settings as SettingsIcon,
  SlidersHorizontal,
  X,
} from "lucide-react";
import { AppProvider, useApp, type Screen } from "./state";
import { api } from "./api";
import { Modal } from "./components/ui";
import { summary } from "./lib/processes";
import StartScreen from "./screens/StartScreen";
import ImportScreen from "./screens/ImportScreen";
import TrimScreen from "./screens/TrimScreen";
import BlastScreen from "./screens/BlastScreen";
import ResultsScreen from "./screens/ResultsScreen";
import ExportScreen from "./screens/ExportScreen";
import SettingsScreen from "./screens/SettingsScreen";
import HelpScreen from "./screens/HelpScreen";
import ProcessesScreen from "./screens/ProcessesScreen";

const NAV: { id: Screen; icon: typeof Home; needsProject: boolean }[] = [
  { id: "start", icon: Home, needsProject: false },
  { id: "import", icon: FolderOpen, needsProject: true },
  { id: "trim", icon: SlidersHorizontal, needsProject: true },
  { id: "blast", icon: Search, needsProject: true },
  { id: "processes", icon: Activity, needsProject: false },
  { id: "results", icon: ListChecks, needsProject: true },
  { id: "export", icon: FileDown, needsProject: true },
  { id: "settings", icon: SettingsIcon, needsProject: false },
  { id: "help", icon: HelpCircle, needsProject: false },
];

function Shell() {
  const { t } = useTranslation();
  const { project, screen, go, run, setProject, select } = useApp();
  const [closeAsk, setCloseAsk] = useState<number | null>(null);
  const [now, setNow] = useState(Date.now());

  // The window asks before closing while a search runs (the backend holds the close).
  useEffect(() => {
    const off = listen<number>("confirm-close", (e) => setCloseAsk(e.payload));
    return () => {
      off.then((f) => f());
    };
  }, []);

  // Keep the menu badge's "slow" colour current while searches run.
  const running = !!project?.blastRunning;
  useEffect(() => {
    if (!running) return;
    const t = window.setInterval(() => setNow(Date.now()), 15_000);
    return () => window.clearInterval(t);
  }, [running]);
  const proc = summary(project?.groups ?? [], now);

  const closeProject = async () => {
    const ok = await run(() => api.closeProject());
    if (ok !== undefined) {
      setProject(null);
      select(null);
      go("start");
    }
  };

  const saveAs = async () => {
    const path = await save({
      defaultPath: `${project?.name ?? "project"}.coiproj`,
      filters: [{ name: "COI Barcode BLAST project", extensions: ["coiproj"] }],
    });
    if (path) await run(() => api.saveProjectAs(path), "Project saved");
  };

  const Body = {
    start: StartScreen,
    import: ImportScreen,
    trim: TrimScreen,
    blast: BlastScreen,
    processes: ProcessesScreen,
    results: ResultsScreen,
    export: ExportScreen,
    settings: SettingsScreen,
    help: HelpScreen,
  }[screen];

  return (
    <div className="flex h-full">
      <aside className="flex w-[212px] shrink-0 flex-col border-r border-line bg-panel">
        <div className="flex items-center gap-2 px-4 pt-4 pb-3">
          <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-accent text-white">
            <Dna size={18} />
          </div>
          <div className="leading-tight">
            <div className="font-semibold">COI Barcode</div>
            <div className="label">BLAST</div>
          </div>
        </div>
        <nav className="flex flex-col gap-0.5 px-2">
          {NAV.map(({ id, icon: Icon, needsProject }) => {
            const disabled = needsProject && !project;
            return (
              <button
                key={id}
                disabled={disabled}
                onClick={() => go(id)}
                className={clsx(
                  "flex h-9 items-center gap-2.5 rounded-md px-3 text-left font-medium",
                  screen === id ? "bg-accent-soft text-accent-strong" : "text-ink hover:bg-panel-2",
                  disabled && "cursor-default opacity-40 hover:bg-transparent",
                )}
              >
                <Icon size={16} />
                {t(`nav.${id}`)}
                {id === "blast" && project?.blastRunning && <Loader2 size={14} className="ml-auto animate-spin text-accent" />}
                {id === "processes" && running && proc.open > 0 && (
                  <span
                    title={proc.slow ? "A search is taking longer than usual" : `${proc.open} group(s) left`}
                    className={clsx(
                      "ml-auto inline-flex items-center gap-1 rounded-full px-1.5 text-[11.5px] font-semibold",
                      proc.slow ? "bg-warn-soft text-warn" : "bg-accent-soft text-accent-strong",
                    )}
                  >
                    <Loader2 size={11} className="animate-spin" />
                    {proc.open}
                  </span>
                )}
                {id === "processes" && !running && proc.failed > 0 && (
                  <span title={`${proc.failed} group(s) did not finish`} className="ml-auto rounded-full bg-bad-soft px-1.5 text-[11.5px] font-semibold text-bad">
                    {proc.failed}
                  </span>
                )}
              </button>
            );
          })}
        </nav>
        <div className="mt-auto border-t border-line p-3">
          {project ? (
            <div className="flex flex-col gap-2">
              <div className="min-w-0">
                <div className="truncate font-semibold" title={project.name}>
                  {project.name}
                </div>
                <div className="label truncate" title={project.path}>
                  {project.reads.filter((r) => !r.pairedWith).length} {t("common.samples")} · {project.path}
                </div>
              </div>
              <div className="flex gap-1.5">
                <button className="btn btn-sm flex-1" onClick={saveAs}>
                  {t("common.saveAs")}
                </button>
                <button className="btn btn-sm" title={t("common.close")} onClick={closeProject}>
                  <X size={14} />
                </button>
              </div>
            </div>
          ) : (
            <div className="label">{t("common.noProject")}</div>
          )}
        </div>
      </aside>
      <main className="flex min-w-0 flex-1 flex-col overflow-hidden">
        <Body />
      </main>
      <Modal
        open={closeAsk != null}
        onOpenChange={(v) => !v && setCloseAsk(null)}
        title="A BLAST search is still running"
        footer={
          <>
            <button className="btn" onClick={() => setCloseAsk(null)}>
              Keep open
            </button>
            <button className="btn btn-primary" onClick={() => api.quitApp()}>
              Close
            </button>
          </>
        }
      >
        <p className="m-0">
          {closeAsk} group{closeAsk === 1 ? "" : "s"} still {closeAsk === 1 ? "waits" : "wait"} for NCBI. If you close now, the search stops and
          continues automatically the next time you open this project. Nothing is lost.
        </p>
      </Modal>
    </div>
  );
}

export default function App() {
  return (
    <AppProvider>
      <Shell />
      <Toaster position="bottom-right" richColors closeButton />
    </AppProvider>
  );
}
