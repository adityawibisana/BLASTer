import { useEffect, useMemo, useState } from "react";
import * as Tabs from "@radix-ui/react-tabs";
import { useTranslation } from "react-i18next";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Sparkles } from "lucide-react";
import { toast } from "sonner";
import clsx from "clsx";
import { api, errorText } from "../api";
import { useApp } from "../state";
import { Badge, Empty, Flags, PageHeader } from "../components/ui";
import { VirtualTable, type Column } from "../components/VirtualTable";
import { alignmentBlocks, cover, evalue, pct, readsLabel, score, shortDate } from "../lib/format";
import CompiledView from "./CompiledView";
import type { HitDetail, HitRow, ReadView, SampleResult } from "../types";

const genbank = (acc: string) => openUrl(`https://www.ncbi.nlm.nih.gov/nuccore/${encodeURIComponent(acc)}`);

function AccessionLink({ acc }: { acc: string }) {
  return (
    <button
      className="text-accent underline-offset-2 hover:underline"
      onClick={(e) => {
        e.stopPropagation();
        genbank(acc);
      }}
      title="Open in GenBank"
    >
      {acc}
    </button>
  );
}

export default function ResultsScreen() {
  const { t } = useTranslation();
  const { project, selectedId, select, run, setProject } = useApp();
  const [tab, setTab] = useState("overview");

  const overviewColumns: Column<ReadView>[] = useMemo(
    () => [
      { key: "id", header: "Sample", width: 112, render: (r) => <span className="font-medium">{r.id}</span>, sort: (r) => r.id },
      { key: "status", header: "Status", width: 170, render: (r) => <Flags flags={r.flags} />, sort: (r) => r.flags.length },
      { key: "reads", header: "Reads", width: 92, render: (r) => readsLabel(r), sort: (r) => readsLabel(r) },
      {
        key: "primers",
        header: "Primers",
        width: 150,
        render: (r) => (
          <span className="text-muted">
            {r.primer.startSite?.primer ?? "none"} / {r.primer.endSite?.primer ?? "none"}
          </span>
        ),
      },
      { key: "len", header: "Final length", width: 92, align: "right", render: (r) => r.derived.finalLen, sort: (r) => r.derived.finalLen },
      { key: "hq", header: "% HQ", width: 64, align: "right", render: (r) => (r.hasTrace ? r.derived.hqPct.toFixed(1) : "—"), sort: (r) => (r.hasTrace ? r.derived.hqPct : -1) },
      { key: "stops", header: "Stops", width: 56, align: "right", render: (r) => r.derived.stopCodons, sort: (r) => r.derived.stopCodons },
      {
        key: "top",
        header: "Top hit species",
        width: 210,
        render: (r) => <i>{r.topHit?.sciname ?? ""}</i>,
        sort: (r) => r.topHit?.sciname ?? null,
      },
      { key: "ident", header: "Per. Ident", width: 84, align: "right", render: (r) => pct(r.topHit?.perIdent), sort: (r) => r.topHit?.perIdent ?? null },
      { key: "cover", header: "Query Cover", width: 92, align: "right", render: (r) => cover(r.topHit?.queryCover), sort: (r) => r.topHit?.queryCover ?? null },
      { key: "acc", header: "Accession", width: 104, render: (r) => (r.topHit ? <AccessionLink acc={r.topHit.accession} /> : "") },
    ],
    [],
  );

  if (!project) return null;

  const compile = () =>
    run(async () => {
      setProject(await api.compileResults());
      setTab("compiled");
    }, "Results compiled");

  const searched = project.reads.filter((r) => r.searched).length;

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title="Results"
        subtitle={`${searched} of ${project.reads.length} samples have BLAST results`}
        actions={
          <>
            {project.recompileNeeded && <Badge tone="warn">{t("results.recompile")}</Badge>}
            <button className="btn btn-primary" disabled={searched === 0} onClick={compile}>
              <Sparkles size={15} /> {t("results.compile")}
            </button>
          </>
        }
      />
      <Tabs.Root value={tab} onValueChange={setTab} className="flex min-h-0 flex-1 flex-col">
        <Tabs.List className="flex gap-1 border-b border-line px-6">
          {[
            ["overview", "Batch overview"],
            ["sample", "Sample detail"],
            ["compiled", "Compiled"],
          ].map(([v, label]) => (
            <Tabs.Trigger
              key={v}
              value={v}
              className="-mb-px border-b-2 border-transparent px-3 py-2 font-medium text-muted data-[state=active]:border-accent data-[state=active]:text-ink"
            >
              {label}
            </Tabs.Trigger>
          ))}
        </Tabs.List>
        <Tabs.Content value="overview" className="min-h-0 flex-1 p-6 pt-4">
          <VirtualTable
            rows={project.reads}
            columns={overviewColumns}
            rowKey={(r) => r.id}
            selectedKey={selectedId}
            onRowClick={(r) => {
              select(r.id);
              setTab("sample");
            }}
          />
        </Tabs.Content>
        <Tabs.Content value="sample" className="min-h-0 flex-1 overflow-auto p-6 pt-4">
          <SampleDetail id={selectedId ?? project.reads.find((r) => r.searched)?.id ?? project.reads[0]?.id} />
        </Tabs.Content>
        <Tabs.Content value="compiled" className="min-h-0 flex-1 overflow-auto p-6 pt-4">
          <CompiledView onCompile={compile} onOpenSample={(id) => {
            select(id);
            setTab("sample");
          }} />
        </Tabs.Content>
      </Tabs.Root>
    </div>
  );
}

function SampleDetail({ id }: { id: string | undefined }) {
  const { project, select } = useApp();
  const [result, setResult] = useState<SampleResult | null | undefined>(undefined);
  const [rank, setRank] = useState<number | null>(null);
  const [detail, setDetail] = useState<HitDetail | null>(null);
  const [org, setOrg] = useState("");
  const [identMin, setIdentMin] = useState("");
  const [identMax, setIdentMax] = useState("");
  const [eMax, setEMax] = useState("");

  useEffect(() => {
    setResult(undefined);
    setRank(null);
    setDetail(null);
    if (!id) return;
    api.getSampleResult(id).then((r) => {
      setResult(r);
      setRank(r?.hits[0]?.rank ?? null);
    }).catch((e) => toast.error(errorText(e)));
  }, [id, project?.groups.filter((g) => g.status === "ready").length]);

  useEffect(() => {
    setDetail(null);
    if (!id || rank == null) return;
    api.getHitDetail(id, rank).then(setDetail).catch((e) => toast.error(errorText(e)));
  }, [id, rank]);

  const hits = useMemo(() => {
    if (!result) return [];
    const o = org.trim().toLowerCase();
    const lo = identMin === "" ? -Infinity : Number(identMin);
    const hi = identMax === "" ? Infinity : Number(identMax);
    const em = eMax === "" ? Infinity : Number(eMax);
    return result.hits.filter(
      (h) => (!o || h.sciname.toLowerCase().includes(o) || h.title.toLowerCase().includes(o)) && h.perIdent >= lo && h.perIdent <= hi && h.evalue <= em,
    );
  }, [result, org, identMin, identMax, eMax]);

  const hitColumns: Column<HitRow>[] = useMemo(
    () => [
      { key: "desc", header: "Description", width: 430, render: (h) => <span title={h.title}>{h.title}</span>, sort: (h) => h.title },
      { key: "sci", header: "Scientific Name", width: 180, render: (h) => <i>{h.sciname}</i>, sort: (h) => h.sciname },
      { key: "max", header: "Max Score", width: 78, align: "right", render: (h) => score(h.maxScore), sort: (h) => h.maxScore },
      { key: "tot", header: "Total Score", width: 80, align: "right", render: (h) => score(h.totalScore), sort: (h) => h.totalScore },
      { key: "cov", header: "Query Cover", width: 86, align: "right", render: (h) => cover(h.queryCover), sort: (h) => h.queryCover },
      { key: "e", header: "E value", width: 70, align: "right", render: (h) => evalue(h.evalue), sort: (h) => h.evalue },
      { key: "id", header: "Per. Ident", width: 80, align: "right", render: (h) => pct(h.perIdent), sort: (h) => h.perIdent },
      { key: "len", header: "Acc. Len", width: 70, align: "right", render: (h) => h.accLen, sort: (h) => h.accLen },
      { key: "acc", header: "Accession", width: 104, render: (h) => <AccessionLink acc={h.accession} />, sort: (h) => h.accession },
    ],
    [],
  );

  if (!project || !id) return <Empty>No sample selected.</Empty>;
  const read = project.reads.find((r) => r.id === id);

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center gap-2">
        <span className="label">Sample</span>
        <select className="input" value={id} onChange={(e) => select(e.target.value)}>
          {project.reads.map((r) => (
            <option key={r.id} value={r.id}>
              {r.id}
              {r.searched ? "" : " (no results)"}
            </option>
          ))}
        </select>
        {read && <Flags flags={read.flags} />}
      </div>

      {result === undefined ? (
        <div className="text-muted">Loading…</div>
      ) : result === null ? (
        <div className="card p-5 text-muted">
          This sample has no BLAST results yet{read?.derived.eligible ? "." : " (it does not pass QC and is not included)."}
        </div>
      ) : (
        <>
          {result.outdated && (
            <div className="card border-warn bg-warn-soft px-4 py-2.5 text-warn">
              Outdated: the sequence changed after this search. Use "Search new and changed samples" on the BLAST screen.
            </div>
          )}
          <div className="card grid grid-cols-[130px_1fr_130px_1fr] gap-x-4 gap-y-1.5 px-4 py-3">
            <span className="label">Job Title</span>
            <span>{result.jobTitle}</span>
            <span className="label">RID</span>
            <span className="tabular">
              {result.rid} <span className="text-muted">· {shortDate(result.submittedAt)}</span>
            </span>
            <span className="label">Program</span>
            <span>{result.program.toUpperCase()}</span>
            <span className="label">Database</span>
            <span>{result.database}</span>
            <span className="label">Query ID</span>
            <span className="tabular">{result.queryId}</span>
            <span className="label">Description</span>
            <span className="truncate" title={result.queryTitle}>
              {result.queryTitle || "None"}
            </span>
            <span className="label">Molecule type</span>
            <span>{result.moleculeType}</span>
            <span className="label">Query Length</span>
            <span className="tabular">{result.queryLen}</span>
          </div>

          <div>
            <div className="mb-2 flex flex-wrap items-end gap-3">
              <div className="font-semibold">Sequences producing significant alignments</div>
              <span className="label">
                {hits.length} of {result.hits.length}
              </span>
              <div className="ml-auto flex flex-wrap items-center gap-2">
                <input className="input w-[170px]" placeholder="Organism filter" value={org} onChange={(e) => setOrg(e.target.value)} />
                <span className="label">Per. Ident</span>
                <input className="input w-[64px]" placeholder="min" value={identMin} onChange={(e) => setIdentMin(e.target.value)} />
                <input className="input w-[64px]" placeholder="max" value={identMax} onChange={(e) => setIdentMax(e.target.value)} />
                <span className="label">E value ≤</span>
                <input className="input w-[72px]" placeholder="any" value={eMax} onChange={(e) => setEMax(e.target.value)} />
              </div>
            </div>
            {result.hits.length === 0 ? (
              <div className="card p-5 text-muted">{result.message ?? "No significant similarity found."}</div>
            ) : (
              <div className="h-[340px]">
                <VirtualTable rows={hits} columns={hitColumns} rowKey={(h) => String(h.rank)} selectedKey={rank != null ? String(rank) : null} onRowClick={(h) => setRank(h.rank)} />
              </div>
            )}
          </div>

          {detail && <AlignmentDetail detail={detail} />}
        </>
      )}
    </div>
  );
}

function AlignmentDetail({ detail }: { detail: HitDetail }) {
  return (
    <div className="card p-4">
      <div className="text-[15px] font-semibold">{detail.title}</div>
      <div className="mt-1 mb-3 flex gap-4 text-muted">
        <span>
          Sequence ID: <AccessionLink acc={detail.accession} />
        </span>
        <span>Length: {detail.length}</span>
        <span>Number of Matches: {detail.hsps.length}</span>
      </div>
      {detail.hsps.map((h, i) => (
        <div key={i} className={clsx(i > 0 && "mt-5 border-t border-line pt-4")}>
          <div className="mb-2 font-semibold">
            Range {i + 1}: {Math.min(h.hitFrom, h.hitTo)} to {Math.max(h.hitFrom, h.hitTo)}
          </div>
          <div className="mb-3 grid max-w-[720px] grid-cols-5 gap-2 border-y border-line py-2">
            <div>
              <div className="label">Score</div>
              <div className="tabular">
                {score(h.bitScore)} bits({Math.round(h.score)})
              </div>
            </div>
            <div>
              <div className="label">Expect</div>
              <div className="tabular">{evalue(h.evalue)}</div>
            </div>
            <div>
              <div className="label">Identities</div>
              <div className="tabular">
                {h.identity}/{h.alignLen}({Math.floor((h.identity * 100) / h.alignLen)}%)
              </div>
            </div>
            <div>
              <div className="label">Gaps</div>
              <div className="tabular">
                {h.gaps}/{h.alignLen}({Math.floor((h.gaps * 100) / h.alignLen)}%)
              </div>
            </div>
            <div>
              <div className="label">Strand</div>
              <div>
                {h.queryStrand}/{h.hitStrand}
              </div>
            </div>
          </div>
          <pre className="m-0 overflow-x-auto font-mono text-[12.5px] leading-[18px] select-text">
            {alignmentBlocks(h)
              .map(
                (b) =>
                  `Query  ${String(b.qStart).padEnd(5)} ${b.q}  ${b.qEnd}\n` +
                  `             ${b.mid}\n` +
                  `Sbjct  ${String(b.sStart).padEnd(5)} ${b.s}  ${b.sEnd}\n`,
              )
              .join("\n")}
          </pre>
        </div>
      ))}
    </div>
  );
}
