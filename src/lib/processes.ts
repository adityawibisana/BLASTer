import type { GroupView } from "../types";

/** Where a group's search is, as the user sees it. */
export type Stage = "queued" | "sent" | "searching" | "downloading" | "done" | "failed";

/** The stage track, in order. "failed" replaces the last steps when a search did not finish. */
export const TRACK: { stage: Stage; label: string }[] = [
  { stage: "queued", label: "Queued" },
  { stage: "sent", label: "Sent" },
  { stage: "searching", label: "NCBI searching" },
  { stage: "downloading", label: "Downloading" },
  { stage: "done", label: "Done" },
];

/** NCBI usually finishes a group of 20 COI reads in 1–10 minutes; past this it counts as slow. */
export const SLOW_AFTER_MS = 15 * 60_000;
/** NCBI allows one status check per search per minute. */
export const POLL_MS = 60_000;

export function stage(g: GroupView): Stage {
  switch (g.status) {
    case "pending":
      return "queued";
    case "submitted":
      // BLAST+ on this computer has no NCBI steps: it is searching until done.
      if (g.source === "local") return "searching";
      if (g.lastCheck === "READY") return "downloading";
      return g.lastCheckedAt ? "searching" : "sent";
    case "ready":
      return "done";
    default:
      return "failed";
  }
}

/** How long the search has been at NCBI (or took, once done). */
export function runTime(g: GroupView, now: number): number | null {
  if (!g.submittedAt) return null;
  const start = Date.parse(g.submittedAt);
  const end = g.status === "ready" && g.finishedAt ? Date.parse(g.finishedAt) : g.status === "submitted" ? now : null;
  return end == null ? null : Math.max(0, end - start);
}

/** A search waiting at NCBI for longer than usual (a search on this computer is never "slow at NCBI"). */
export function isSlow(g: GroupView, now: number): boolean {
  return g.status === "submitted" && g.source !== "local" && (runTime(g, now) ?? 0) > SLOW_AFTER_MS;
}

/** Same limits as the app's backend (10 min – 24 h, 120 min when unset). */
export function giveUpMs(minutes: number | undefined): number {
  const m = minutes != null && Number.isFinite(minutes) ? Math.min(1440, Math.max(10, Math.round(minutes))) : 120;
  return m * 60_000;
}

/** When the app gives up on a waiting search. */
export function deadline(g: GroupView, giveUpMinutes: number | undefined): number | null {
  if (g.status !== "submitted" || !g.submittedAt || g.source === "local") return null;
  return Date.parse(g.submittedAt) + giveUpMs(giveUpMinutes);
}

/** When the app next asks NCBI about this search. */
export function nextCheck(g: GroupView): number | null {
  if (g.status !== "submitted" || g.source === "local") return null;
  if (g.lastCheckedAt) return Date.parse(g.lastCheckedAt) + POLL_MS;
  if (!g.submittedAt) return null;
  return Date.parse(g.submittedAt) + Math.max(POLL_MS, (g.rtoeSecs ?? 0) * 1000);
}

/** A group whose search can be sent to NCBI again. */
export const canResend = (g: GroupView) => ["submitted", "failed", "expired", "cancelled"].includes(g.status);

/** Countdown text: "0:42", or "now" once it is due. */
export function countdown(ms: number): string {
  if (ms <= 0) return "now";
  const s = Math.ceil(ms / 1000);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

/** "1 h 23 min", "4 min", "35 s": short run time for the table. */
export function runTimeText(ms: number): string {
  const s = Math.floor(ms / 1000);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (h > 0) return `${h} h ${m} min`;
  if (m > 0) return `${m} min ${s % 60} s`;
  return `${s} s`;
}

export const clock = (t: number) => new Date(t).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });

/** Summary for the menu badge and the batch bar. */
export function summary(groups: GroupView[], now: number) {
  const open = groups.filter((g) => g.status === "pending" || g.status === "submitted");
  return {
    total: groups.length,
    done: groups.filter((g) => g.status === "ready").length,
    failed: groups.filter((g) => stage(g) === "failed").length,
    open: open.length,
    slow: groups.some((g) => isSlow(g, now)),
    // Distinct samples: a sample searched again (e.g. after a change) is in more than one group.
    samplesTotal: new Set(groups.flatMap((g) => g.sampleIds)).size,
    samplesDone: new Set(groups.filter((g) => g.status === "ready").flatMap((g) => g.sampleIds)).size,
  };
}
