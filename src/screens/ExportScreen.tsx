import { useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { FileArchive, FileSpreadsheet, FileText, FileType2, Loader2 } from "lucide-react";
import { api } from "../api";
import { useApp } from "../state";
import { PageHeader } from "../components/ui";

export default function ExportScreen() {
  const { project, run } = useApp();
  const [busy, setBusy] = useState<string | null>(null);
  const [onlyEligible, setOnlyEligible] = useState(true);
  const [fullReport, setFullReport] = useState<boolean | null>(null);
  if (!project) return null;
  // Summary only by default for large batches: a full report has a page per sample.
  const full = fullReport ?? project.reads.length <= 100;

  const base = project.name;
  const withBusy = async (label: string, fn: () => Promise<unknown>) => {
    setBusy(label);
    try {
      await fn();
    } finally {
      setBusy(null);
    }
  };

  const excel = async () => {
    const path = await save({ defaultPath: `${base}.xlsx`, filters: [{ name: "Excel workbook", extensions: ["xlsx"] }] });
    if (path) withBusy("excel", () => run(() => api.exportExcel(path), `Saved ${path}`));
  };
  const fasta = async () => {
    const path = await save({ defaultPath: `${base}.fasta`, filters: [{ name: "FASTA", extensions: ["fasta", "fa"] }] });
    if (path) withBusy("fasta", () => run(async () => api.exportFasta(path, onlyEligible), `Saved ${path}`));
  };
  const pdf = async () => {
    const path = await save({ defaultPath: `${base}-report.pdf`, filters: [{ name: "PDF", extensions: ["pdf"] }] });
    if (path) withBusy("pdf", () => run(() => api.exportPdf(path, full), `Saved ${path}`));
  };
  const raw = async () => {
    const path = await save({ defaultPath: `${base}-ncbi-results.zip`, filters: [{ name: "Zip", extensions: ["zip"] }] });
    if (path) withBusy("raw", () => run(() => api.exportRaw(path), `Saved ${path}`));
  };

  const ready = project.groups.filter((g) => g.status === "ready").length;

  return (
    <div className="h-full overflow-auto">
      <PageHeader title="Export" subtitle="Every export covers the whole batch." />
      <div className="grid max-w-[900px] grid-cols-1 gap-3 px-6 pb-6">
        <div className="card flex items-start gap-4 p-4">
          <FileSpreadsheet className="mt-0.5 text-accent" size={26} />
          <div className="flex-1">
            <div className="font-semibold">Excel workbook (.xlsx)</div>
            <div className="mt-1 text-muted">
              Sheets: Overview, Compiled with a species chart{project.compiled ? "" : " (run Compile results first)"}, QC, Hits,
              Sequences, Run info.
            </div>
          </div>
          <button className="btn btn-primary" onClick={excel} disabled={!!busy}>
            {busy === "excel" && <Loader2 size={14} className="animate-spin" />} Save Excel…
          </button>
        </div>
        <div className="card flex items-start gap-4 p-4">
          <FileType2 className="mt-0.5 text-accent" size={26} />
          <div className="flex-1">
            <div className="font-semibold">PDF report (.pdf)</div>
            <div className="mt-1 text-muted">
              Cover page with the settings used, the compiled species chart and table, the batch overview, and an appendix
              with the final sequences.
            </div>
            <div className="mt-2 flex gap-4">
              <label className="flex items-center gap-2">
                <input type="radio" name="report" checked={!full} onChange={() => setFullReport(false)} />
                Summary only
              </label>
              <label className="flex items-center gap-2">
                <input type="radio" name="report" checked={full} onChange={() => setFullReport(true)} />
                Full report: one page per sample with top 10 hits and the best alignment (
                {project.reads.filter((r) => !r.pairedWith).length} pages more)
              </label>
            </div>
          </div>
          <button className="btn btn-primary" onClick={pdf} disabled={!!busy}>
            {busy === "pdf" && <Loader2 size={14} className="animate-spin" />} Save PDF…
          </button>
        </div>
        <div className="card flex items-start gap-4 p-4">
          <FileText className="mt-0.5 text-accent" size={26} />
          <div className="flex-1">
            <div className="font-semibold">FASTA (.fasta)</div>
            <div className="mt-1 text-muted">The final, trimmed barcode sequences, 5'→3' along COI, one record per sample.</div>
            <label className="mt-2 flex items-center gap-2">
              <input type="checkbox" checked={onlyEligible} onChange={(e) => setOnlyEligible(e.target.checked)} />
              Only samples included in BLAST
            </label>
          </div>
          <button className="btn" onClick={fasta} disabled={!!busy}>
            {busy === "fasta" && <Loader2 size={14} className="animate-spin" />} Save FASTA…
          </button>
        </div>
        <div className="card flex items-start gap-4 p-4">
          <FileArchive className="mt-0.5 text-accent" size={26} />
          <div className="flex-1">
            <div className="font-semibold">Raw NCBI results (.zip)</div>
            <div className="mt-1 text-muted">NCBI's original JSON response for each finished search group ({ready} available).</div>
          </div>
          <button className="btn" onClick={raw} disabled={!!busy || ready === 0}>
            {busy === "raw" && <Loader2 size={14} className="animate-spin" />} Save raw results…
          </button>
        </div>
      </div>
    </div>
  );
}
