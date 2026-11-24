import { Fragment, useEffect, useState } from "react";
import clsx from "clsx";
import { openUrl } from "@tauri-apps/plugin-opener";
import { AlertTriangle, Check, ChevronDown, ChevronRight, Cpu, ExternalLink, FileUp, Globe, Loader2, RotateCcw, X } from "lucide-react";
import { api } from "../api";
import { useApp } from "../state";
import { Empty, Modal, PageHeader } from "../components/ui";
import {
  TRACK,
  canResend,
  clock,
  countdown,
  deadline,
  isSlow,
  nextCheck,
  runTime,
  runTimeText,
  stage,
  summary,
  type Stage,
} from "../lib/processes";
import { pickAndImportResults } from "../lib/blastImport";
import type { GroupView } from "../types";

const ncbiPage = (rid: string) => `https://blast.ncbi.nlm.nih.gov/Blast.cgi?CMD=Get&RID=${encodeURIComponent(rid)}`;

const ANSWER: Record<string, string> = {
  WAITING: "still searching",
  READY: "finished",
  FAILED: "search failed",
  UNKNOWN: "does not know this search",
};

type Tone = "active" | "slow" | "done" | "bad" | "idle";

function tone(g: GroupView, now: number): Tone {
  const s = stage(g);
  if (s === "done") return "done";
  if (s === "failed") return "bad";
  if (s === "queued") return "idle";
  return isSlow(g, now) ? "slow" : "active";
}

const TONE_TEXT: Record<Tone, string> = {
  active: "text-accent",
  slow: "text-warn",
  done: "text-ok",
  bad: "text-bad",
  idle: "text-muted",
};

function useNow() {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const t = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(t);
  }, []);
  return now;
}

/** Status dot: spins while NCBI works, pulses once each time the app checks. */
function Indicator({ g, now }: { g: GroupView; now: number }) {
  const t = tone(g, now);
  if (t === "done") return <Check size={16} className="text-ok" />;
  if (t === "bad") return <X size={16} className="text-bad" />;
  if (t === "idle") return <span className="block h-2.5 w-2.5 rounded-full border-2 border-muted/60" />;
  return <Loader2 size={16} className={clsx("animate-spin", TONE_TEXT[t])} />;
}

/** Queued ─ Sent ─ NCBI searching ─ Downloading ─ Done, with the current step marked. */
function StageTrack({ g, now }: { g: GroupView; now: number }) {
  const s = stage(g);
  const t = tone(g, now);
  const failed = s === "failed";
  // A failed search stopped while NCBI was searching.
  const current: Stage = failed ? "searching" : s;
  // BLAST+ on this computer has no NCBI steps; imported results have no search steps at all.
  const track =
    g.source === "local"
      ? TRACK.filter((x) => x.stage !== "sent" && x.stage !== "downloading").map((x) =>
          x.stage === "searching" ? { ...x, label: "Searching on this computer" } : x,
        )
      : g.source === "file" && s === "done"
        ? TRACK.filter((x) => x.stage === "queued" || x.stage === "done").map((x) => (x.stage === "done" ? { ...x, label: "Imported" } : x))
        : TRACK;
  const at = track.findIndex((x) => x.stage === current);
  return (
    <div className="flex items-center gap-1 text-[11.5px]">
      {track.map((step, i) => {
        const past = i < at || s === "done";
        const here = i === at && s !== "done";
        return (
          <Fragment key={step.stage}>
            {i > 0 && <span className={clsx("h-px w-3", past || here ? "bg-current opacity-40" : "bg-line")} />}
            <span
              className={clsx(
                "inline-flex items-center gap-1 rounded px-1.5 py-px whitespace-nowrap",
                past && "text-ok",
                here && !failed && t === "slow" && "bg-warn-soft font-semibold text-warn",
                here && !failed && t !== "slow" && "bg-accent-soft font-semibold text-accent-strong",
                here && failed && "bg-bad-soft font-semibold text-bad",
                !past && !here && "text-muted/70",
              )}
            >
              {past && <Check size={11} />}
              {here && failed ? "Stopped" : step.label}
            </span>
          </Fragment>
        );
      })}
    </div>
  );
}

/** "NCBI said still searching at 17:36 · next check in 0:42", with a pulse at each check. */
function Heartbeat({ g, now }: { g: GroupView; now: number }) {
  if (g.status === "ready" && g.source === "file") return <div className="text-muted">Results imported from a file</div>;
  if (g.status === "ready" && g.source === "local") return <div className="text-muted">Searched with BLAST+ on this computer</div>;
  if (g.status !== "submitted") return null;
  if (g.source === "local") {
    return (
      <div className="flex items-center gap-2 text-muted">
        <Cpu size={13} className="text-accent" />
        BLAST+ is searching on this computer
      </div>
    );
  }
  if (stage(g) === "downloading") {
    return (
      <div className="flex items-center gap-2 text-muted">
        <span className="relative inline-flex h-2 w-2 rounded-full bg-ok" />
        NCBI finished at {clock(Date.parse(g.lastCheckedAt!))}; downloading the results…
      </div>
    );
  }
  const next = nextCheck(g);
  const due = next != null ? countdown(next - now) : null;
  return (
    <div className="flex items-center gap-2 text-muted">
      <span className="relative flex h-2 w-2">
        <span key={g.lastCheckedAt ?? "none"} className="beat absolute inline-flex h-full w-full rounded-full bg-accent" />
        <span className="relative inline-flex h-2 w-2 rounded-full bg-accent" />
      </span>
      {g.lastCheckedAt ? (
        <span>
          NCBI said <b className="font-medium text-ink">{ANSWER[g.lastCheck ?? ""] ?? g.lastCheck}</b> at {clock(Date.parse(g.lastCheckedAt))}
        </span>
      ) : (
        <span>Sent to NCBI{g.rtoeSecs ? `; NCBI estimated about ${Math.max(1, Math.round(g.rtoeSecs / 60))} min` : ""}</span>
      )}
      {due && <span>· {due === "now" ? "checking now…" : `next check in ${due}`}</span>}
    </div>
  );
}

export default function ProcessesScreen() {
  const { project, go, run } = useApp();
  const now = useNow();
  const [open, setOpen] = useState<Record<string, boolean>>({});
  const [resend, setResend] = useState<GroupView | null>(null);
  if (!project) return <Empty>No project open. Processes are the BLAST searches of the open project.</Empty>;

  const groups = project.groups;
  const giveUp = project.settings.blast.giveUpHours;
  const where = project.settings.blast.location ?? "ncbi";
  const onWebsite = (g: GroupView) => run(() => api.openOnWebsite(g.id));
  const waitingForYou = where === "website" ? groups.filter((g) => g.status === "pending") : [];
  const s = summary(groups, now);
  const active = groups.find((g) => g.status === "submitted");
  const slow = groups.filter((g) => isSlow(g, now));

  const confirmResend = async () => {
    const g = resend;
    setResend(null);
    if (g) await run(() => api.resubmitGroup(g.id), `Group ${g.id} will be sent to NCBI again`);
  };

  return (
    <div className="h-full overflow-auto">
      <PageHeader
        title="Processes"
        subtitle={
          where === "local"
            ? "BLAST searches, one group at a time, with BLAST+ on this computer. Closing the app is safe: an unfinished group starts over next time."
            : where === "website"
              ? "BLAST groups. You run each on the NCBI BLAST website and import the downloaded results here."
              : "BLAST searches sent to NCBI. They run in the background, one group at a time as NCBI asks. Closing the app is safe: they continue next time."
        }
        actions={
          <>
            <button className="btn" title="Results downloaded from the NCBI BLAST website, or written by BLAST+" onClick={() => pickAndImportResults()}>
              <FileUp size={14} /> Import results file
            </button>
            {project.blastRunning && (
              <button className="btn" onClick={() => api.cancelBlast()}>
                Cancel search
              </button>
            )}
          </>
        }
      />
      <div className="flex flex-col gap-4 px-6 pb-6">
        {groups.length === 0 ? (
          <div className="card flex items-center justify-between gap-3 p-5 text-muted">
            No searches yet.
            <button className="btn" onClick={() => go("blast")}>
              Go to BLAST
            </button>
          </div>
        ) : (
          <div className="card flex flex-col gap-2 p-4">
            <div className="flex items-baseline justify-between gap-3">
              <div className="font-semibold">
                {s.open > 0 && project.blastRunning
                  ? `Running · ${s.open} group${s.open > 1 ? "s" : ""} left`
                  : s.open > 0 && where === "website" && waitingForYou.length === s.open
                    ? `Waiting for you · ${s.open} group${s.open > 1 ? "s" : ""} to run on the NCBI website`
                  : s.open > 0
                    ? `Paused · ${s.open} group${s.open > 1 ? "s" : ""} left`
                    : s.failed > 0
                      ? `Stopped · ${s.failed} group${s.failed > 1 ? "s" : ""} did not finish`
                      : "All searches finished"}
              </div>
              <div className="tabular text-muted">
                Groups done {s.done} / {s.total} · {s.samplesDone} of {s.samplesTotal} samples have results
              </div>
            </div>
            <div className="h-2 overflow-hidden rounded-full bg-panel-2">
              <div
                className={clsx("h-full rounded-full transition-all", s.slow ? "bg-warn" : "bg-accent")}
                style={{ width: `${s.total ? (100 * s.done) / s.total : 0}%` }}
              />
            </div>
          </div>
        )}

        {waitingForYou.length > 0 && (
          <div className="flex items-start gap-3 rounded-[10px] border border-accent bg-accent-soft/40 p-4">
            <Globe size={18} className="mt-0.5 shrink-0 text-accent" />
            <div className="flex min-w-0 flex-1 flex-col gap-1">
              <div className="font-semibold">
                {waitingForYou.length} group{waitingForYou.length > 1 ? "s" : ""} to run on the NCBI website
              </div>
              <ol className="m-0 flex list-decimal flex-col gap-0.5 pl-5">
                <li>
                  Click <b>NCBI website</b> on a group. The NCBI BLAST page opens with its sequences and settings filled in; click{" "}
                  <b>BLAST</b> there.
                </li>
                <li>
                  When it finishes, choose <b>Download All → Single-file JSON</b> (XML works too).
                </li>
                <li>
                  Click <b>Import results file</b> here and pick the downloaded file. The results attach to the samples by name.
                </li>
              </ol>
            </div>
          </div>
        )}

        {slow.map((g) => {
          const end = deadline(g, giveUp);
          return (
            <div key={g.id} className="flex items-start gap-3 rounded-[10px] border border-warn bg-warn-soft p-4">
              <AlertTriangle size={18} className="mt-0.5 shrink-0 text-warn" />
              <div className="flex min-w-0 flex-1 flex-col gap-1">
                <div className="font-semibold text-warn">
                  NCBI is taking longer than usual for group {g.id} ({runTimeText(runTime(g, now) ?? 0)})
                </div>
                <div>
                  This is NCBI's queue, not your computer. A group usually takes 1–10 minutes. NCBI limits how much BLAST each
                  internet connection (IP address) can use, so searches slow down for large batches, and on a shared network such as
                  a lab or campus, where everyone's searches count against the same limit.
                  {groups.some((x) => x.status === "pending") && " The groups after it wait, because NCBI allows one search at a time."}{" "}
                  {end && (
                    <>
                      If it is not done by <b>{clock(end)}</b>, the app stops waiting and continues with the next group; you can
                      send this one again afterwards.
                    </>
                  )}
                </div>
                <div>
                  To get results sooner: run this group on the NCBI website and import the results file, or run BLAST on this
                  computer (Settings → BLAST). Fewer max hits (now {project.settings.blast.maxHits}) also make each search lighter.
                </div>
                <div className="mt-1 flex flex-wrap gap-2">
                  <button className="btn btn-sm" onClick={() => onWebsite(g)}>
                    <Globe size={13} /> Open on NCBI website
                  </button>
                  <button className="btn btn-sm" onClick={() => go("settings")}>
                    <Cpu size={13} /> Set up this computer
                  </button>
                  <button className="btn btn-sm" onClick={() => setResend(g)}>
                    <RotateCcw size={13} /> Send again
                  </button>
                  {g.rid && (
                    <button className="btn btn-sm" onClick={() => openUrl(ncbiPage(g.rid!))}>
                      <ExternalLink size={13} /> View on NCBI
                    </button>
                  )}
                </div>
              </div>
            </div>
          );
        })}

        {groups.length > 0 && (
          <div className="card overflow-hidden">
            <div className="grid grid-cols-[28px_120px_minmax(0,1fr)_120px_190px] gap-3 border-b border-line bg-panel-2 px-3 py-2 text-[12px] font-semibold text-muted">
              <div />
              <div>Group</div>
              <div>Progress</div>
              <div>Run time</div>
              <div />
            </div>
            {groups.map((g) => {
              const t = tone(g, now);
              const rt = runTime(g, now);
              const isOpen = !!open[g.id];
              return (
                <div key={g.id} className={clsx("border-b border-line/70", g === active && "bg-accent-soft/30")}>
                  <div className="grid grid-cols-[28px_120px_minmax(0,1fr)_120px_190px] items-center gap-3 px-3 py-2.5">
                    <div className="flex justify-center">
                      <Indicator g={g} now={now} />
                    </div>
                    <button
                      className="flex items-center gap-1 text-left"
                      onClick={() => setOpen({ ...open, [g.id]: !isOpen })}
                      title="Show the samples in this group"
                    >
                      {isOpen ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
                      <span className="tabular font-medium">{g.id}</span>
                      <span className="label">· {g.sampleIds.length}</span>
                    </button>
                    <div className="flex min-w-0 flex-col gap-1">
                      <StageTrack g={g} now={now} />
                      <Heartbeat g={g} now={now} />
                      {g.status === "pending" && project.blastRunning && active && (
                        <div className="text-muted">
                          Waits for group {active.id};{" "}
                          {active.source === "local" ? "BLAST+ runs one group at a time." : "NCBI allows one search at a time."}
                        </div>
                      )}
                      {g.status === "pending" && where === "website" && (
                        <div className="text-muted">Open it on the NCBI website, then import the downloaded results.</div>
                      )}
                      {g.error && <div className={clsx("truncate", t === "bad" ? "text-bad" : "text-warn")} title={g.error}>{g.error}</div>}
                    </div>
                    <div className={clsx("tabular", t === "slow" ? "font-semibold text-warn" : "text-muted")}>
                      {rt != null ? runTimeText(rt) : "–"}
                    </div>
                    <div className="flex flex-wrap justify-end gap-1.5">
                      {g.status !== "ready" && g.source !== "local" && (where === "website" || g.status !== "pending") && (
                        <button className="btn btn-sm" title="Open the NCBI BLAST website with this group's sequences filled in" onClick={() => onWebsite(g)}>
                          <Globe size={13} /> NCBI website
                        </button>
                      )}
                      {g.rid && g.status === "submitted" && (
                        <button className="btn btn-sm" title="Open NCBI's own page for this search" onClick={() => openUrl(ncbiPage(g.rid!))}>
                          <ExternalLink size={13} /> NCBI
                        </button>
                      )}
                      {canResend(g) && (
                        <button className="btn btn-sm" onClick={() => setResend(g)}>
                          <RotateCcw size={13} /> Send again
                        </button>
                      )}
                    </div>
                  </div>
                  {isOpen && (
                    <div className="flex flex-col gap-1 px-3 pb-3 pl-[52px] text-muted">
                      <div>
                        {g.rid ? (
                          <>
                            NCBI search ID <span className="tabular text-ink">{g.rid}</span>
                          </>
                        ) : (
                          "Not sent yet"
                        )}
                        {g.submittedAt && <> · sent {clock(Date.parse(g.submittedAt))}</>}
                        {g.finishedAt && <> · finished {clock(Date.parse(g.finishedAt))}</>}
                        {g.attempts > 1 && <> · sent {g.attempts} times</>}
                      </div>
                      <div className="flex flex-wrap gap-1">
                        {g.sampleIds.map((id) => (
                          <span key={id} className="rounded bg-panel-2 px-1.5 py-px text-[11.5px] text-ink">
                            {id}
                          </span>
                        ))}
                      </div>
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>

      <Modal
        open={!!resend}
        onOpenChange={(v) => !v && setResend(null)}
        title={`Send group ${resend?.id ?? ""} again?`}
        footer={
          <>
            <button className="btn" onClick={() => setResend(null)}>
              Cancel
            </button>
            <button className="btn btn-primary" onClick={confirmResend}>
              Send again
            </button>
          </>
        }
      >
        {resend && (
          <p className="m-0">
            {resend.status === "submitted" ? (
              <>
                NCBI has been working on these {resend.sampleIds.length} samples for {runTimeText(runTime(resend, now) ?? 0)}. Sending
                again starts a new search and the wait starts over; the current one is dropped.
              </>
            ) : (
              <>
                The {resend.sampleIds.length} samples in this group have no results.{" "}
                {where === "local"
                  ? "They will be searched again with BLAST+ on this computer."
                  : where === "website"
                    ? "The group goes back to the list, to run on the NCBI website."
                    : "They will be sent to NCBI again as a new search."}
              </>
            )}
          </p>
        )}
      </Modal>
    </div>
  );
}
