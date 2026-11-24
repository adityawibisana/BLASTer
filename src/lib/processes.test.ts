import { describe, expect, it } from "vitest";
import { canResend, countdown, deadline, giveUpMs, isSlow, nextCheck, runTime, stage, summary } from "./processes";
import type { GroupView } from "../types";

const T0 = Date.parse("2026-11-24T16:18:00+07:00");
const iso = (ms: number) => new Date(ms).toISOString();

const g = (over: Partial<GroupView>): GroupView => ({
  id: "g0001",
  sampleIds: ["a", "b"],
  status: "submitted",
  rid: "R1",
  submittedAt: iso(T0),
  finishedAt: null,
  attempts: 1,
  error: null,
  rtoeSecs: 30,
  lastCheckedAt: null,
  lastCheck: null,
  ...over,
});

describe("processes", () => {
  it("names the stage from status and NCBI's last answer", () => {
    expect(stage(g({ status: "pending", rid: null, submittedAt: null }))).toBe("queued");
    expect(stage(g({}))).toBe("sent");
    expect(stage(g({ lastCheckedAt: iso(T0 + 60_000), lastCheck: "WAITING" }))).toBe("searching");
    expect(stage(g({ lastCheckedAt: iso(T0 + 60_000), lastCheck: "READY" }))).toBe("downloading");
    expect(stage(g({ status: "ready" }))).toBe("done");
    for (const status of ["failed", "expired", "cancelled"] as const) expect(stage(g({ status }))).toBe("failed");
  });

  it("is slow only after 15 minutes at NCBI", () => {
    expect(isSlow(g({}), T0 + 14 * 60_000)).toBe(false);
    expect(isSlow(g({}), T0 + 16 * 60_000)).toBe(true);
    expect(isSlow(g({ status: "ready", finishedAt: iso(T0 + 3_600_000) }), T0 + 7_200_000)).toBe(false);
  });

  it("gives up at the setting, kept within 0.5–24 h", () => {
    expect(deadline(g({}), 2)).toBe(T0 + 2 * 3_600_000);
    expect(deadline(g({}), 3)).toBe(T0 + 3 * 3_600_000);
    expect(giveUpMs(0)).toBe(1_800_000);
    expect(giveUpMs(undefined)).toBe(7_200_000);
    expect(deadline(g({ status: "ready" }), 2)).toBeNull();
  });

  it("next check is a minute after the last one, or after NCBI's estimate", () => {
    expect(nextCheck(g({}))).toBe(T0 + 60_000);
    expect(nextCheck(g({ rtoeSecs: 90 }))).toBe(T0 + 90_000);
    expect(nextCheck(g({ lastCheckedAt: iso(T0 + 300_000) }))).toBe(T0 + 360_000);
    expect(countdown(42_000)).toBe("0:42");
    expect(countdown(-1)).toBe("now");
  });

  it("run time counts while searching and stops when done", () => {
    expect(runTime(g({}), T0 + 83 * 60_000)).toBe(83 * 60_000);
    expect(runTime(g({ status: "ready", finishedAt: iso(T0 + 71_000) }), T0 + 9e9)).toBe(71_000);
    expect(runTime(g({ status: "pending", submittedAt: null }), T0)).toBeNull();
  });

  it("only waiting or unfinished groups can be sent again", () => {
    expect(["pending", "submitted", "ready", "failed", "expired", "cancelled"].filter((s) => canResend(g({ status: s as GroupView["status"] })))).toEqual([
      "submitted",
      "failed",
      "expired",
      "cancelled",
    ]);
  });

  it("summarises the batch", () => {
    const s = summary(
      [g({ status: "ready", sampleIds: ["a", "b", "c"] }), g({}), g({ status: "pending" }), g({ status: "failed" })],
      T0 + 20 * 60_000,
    );
    expect(s).toEqual({ total: 4, done: 1, failed: 1, open: 2, slow: true, samplesTotal: 9, samplesDone: 3 });
  });
});
