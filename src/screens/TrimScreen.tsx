import { useEffect, useMemo, useRef, useState } from "react";
import { RotateCcw } from "lucide-react";
import { toast } from "sonner";
import { api, errorText } from "../api";
import { useApp } from "../state";
import { Chromatogram } from "../components/Chromatogram";
import { Badge, Empty, Flags, Stat } from "../components/ui";
import { VirtualTable, type Column } from "../components/VirtualTable";
import type { Derived, Dir, Range, ReadView, TraceView } from "../types";

function siteText(s: ReadView["primer"]["startSite"]) {
  if (!s) return "not found";
  return s.partial ? `${s.primer}, partial (${s.matched} bp, ${s.mismatches} edits)` : `${s.primer} (${s.mismatches} mismatches)`;
}

export default function TrimScreen() {
  const { project, selectedId, select, run } = useApp();
  const read = project?.reads.find((r) => r.id === selectedId) ?? project?.reads[0] ?? null;
  const [trace, setTrace] = useState<TraceView | null>(null);
  const [dragRange, setDragRange] = useState<Range | null>(null);
  const [preview, setPreview] = useState<Derived | null>(null);
  const inflight = useRef(false);
  const queued = useRef<Range | null>(null);

  useEffect(() => {
    setTrace(null);
    setDragRange(null);
    setPreview(null);
    if (!read?.hasTrace) return;
    api.getTrace(read.id).then(setTrace).catch((e) => toast.error(errorText(e)));
  }, [read?.id]);

  // A forward read in a pair holds the consensus; show this read's own stats instead.
  const committedKey = read ? `${read.id}:${(read.manualTrim ?? read.autoTrim).start}:${(read.manualTrim ?? read.autoTrim).end}` : "";
  useEffect(() => {
    if (!read?.pair) return;
    const t = read.manualTrim ?? read.autoTrim;
    api.previewTrim(read.id, t.start, t.end).then(setPreview).catch(() => {});
  }, [committedKey, read?.pair?.reverseId]);

  const listColumns: Column<ReadView>[] = useMemo(
    () => [
      { key: "id", header: "Sample", width: 104, render: (r) => <span className="font-medium">{r.id}</span>, sort: (r) => r.id },
      { key: "len", header: "Kept", width: 52, align: "right", render: (r) => r.derived.finalLen, sort: (r) => r.derived.finalLen },
      {
        key: "st",
        header: "QC",
        width: 70,
        render: (r) =>
          r.derived.flags.length === 0 ? (
            <Badge tone="ok">OK</Badge>
          ) : r.derived.qcFailed ? (
            <Badge tone="bad">Failed</Badge>
          ) : (
            <Badge tone="warn">{r.derived.flags.length} flag{r.derived.flags.length > 1 ? "s" : ""}</Badge>
          ),
        sort: (r) => (r.derived.qcFailed ? 2 : r.derived.flags.length ? 1 : 0),
      },
    ],
    [],
  );

  if (!project) return null;
  if (!read) return <Empty>No samples.</Empty>;

  const committed = read.manualTrim ?? read.autoTrim;
  const range = dragRange ?? read.derived.trim ?? committed;
  const d = preview ?? read.derived;

  // Ask the backend for stats while dragging; at most one request in flight.
  const requestPreview = (r: Range) => {
    queued.current = r;
    if (inflight.current) return;
    const next = () => {
      const q = queued.current;
      if (!q) {
        inflight.current = false;
        return;
      }
      queued.current = null;
      inflight.current = true;
      api
        .previewTrim(read.id, q.start, q.end)
        .then(setPreview)
        .catch(() => {})
        .finally(next);
    };
    next();
  };

  const commit = async (r: Range) => {
    await run(() => api.setTrim(read.id, r.start, r.end));
    setDragRange(null);
    if (!read.pair) setPreview(null);
  };
  const inPair = !!read.pair || !!read.pairedWith;
  const partnerId = read.pair?.reverseId ?? read.pairedWith;
  const sample = read.pair ? read : project.reads.find((r) => r.id === read.pairedWith) ?? null;

  const hqTone = d.hqPct > project.settings.hqThreshold ? "ok" : "bad";

  return (
    <div className="flex h-full">
      <div className="flex w-[256px] shrink-0 flex-col border-r border-line p-3">
        <div className="label mb-2 px-1">{project.reads.length} samples</div>
        <div className="min-h-0 flex-1">
          <VirtualTable rows={project.reads} columns={listColumns} rowKey={(r) => r.id} selectedKey={read.id} onRowClick={(r) => select(r.id)} rowHeight={32} />
        </div>
      </div>

      <div className="min-w-0 flex-1 overflow-auto p-6">
        <div className="mb-4 flex flex-wrap items-start justify-between gap-3">
          <div>
            <h1 className="m-0 text-[19px] font-semibold">{read.id}</h1>
            <div className="text-muted">
              {read.fileName} · raw {read.rawLength} bases, {read.hasTrace ? `${read.rawHqPct.toFixed(1)}% HQ` : "from FASTA"}
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <select
              className="input"
              value={read.directionManual ?? "auto"}
              disabled={inPair}
              title={inPair ? "Direction is set by the pair; use Swap F/R on the Import screen" : undefined}
              onChange={(e) => run(() => api.setDirection(read.id, e.target.value === "auto" ? null : (e.target.value as Dir)))}
            >
              <option value="auto">Direction: auto ({read.directionAuto ?? "unknown"})</option>
              <option value="F">Direction: forward</option>
              <option value="R">Direction: reverse (reverse-complement)</option>
            </select>
            <button
              className="btn"
              disabled={!read.manualTrim && !read.directionManual}
              onClick={() => run(() => api.resetRead(read.id), "Automatic limits restored")}
            >
              <RotateCcw size={14} /> Reset
            </button>
          </div>
        </div>

        <div className="card mb-4 flex flex-wrap gap-x-8 gap-y-3 px-4 py-3">
          <Stat label="Kept bases" value={`${range.start + 1}–${range.end}`} />
          <Stat
            label="Length"
            value={`${d.finalLen} bp`}
            tone={d.finalLen < project.settings.singleLenMin || d.finalLen > project.settings.singleLenMax ? "warn" : undefined}
          />
          {read.hasTrace && <Stat label="% HQ kept" value={`${d.hqPct.toFixed(1)}%`} tone={hqTone} />}
          <Stat label="Stop codons" value={d.stopCodons} tone={d.stopCodons > 0 ? "warn" : "ok"} />
          <Stat label="Direction" value={d.direction ?? "unknown"} tone={d.direction ? undefined : "warn"} />
          <div className="min-w-[200px]">
            <div className="label">Status</div>
            <div className="mt-0.5">
              <Flags flags={d.flags} wrap />
            </div>
          </div>
        </div>

        {inPair && sample?.pair && (
          <div className="card mb-4 flex flex-wrap items-center gap-x-4 gap-y-1 border-accent bg-accent-soft px-4 py-2.5">
            <span>
              {read.pair ? "Forward read" : "Reverse read"} of pair <b>{sample.id}</b> + <b>{sample.pair.reverseId}</b>.
            </span>
            {sample.pair.noOverlap ? (
              <span className="text-bad">The two reads do not overlap; choose a read on the Import screen.</span>
            ) : (
              <span className="text-muted">
                Consensus {sample.derived.finalLen} bp, {sample.derived.hqPct.toFixed(1)}% HQ · overlap {sample.pair.overlap} bp ·{" "}
                {sample.pair.disagreements} disagreements
                {sample.pair.conflicts.length > 0 && (
                  <span className="text-warn">
                    {" "}
                    · conflicts at consensus position{sample.pair.conflicts.length > 1 ? "s" : ""}{" "}
                    {sample.pair.conflicts.slice(0, 8).map((c) => c + 1).join(", ")}
                    {sample.pair.conflicts.length > 8 ? "…" : ""}
                  </span>
                )}
              </span>
            )}
            {partnerId && (
              <button className="btn btn-sm ml-auto" onClick={() => select(partnerId)}>
                Open {partnerId}
              </button>
            )}
          </div>
        )}

        {read.derived.qcFailed && !inPair && (
          <div className="card mb-4 flex items-center justify-between gap-3 border-bad bg-bad-soft px-4 py-2.5">
            <div className="text-bad">
              Failed QC: no region of at least {project.settings.minLength} bases is above {project.settings.hqThreshold}% high
              quality. It is left out of BLAST unless you include it.
            </div>
            <label className="flex shrink-0 items-center gap-2">
              <input
                type="checkbox"
                checked={read.includeFailed}
                onChange={(e) => run(() => api.setInclusion([read.id], e.target.checked, undefined))}
              />
              Include anyway
            </label>
          </div>
        )}

        {!read.hasTrace ? (
          <div className="card px-4 py-3">
            <div className="mb-2 text-muted">
              Imported from FASTA, so there is no chromatogram and no quality trimming. Primers are still removed.
            </div>
            <div className="break-all font-mono text-[12px] leading-5">{d.finalSeq}</div>
          </div>
        ) : trace ? (
          <Chromatogram
            trace={trace}
            range={range}
            primer={read.primer}
            hqQuality={project.settings.hqQuality}
            onDrag={(r) => {
              setDragRange(r);
              requestPreview(r);
            }}
            onCommit={commit}
          />
        ) : (
          <div className="card flex h-[260px] items-center justify-center text-muted">Loading chromatogram…</div>
        )}

        <div className="mt-5 grid grid-cols-[1fr_auto] gap-x-6 gap-y-1">
          <div className="label">Primer site at read start</div>
          <div>{siteText(read.primer.startSite)}</div>
          <div className="label">Primer site at read end</div>
          <div>{siteText(read.primer.endSite)}</div>
          <div className="label">Automatic limits</div>
          <div className="tabular">
            {read.autoTrim.start + 1}–{read.autoTrim.end}
            {read.manualTrim && <span className="ml-2 text-muted">(changed by hand)</span>}
          </div>
        </div>

        {(() => {
          // For a pair, the sequence that is searched is the consensus, stored on the forward read.
          const shown = sample?.pair && !sample.pair.noOverlap ? sample.derived : d;
          const owner = sample?.pair ? sample : read;
          return (
            <>
              <div className="mt-5 flex items-center justify-between">
                <div className="font-semibold">
                  {sample?.pair && !sample.pair.noOverlap ? "Consensus" : "Final sequence"} (5'→3' along COI, {shown.finalLen} bp)
                </div>
                <label className="flex items-center gap-2">
                  <input
                    type="checkbox"
                    checked={!owner.excluded}
                    onChange={(e) => run(() => api.setInclusion([owner.id], undefined, !e.target.checked))}
                  />
                  Include in BLAST
                </label>
              </div>
              <pre className="card mt-2 max-h-[180px] overflow-auto p-3 font-mono text-[12px] leading-5 break-all whitespace-pre-wrap select-text">
                {shown.finalSeq || "(empty)"}
              </pre>
            </>
          );
        })()}
      </div>
    </div>
  );
}
