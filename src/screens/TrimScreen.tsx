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
    if (!read) return;
    api.getTrace(read.id).then(setTrace).catch((e) => toast.error(errorText(e)));
  }, [read?.id]);

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
    setPreview(null);
  };

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
              {read.fileName} · raw {read.rawLength} bases, {read.rawHqPct.toFixed(1)}% HQ
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <select
              className="input"
              value={read.directionManual ?? "auto"}
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
          <Stat label="% HQ kept" value={`${d.hqPct.toFixed(1)}%`} tone={hqTone} />
          <Stat label="Stop codons" value={d.stopCodons} tone={d.stopCodons > 0 ? "warn" : "ok"} />
          <Stat label="Direction" value={d.direction ?? "unknown"} tone={d.direction ? undefined : "warn"} />
          <div className="min-w-[200px]">
            <div className="label">Status</div>
            <div className="mt-0.5">
              <Flags flags={d.flags} wrap />
            </div>
          </div>
        </div>

        {read.derived.qcFailed && (
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

        {trace ? (
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

        <div className="mt-5 flex items-center justify-between">
          <div className="font-semibold">Final sequence (5'→3' along COI, {d.finalLen} bp)</div>
          <label className="flex items-center gap-2">
            <input type="checkbox" checked={!read.excluded} onChange={(e) => run(() => api.setInclusion([read.id], undefined, !e.target.checked))} />
            Include in BLAST
          </label>
        </div>
        <pre className="card mt-2 max-h-[180px] overflow-auto p-3 font-mono text-[12px] leading-5 break-all whitespace-pre-wrap select-text">
          {d.finalSeq || "(empty)"}
        </pre>
      </div>
    </div>
  );
}
