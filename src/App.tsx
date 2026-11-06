import clsx from "clsx";
import { Toaster } from "sonner";
import { useTranslation } from "react-i18next";
import { save } from "@tauri-apps/plugin-dialog";
import {
  Dna,
  FileDown,
  FolderOpen,
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
import StartScreen from "./screens/StartScreen";
import ImportScreen from "./screens/ImportScreen";
import TrimScreen from "./screens/TrimScreen";
import BlastScreen from "./screens/BlastScreen";
import ResultsScreen from "./screens/ResultsScreen";
import ExportScreen from "./screens/ExportScreen";
import SettingsScreen from "./screens/SettingsScreen";

const NAV: { id: Screen; icon: typeof Home; needsProject: boolean }[] = [
  { id: "start", icon: Home, needsProject: false },
  { id: "import", icon: FolderOpen, needsProject: true },
  { id: "trim", icon: SlidersHorizontal, needsProject: true },
  { id: "blast", icon: Search, needsProject: true },
  { id: "results", icon: ListChecks, needsProject: true },
  { id: "export", icon: FileDown, needsProject: true },
  { id: "settings", icon: SettingsIcon, needsProject: false },
];

function Shell() {
  const { t } = useTranslation();
  const { project, screen, go, run, setProject, select } = useApp();

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
    results: ResultsScreen,
    export: ExportScreen,
    settings: SettingsScreen,
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
                  {project.reads.length} {t("common.samples")} · {project.path}
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
