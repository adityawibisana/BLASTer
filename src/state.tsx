import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { api, errorText } from "./api";
import i18n from "./i18n";
import type { ProjectView } from "./types";

export type Screen = "start" | "import" | "trim" | "blast" | "results" | "export" | "settings";

interface AppContextValue {
  project: ProjectView | null;
  setProject: (p: ProjectView | null) => void;
  refresh: () => Promise<void>;
  screen: Screen;
  go: (s: Screen) => void;
  selectedId: string | null;
  select: (id: string | null) => void;
  /** Run an action, show its error as a toast, refresh afterwards. */
  run: <T>(action: () => Promise<T>, success?: string) => Promise<T | undefined>;
}

const AppContext = createContext<AppContextValue | null>(null);

export function AppProvider({ children }: { children: ReactNode }) {
  const [project, setProjectState] = useState<ProjectView | null>(null);
  const [screen, setScreen] = useState<Screen>("start");
  const [selectedId, select] = useState<string | null>(null);
  const pending = useRef<number | null>(null);

  const setProject = useCallback((p: ProjectView | null) => {
    setProjectState(p);
    if (p) i18n.changeLanguage(p.settings.language || "en");
  }, []);

  const refresh = useCallback(async () => {
    try {
      setProject(await api.getProject());
    } catch (e) {
      toast.error(errorText(e));
    }
  }, [setProject]);

  useEffect(() => {
    refresh();
    api.getDefaults().then((d) => i18n.changeLanguage(d.language || "en")).catch(() => {});
    const offChanged = listen<string>("project-changed", () => {
      // Coalesce bursts of events into one refresh.
      if (pending.current) window.clearTimeout(pending.current);
      pending.current = window.setTimeout(refresh, 150);
    });
    const offSave = listen<string>("save-error", (e) => {
      toast.error(`The project could not be saved: ${e.payload}. Your work is kept in memory; use "Save as…" to choose another location.`, {
        duration: 15000,
      });
    });
    return () => {
      offChanged.then((f) => f());
      offSave.then((f) => f());
    };
  }, [refresh]);

  const run = useCallback(
    async <T,>(action: () => Promise<T>, success?: string) => {
      try {
        const out = await action();
        if (success) toast.success(success);
        await refresh();
        return out;
      } catch (e) {
        toast.error(errorText(e));
        return undefined;
      }
    },
    [refresh],
  );

  return (
    <AppContext.Provider value={{ project, setProject, refresh, screen, go: setScreen, selectedId, select, run }}>
      {children}
    </AppContext.Provider>
  );
}

export function useApp() {
  const ctx = useContext(AppContext);
  if (!ctx) throw new Error("useApp outside AppProvider");
  return ctx;
}
