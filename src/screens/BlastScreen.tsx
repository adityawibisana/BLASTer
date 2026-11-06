import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Loader2, Play, RefreshCw, Square } from "lucide-react";
import { toast } from "sonner";
import { api, errorText } from "../api";
import { useApp } from "../state";
import { Badge, Field, Modal, PageHeader, Stat } from "../components/ui";
import { duration, isNcbiOffPeak, shortDate } from "../lib/format";
import type { BlastSettings, GroupStatus, GroupView } from "../types";

const STATUS: Record<GroupStatus, { label: string; tone: "ok" | "warn" | "bad" | "info" }> = {
  pending: { label: "Waiting to submit", tone: "info" },
  submitted: { label: "Searching at NCBI", tone: "warn" },
  ready: { label: "Done", tone: "ok" },
  failed: { label: "Failed", tone: "bad" },
  expired: { label: "Expired", tone: "bad" },
  cancelled: { label: "Cancelled", tone: "info" },
};

function useNow(active: boolean) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    if (!active) return;
    const t = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(t);
  }, [active]);
  return now;
}

/** Estimated finish from the average duration of finished groups. */
function estimate(groups: GroupView[], now: number) {
  const done = groups.filter((g) => g.status === "ready" && g.submittedAt && g.finishedAt);
  const remaining = groups.filter((g) => g.status === "pending" || g.status === "submitted").length;
  if (!remaining) return null;
  const avg = done.length
    ? done.reduce((a, g) => a + (Date.parse(g.finishedAt!) - Date.parse(g.submittedAt!)), 0) / done.length
    : 4 * 60_000; // before any group finishes, assume ~4 min per group
  return { remaining, finishAt: now + avg * remaining, guessed: done.length === 0 };
}

export default function BlastScreen() {
  const { t } = useTranslation();
  const { project, run, go, setProject } = useApp();
  const [emailOpen, setEmailOpen] = useState(false);
  const [email, setEmail] = useState("");
  const [pendingMode, setPendingMode] = useState<boolean | null>(null);
  const now = useNow(!!project?.blastRunning);
  const groups = project?.groups ?? [];
  const est = useMemo(() => estimate(groups, now), [groups, now]);
  if (!project) return null;

  const s = project.settings;
  const eligible = project.reads.filter((r) => r.derived.eligible);
  const needSearch = eligible.filter((r) => !r.searched || r.outdated).length;
  const searched = eligible.filter((r) => r.searched && !r.outdated).length;
  const failedGroups = groups.filter((g) => g.status === "failed" || g.status === "expired" || g.status === "cancelled");
  const offPeak = isNcbiOffPeak(new Date(now));

  const updateBlast = (patch: Partial<BlastSettings>) =>
    run(async () => setProject(await api.updateSettings({ ...s, blast: { ...s.blast, ...patch } })));

  const start = async (onlyChanged: boolean) => {
    try {
      const r = await api.startBlast(onlyChanged);
      toast.success(r.queuedSamples ? `${r.queuedSamples} samples queued in ${r.groups} group(s)` : "Nothing new to search; resuming open searches");
    } catch (e) {
      if (errorText(e) === "NO_EMAIL") {
        setPendingMode(onlyChanged);
        setEmailOpen(true);
        return;
      }
      toast.error(errorText(e));
    }
  };

  const saveEmail = async () => {
    const value = email.trim();
    if (!/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(value)) {
      toast.error("Please enter a valid email address.");
      return;
    }
    const p = await api.updateSettings({ ...s, email: value });
    setProject(p);
    const defaults = await api.getDefaults();
    await api.saveDefaults({ ...defaults, email: value });
    setEmailOpen(false);
    if (pendingMode !== null) start(pendingMode);
  };

  const ready = groups.filter((g) => g.status === "ready").length;
  const open = groups.filter((g) => g.status === "pending" || g.status === "submitted").length;

  return (
    <div className="flex h-full flex-col overflow-auto">
      <PageHeader
        title="BLAST"
        subtitle="Searches go to NCBI's online BLAST service, in groups, within NCBI's usage limits."
        actions={
          project.blastRunning ? (
            <button className="btn" onClick={() => api.cancelBlast()}>
              <Square size={14} /> {t("blast.cancel")}
            </button>
          ) : (
            <>
              <button className="btn" disabled={needSearch === 0} onClick={() => start(true)}>
                <RefreshCw size={14} /> {t("blast.searchChanged")} ({needSearch})
              </button>
              <button className="btn btn-primary" disabled={eligible.length === 0} onClick={() => start(false)}>
                <Play size={14} /> {t("blast.searchAll")} ({eligible.length})
              </button>
            </>
          )
        }
      />

      <div className="grid grid-cols-[minmax(0,1fr)_340px] gap-4 px-6 pb-6">
        <div className="flex min-w-0 flex-col gap-4">
          <div className="card flex flex-wrap gap-x-8 gap-y-3 px-4 py-3">
            <Stat label="Eligible samples" value={eligible.length} />
            <Stat label="With current results" value={searched} tone={searched === eligible.length && searched > 0 ? "ok" : undefined} />
            <Stat label="New or changed" value={needSearch} tone={needSearch ? "warn" : undefined} />
            <Stat label="Groups done" value={`${ready} / ${groups.length}`} />
            {est && (
              <Stat
                label={est.guessed ? "Estimated finish (rough)" : "Estimated finish"}
                value={new Date(est.finishAt).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" })}
              />
            )}
          </div>

          {project.blastRunning && (
            <div className="card flex items-center gap-2 border-accent px-4 py-2.5">
              <Loader2 size={16} className="animate-spin text-accent" />
              Searching: {open} group(s) left. You can keep reviewing finished samples under Results; closing the app is safe, the search resumes next time.
            </div>
          )}
          {!offPeak && needSearch > 100 && (
            <div className="card border-warn bg-warn-soft px-4 py-2.5 text-warn">
              This batch has more than 100 samples to search. NCBI asks for large batches to run on weekends or between 9 pm
              and 5 am US Eastern time (9 am–5 pm WIB).
            </div>
          )}
          {failedGroups.length > 0 && !project.blastRunning && (
            <div className="card flex items-center justify-between gap-3 border-bad bg-bad-soft px-4 py-2.5">
              <span className="text-bad">
                {failedGroups.length} group(s) failed, expired or were cancelled. Their samples have no results yet.
              </span>
              <button className="btn btn-sm" onClick={() => start(true)}>
                Resubmit
              </button>
            </div>
          )}

          <div className="card overflow-hidden">
            <div className="grid grid-cols-[70px_80px_150px_130px_minmax(0,1fr)] border-b border-line bg-panel-2 px-3 py-2 text-[12px] font-semibold text-muted">
              <div>Group</div>
              <div>Samples</div>
              <div>Status</div>
              <div>RID</div>
              <div>Time / message</div>
            </div>
            {groups.length === 0 && <div className="p-5 text-muted">No searches yet.</div>}
            <div className="max-h-[420px] overflow-auto">
              {[...groups].reverse().map((g) => {
                const st = STATUS[g.status];
                const elapsed = g.submittedAt && g.status === "submitted" ? duration(now - Date.parse(g.submittedAt)) : null;
                return (
                  <div key={g.id} className="grid grid-cols-[70px_80px_150px_130px_minmax(0,1fr)] items-center border-b border-line/70 px-3 py-2">
                    <div className="tabular">{g.id}</div>
                    <div className="tabular">{g.sampleIds.length}</div>
                    <div className="flex items-center gap-1.5">
                      <Badge tone={st.tone}>{st.label}</Badge>
                      {g.status === "submitted" && <Loader2 size={13} className="animate-spin text-muted" />}
                    </div>
                    <div className="tabular text-muted">{g.rid ?? "–"}</div>
                    <div className="truncate text-muted" title={g.error ?? ""}>
                      {g.error ? <span className="text-warn">{g.error}</span> : elapsed ? `running ${elapsed}` : shortDate(g.finishedAt ?? g.submittedAt)}
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
          {ready > 0 && (
            <div>
              <button className="btn btn-primary" onClick={() => go("results")}>
                Open results
              </button>
            </div>
          )}
        </div>

        <div className="card flex flex-col gap-3 self-start p-4">
          <div className="font-semibold">Search settings</div>
          <div className="grid grid-cols-[110px_1fr] gap-x-3 gap-y-1.5">
            <span className="label">Program</span>
            <span>blastn</span>
            <span className="label">Database</span>
            <span>core_nt (Core nucleotide)</span>
            <span className="label">Max hits</span>
            <span>{s.blast.maxHits} per sample</span>
            <span className="label">NCBI email</span>
            <span className="truncate">{s.email || <span className="text-warn">not set</span>}</span>
          </div>
          <Field label="Optimize for">
            <select className="input" value={s.blast.megablast ? "megablast" : "blastn"} onChange={(e) => updateBlast({ megablast: e.target.value === "megablast" })} disabled={project.blastRunning}>
              <option value="megablast">Highly similar sequences (megablast)</option>
              <option value="blastn">Somewhat similar sequences (blastn)</option>
            </select>
          </Field>
          <Field label="Organism (optional)" hint="Common name, binomial or taxid, e.g. Culicidae">
            <input
              className="input"
              defaultValue={s.blast.organism}
              disabled={project.blastRunning}
              onBlur={(e) => e.target.value !== s.blast.organism && updateBlast({ organism: e.target.value })}
            />
          </Field>
          <label className="flex items-center gap-2">
            <input type="checkbox" checked={s.blast.excludeModels} disabled={project.blastRunning} onChange={(e) => updateBlast({ excludeModels: e.target.checked })} />
            Exclude models (XM/XP)
          </label>
          <label className="flex items-center gap-2">
            <input
              type="checkbox"
              checked={s.blast.excludeUncultured}
              disabled={project.blastRunning}
              onChange={(e) => updateBlast({ excludeUncultured: e.target.checked })}
            />
            Exclude uncultured/environmental samples
          </label>
          <Field label="Samples per submission" hint="1000 samples at 20 per group = 50 submissions, one after another">
            <input
              className="input"
              type="number"
              min={1}
              max={100}
              defaultValue={s.blast.groupSize}
              disabled={project.blastRunning}
              onBlur={(e) => {
                const v = Math.max(1, Math.min(100, Number(e.target.value) || 20));
                if (v !== s.blast.groupSize) updateBlast({ groupSize: v });
              }}
            />
          </Field>
        </div>
      </div>

      <Modal
        open={emailOpen}
        onOpenChange={setEmailOpen}
        title="Email for NCBI"
        footer={
          <>
            <button className="btn" onClick={() => setEmailOpen(false)}>
              Cancel
            </button>
            <button className="btn btn-primary" onClick={saveEmail}>
              Save and search
            </button>
          </>
        }
      >
        <p className="mt-0 text-muted">
          NCBI requires an email address with every BLAST request, so they can contact you if a search causes problems. It is
          sent only to NCBI and saved in Settings.
        </p>
        <input className="input w-full" autoFocus placeholder="name@institution.ac.id" value={email} onChange={(e) => setEmail(e.target.value)} onKeyDown={(e) => e.key === "Enter" && saveEmail()} />
      </Modal>
    </div>
  );
}
