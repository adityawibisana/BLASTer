import { invoke } from "@tauri-apps/api/core";
import type {
  Derived,
  Dir,
  HitDetail,
  PrimerSet,
  ProjectView,
  RecentProject,
  SampleResult,
  Settings,
  TraceView,
} from "./types";

export const api = {
  getDefaults: () => invoke<Settings>("get_defaults"),
  saveDefaults: (settings: Settings) => invoke<void>("save_defaults", { settings }),
  recentProjects: () => invoke<RecentProject[]>("recent_projects"),
  forgetRecent: (path: string) => invoke<void>("forget_recent", { path }),
  primerLibrary: () => invoke<PrimerSet[]>("primer_library"),

  getProject: () => invoke<ProjectView | null>("get_project"),
  createProject: (zipPath: string) => invoke<ProjectView>("create_project", { zipPath }),
  openProject: (path: string) => invoke<ProjectView>("open_project", { path }),
  closeProject: () => invoke<void>("close_project"),
  saveProjectAs: (path: string) => invoke<ProjectView>("save_project_as", { path }),

  getTrace: (id: string) => invoke<TraceView>("get_trace", { id }),
  previewTrim: (id: string, start: number, end: number) => invoke<Derived>("preview_trim", { id, start, end }),
  setTrim: (id: string, start: number, end: number) => invoke<void>("set_trim", { id, start, end }),
  resetRead: (id: string) => invoke<void>("reset_read", { id }),
  setDirection: (id: string, direction: Dir | null) => invoke<void>("set_direction", { id, direction }),
  setInclusion: (ids: string[], includeFailed?: boolean, excluded?: boolean) =>
    invoke<void>("set_inclusion", { ids, includeFailed: includeFailed ?? null, excluded: excluded ?? null }),
  updateSettings: (settings: Settings) => invoke<ProjectView>("update_settings", { settings }),

  getSampleResult: (id: string) => invoke<SampleResult | null>("get_sample_result", { id }),
  getHitDetail: (id: string, rank: number) => invoke<HitDetail>("get_hit_detail", { id, rank }),
  compileResults: () => invoke<ProjectView>("compile_results"),

  exportExcel: (path: string) => invoke<void>("export_excel", { path }),
  exportFasta: (path: string, onlyEligible: boolean) => invoke<number>("export_fasta", { path, onlyEligible }),
  exportRaw: (path: string) => invoke<number>("export_raw", { path }),

  startBlast: (onlyChanged: boolean) =>
    invoke<{ queuedSamples: number; groups: number }>("start_blast", { onlyChanged }),
  cancelBlast: () => invoke<void>("cancel_blast"),
};

export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return JSON.stringify(e);
}
