import type { Flag, Hsp, SpeciesCount } from "../types";

/** NCBI shows bit scores rounded down. */
export const score = (v: number) => Math.floor(v + 1e-9).toString();

export const pct = (v: number | null | undefined, digits = 2) =>
  v == null ? "" : `${v.toFixed(digits)}%`;

/** Query cover as NCBI shows it: whole percent. */
export const cover = (v: number | null | undefined) => (v == null ? "" : `${Math.round(v)}%`);

/** E value the way NCBI writes it: 0.0, 3e-45, 0.002. */
export function evalue(v: number): string {
  if (v === 0) return "0.0";
  if (v < 1e-3) {
    const [m, e] = v.toExponential(0).split("e");
    return `${m}e${e}`;
  }
  return v < 0.1 ? v.toFixed(3) : v.toFixed(2);
}

export const FLAG_LABEL: Record<Flag, string> = {
  failedQc: "Failed QC",
  primerMissing: "Primer not found where expected",
  length: "Unexpected length",
  stopCodon: "Stop codon",
  ambiguous: "Too many ambiguous bases",
  directionUnknown: "Direction unknown",
  edited: "Edited",
  lowIdentity: "Low identity",
  mixedSpecies: "Mixed species",
  noHits: "No hits",
  outdated: "Outdated",
  notSearched: "Not searched",
  frConflict: "F/R conflict",
  noOverlap: "F/R do not overlap",
};

/** How a sample was read: single, an assembled pair, or the reverse half of one. */
export function readsLabel(r: { pair: { noOverlap: boolean; useOnly: string | null } | null; pairedWith: string | null }) {
  if (r.pair) {
    if (!r.pair.noOverlap) return "F + R";
    return r.pair.useOnly ? `${r.pair.useOnly} only` : "F + R, no overlap";
  }
  return r.pairedWith ? `R of ${r.pairedWith}` : "single";
}

/** How serious a flag is, for colouring. */
export function flagTone(f: Flag): "bad" | "warn" | "info" {
  switch (f) {
    case "failedQc":
    case "noHits":
    case "noOverlap":
      return "bad";
    case "edited":
    case "notSearched":
      return "info";
    default:
      return "warn";
  }
}

export const REASON_LABEL: Record<string, string> = {
  noHits: "No hit with enough query cover",
  failedQc: "Failed QC, not searched",
  excluded: "Excluded by user",
  notSearched: "Not searched yet",
};

export interface AlignmentBlock {
  qStart: number;
  qEnd: number;
  q: string;
  mid: string;
  s: string;
  sStart: number;
  sEnd: number;
}

/**
 * Split an HSP into NCBI-style blocks of `width` columns with Query and Sbjct
 * positions. Gaps ('-') do not advance a position; a Minus strand counts down.
 */
export function alignmentBlocks(h: Hsp, width = 60): AlignmentBlock[] {
  const blocks: AlignmentBlock[] = [];
  const qStep = h.queryFrom <= h.queryTo ? 1 : -1;
  const sStep = h.hitStrand === "Minus" || h.hitFrom > h.hitTo ? -1 : 1;
  let q = h.queryFrom;
  let s = h.hitFrom;
  for (let i = 0; i < h.qseq.length; i += width) {
    const qs = h.qseq.slice(i, i + width);
    const ss = h.hseq.slice(i, i + width);
    const qBases = qs.replace(/-/g, "").length;
    const sBases = ss.replace(/-/g, "").length;
    const qEnd = q + qStep * (qBases - 1);
    const sEnd = s + sStep * (sBases - 1);
    blocks.push({ qStart: q, qEnd, q: qs, mid: h.midline.slice(i, i + width), s: ss, sStart: s, sEnd });
    q = qEnd + qStep;
    s = sEnd + sStep;
  }
  return blocks;
}

/** Top `limit` species, the rest folded into "Other". */
export function chartRows(chart: SpeciesCount[], limit = 15, expanded = false) {
  if (expanded || chart.length <= limit) return { rows: chart, other: [] as SpeciesCount[] };
  const rows = chart.slice(0, limit);
  const other = chart.slice(limit);
  const count = other.reduce((a, b) => a + b.count, 0);
  const share = other.reduce((a, b) => a + b.pct, 0);
  return { rows: [...rows, { species: `Other (${other.length} species)`, count, pct: share }], other };
}

/**
 * NCBI asks for large batches on weekends or between 9 pm and 5 am US Eastern time.
 */
export function isNcbiOffPeak(now = new Date()): boolean {
  const parts = new Intl.DateTimeFormat("en-US", {
    timeZone: "America/New_York",
    weekday: "short",
    hour: "numeric",
    hour12: false,
  }).formatToParts(now);
  const weekday = parts.find((p) => p.type === "weekday")?.value ?? "";
  const hour = Number(parts.find((p) => p.type === "hour")?.value ?? "12") % 24;
  return weekday === "Sat" || weekday === "Sun" || hour >= 21 || hour < 5;
}

export function duration(ms: number): string {
  const s = Math.max(0, Math.round(ms / 1000));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (h > 0) return `${h} h ${m} min`;
  if (m > 0) return `${m} min ${s % 60} s`;
  return `${s} s`;
}

export function shortDate(iso: string | null | undefined): string {
  if (!iso) return "";
  const d = new Date(iso);
  return d.toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}
