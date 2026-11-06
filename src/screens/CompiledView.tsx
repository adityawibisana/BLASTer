import { useMemo, useState } from "react";
import { Bar, BarChart, CartesianGrid, Cell, LabelList, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { X } from "lucide-react";
import { useApp } from "../state";
import { Badge, Flags } from "../components/ui";
import { VirtualTable, type Column } from "../components/VirtualTable";
import { REASON_LABEL, chartRows, cover, pct, shortDate } from "../lib/format";
import type { CompiledRow } from "../types";

export default function CompiledView({ onCompile, onOpenSample }: { onCompile: () => void; onOpenSample: (id: string) => void }) {
  const { project } = useApp();
  const [filter, setFilter] = useState<string | null>(null);
  const [expanded, setExpanded] = useState(false);
  const compiled = project?.compiled ?? null;

  const { rows: chartData, other } = useMemo(() => chartRows(compiled?.chart ?? [], 15, expanded), [compiled, expanded]);
  const bars = useMemo(() => chartData.map((b) => ({ ...b, label: `${b.count} (${b.pct.toFixed(0)}%)` })), [chartData]);
  const otherNames = useMemo(() => new Set(other.map((o) => o.species)), [other]);

  const columns: Column<CompiledRow>[] = useMemo(
    () => [
      { key: "id", header: "Sample", width: 112, render: (r) => <span className="font-medium">{r.sampleId}</span>, sort: (r) => r.sampleId },
      { key: "st", header: "Status", width: 150, render: (r) => <Flags flags={r.flags} /> },
      {
        key: "sp",
        header: "Closest species",
        width: 230,
        render: (r) => (r.species ? <i>{r.species}</i> : <span className="text-muted">{REASON_LABEL[r.reason ?? ""] ?? ""}</span>),
        sort: (r) => r.species,
      },
      { key: "id%", header: "% Identity", width: 84, align: "right", render: (r) => pct(r.identity), sort: (r) => r.identity },
      { key: "cov", header: "Query Cover", width: 92, align: "right", render: (r) => cover(r.queryCover), sort: (r) => r.queryCover },
      { key: "acc", header: "Accession", width: 104, render: (r) => r.accession ?? "" },
      { key: "ru", header: "Runner-up species", width: 200, render: (r) => <i className="text-muted">{r.runnerUp ?? ""}</i>, sort: (r) => r.runnerUp },
      { key: "ru%", header: "Runner-up %", width: 92, align: "right", render: (r) => pct(r.runnerUpIdentity), sort: (r) => r.runnerUpIdentity },
      { key: "n", header: "Hits counted", width: 92, align: "right", render: (r) => r.hitsCounted, sort: (r) => r.hitsCounted },
    ],
    [],
  );

  if (!project) return null;
  if (!compiled) {
    return (
      <div className="card flex flex-col items-start gap-3 p-6">
        <div className="text-[15px] font-semibold">Which species is closest for each sample?</div>
        <div className="max-w-[640px] text-muted">
          Compile looks at every hit of every sample and picks the highest % identity among hits that cover at least{" "}
          {project.settings.compileMinCover}% of the sample. Exact ties go to higher query cover, then higher score, then the
          species with more records. Every sample gets one answer.
        </div>
        <button className="btn btn-primary" onClick={onCompile}>
          Compile results
        </button>
      </div>
    );
  }

  const rows = filter
    ? compiled.rows.filter((r) => (filter === "__other" ? r.species && otherNames.has(r.species) : r.species === filter))
    : compiled.rows;
  const total = compiled.chart.reduce((a, b) => a + b.count, 0);
  const height = Math.max(160, bars.length * 30 + 40);

  return (
    <div className="flex flex-col gap-4">
      {project.recompileNeeded && (
        <div className="card flex items-center justify-between border-warn bg-warn-soft px-4 py-2.5 text-warn">
          Recompile needed: results changed after this compile.
          <button className="btn btn-sm" onClick={onCompile}>
            Recompile
          </button>
        </div>
      )}
      <div className="card p-4">
        <div className="mb-1 flex items-baseline justify-between">
          <div className="font-semibold">Closest species per sample</div>
          <div className="label">
            {total} samples compiled · {compiled.chart.length} species · {shortDate(compiled.createdAt)} · query cover ≥ {compiled.minCover}%
          </div>
        </div>
        <div className="label mb-2">Click a bar to list its samples.</div>
        <div style={{ height }}>
          <ResponsiveContainer width="100%" height="100%">
            <BarChart data={bars} layout="vertical" margin={{ top: 4, right: 110, bottom: 4, left: 8 }}>
              <CartesianGrid horizontal={false} stroke="var(--border)" />
              <XAxis type="number" allowDecimals={false} tick={{ fill: "var(--muted)", fontSize: 12 }} stroke="var(--border)" />
              <YAxis
                type="category"
                dataKey="species"
                width={240}
                tick={{ fill: "var(--text)", fontSize: 12.5, fontStyle: "italic" }}
                stroke="var(--border)"
                interval={0}
              />
              <Tooltip
                cursor={{ fill: "var(--panel-2)" }}
                contentStyle={{ background: "var(--panel)", border: "1px solid var(--border)", borderRadius: 8, color: "var(--text)" }}
                formatter={(v) => [`${v} samples`, "Closest match"]}
              />
              <Bar
                dataKey="count"
                radius={[0, 4, 4, 0]}
                cursor="pointer"
                onClick={(d) => {
                  const sp = (d as { payload?: { species?: string } }).payload?.species ?? "";
                  if (sp.startsWith("Other (")) {
                    setExpanded(true);
                    return;
                  }
                  setFilter((f) => (f === sp ? null : sp));
                }}
              >
                {bars.map((b) => (
                  <Cell key={b.species} fill={filter && filter !== b.species ? "var(--border)" : "var(--accent)"} />
                ))}
                <LabelList dataKey="label" position="right" fill="var(--muted)" fontSize={12} />
              </Bar>
            </BarChart>
          </ResponsiveContainer>
        </div>
        {expanded && compiled.chart.length > 15 && (
          <button className="btn btn-sm mt-2" onClick={() => setExpanded(false)}>
            Show top 15 only
          </button>
        )}
      </div>

      <div>
        <div className="mb-2 flex items-center gap-2">
          <div className="font-semibold">Per-sample result</div>
          {filter && (
            <Badge tone="info">
              <span className="flex items-center gap-1">
                <i>{filter}</i>
                <button onClick={() => setFilter(null)} title="Clear filter">
                  <X size={12} />
                </button>
              </span>
            </Badge>
          )}
        </div>
        <div style={{ height: Math.min(560, 40 + rows.length * 34 + 4) }}>
          <VirtualTable rows={rows} columns={columns} rowKey={(r) => r.sampleId} onRowClick={(r) => onOpenSample(r.sampleId)} />
        </div>
      </div>
    </div>
  );
}
