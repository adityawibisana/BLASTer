import { useMemo, useState } from "react";
import { ArrowRight, CheckCircle2 } from "lucide-react";
import { api } from "../api";
import { useApp } from "../state";
import { Badge, Flags, PageHeader } from "../components/ui";
import { PairsPanel } from "../components/PairsPanel";
import { readsLabel } from "../lib/format";
import { VirtualTable, type Column } from "../components/VirtualTable";
import type { Dir, ReadView } from "../types";

const dirLabel = (d: Dir | null) => (d === "F" ? "Forward" : d === "R" ? "Reverse" : "Unknown");

export default function ImportScreen() {
  const { project, run, go, select } = useApp();
  const [confirmed, setConfirmed] = useState(false);
  const columns: Column<ReadView>[] = useMemo(
    () => [
      { key: "id", header: "Sample", width: 120, render: (r) => <span className="font-medium">{r.id}</span>, sort: (r) => r.id },
      { key: "flags", header: "QC status", width: 170, render: (r) => <Flags flags={r.derived.flags} /> },
      { key: "file", header: "File", width: 120, render: (r) => <span className="text-muted">{r.fileName}</span> },
      { key: "raw", header: "Raw length", width: 90, align: "right", render: (r) => r.rawLength, sort: (r) => r.rawLength },
      { key: "hq", header: "% HQ raw", width: 84, align: "right", render: (r) => r.rawHqPct.toFixed(1), sort: (r) => r.rawHqPct },
      { key: "gc", header: "% GC", width: 64, align: "right", render: (r) => r.rawGcPct.toFixed(1), sort: (r) => r.rawGcPct },
      { key: "amb", header: "Ambig.", width: 64, align: "right", render: (r) => r.rawAmbiguous },
      {
        key: "primer",
        header: "Primer sites",
        width: 150,
        render: (r) => (
          <span className="text-muted">
            {r.primer.startSite?.primer ?? "–"} / {r.primer.endSite?.primer ?? "–"}
          </span>
        ),
      },
      {
        key: "dir",
        header: "Direction",
        width: 168,
        render: (r) =>
          r.pair || r.pairedWith ? (
            <span className="text-muted">{readsLabel(r)}</span>
          ) : (
          <select
            className="input h-7 w-full py-0"
            value={r.directionManual ?? "auto"}
            onClick={(e) => e.stopPropagation()}
            onChange={(e) => {
              const v = e.target.value;
              run(() => api.setDirection(r.id, v === "auto" ? null : (v as Dir)));
            }}
          >
            <option value="auto">Auto: {dirLabel(r.directionAuto)}</option>
            <option value="F">Forward</option>
            <option value="R">Reverse</option>
          </select>
        ),
      },
      {
        key: "final",
        header: "Kept",
        width: 70,
        align: "right",
        render: (r) => r.derived.finalLen,
        sort: (r) => r.derived.finalLen,
      },
    ],
    [run],
  );
  if (!project) return null;
  const reads = project.reads;
  const passed = reads.filter((r) => r.derived.eligible).length;
  const failed = reads.filter((r) => r.derived.qcFailed).length;
  const unknownDir = reads.filter((r) => r.derived.direction === null).length;

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title="Import"
        subtitle={
          <>
            {reads.length} samples from <span className="font-medium text-ink">{project.sourceZip.split(/[\\/]/).pop()}</span> · {passed}{" "}
            pass QC · {failed} failed QC{unknownDir > 0 && ` · ${unknownDir} with unknown direction`}
          </>
        }
        actions={
          <button className="btn btn-primary" onClick={() => go("trim")}>
            Review trims <ArrowRight size={15} />
          </button>
        }
      />
      <div className="px-6 pb-3">
        <div className="card flex flex-wrap items-center gap-x-6 gap-y-2 px-4 py-3">
          <div className="font-semibold">Primer auto-detect</div>
          {project.primerSummary.map((p) => (
            <div key={p.setName ?? "none"} className="flex items-center gap-1.5">
              <Badge tone={p.setName ? "ok" : "warn"}>{p.setName ?? "none found"}</Badge>
              <span className="tabular">{p.reads} reads</span>
            </div>
          ))}
          <div className="ml-auto flex items-center gap-2">
            {confirmed ? (
              <span className="flex items-center gap-1 text-ok">
                <CheckCircle2 size={15} /> Confirmed
              </span>
            ) : (
              <>
                <button className="btn btn-sm" onClick={() => go("settings")}>
                  Change primer option
                </button>
                <button className="btn btn-sm btn-primary" onClick={() => setConfirmed(true)}>
                  Looks right
                </button>
              </>
            )}
          </div>
        </div>
        {project.settings.primerMode === "auto" && (
          <div className="label mt-1.5 px-1">
            Each read uses the primer set that matches it best. Sets that bind the same site (e.g. Folmer and its degenerate
            versions) can both appear.
          </div>
        )}
        <div className="mt-3">
          <PairsPanel />
        </div>
        {project.skipped.length > 0 && (
          <div className="card mt-3 border-warn bg-warn-soft px-4 py-2.5">
            <div className="font-semibold text-warn">{project.skipped.length} files skipped</div>
            {project.skipped.map((s) => (
              <div key={s.fileName} className="text-[12.5px]">
                {s.fileName}: {s.reason}
              </div>
            ))}
          </div>
        )}
      </div>
      <div className="min-h-0 flex-1 px-6 pb-6">
        <VirtualTable
          rows={reads}
          columns={columns}
          rowKey={(r) => r.id}
          onRowClick={(r) => {
            select(r.id);
            go("trim");
          }}
        />
      </div>
    </div>
  );
}
