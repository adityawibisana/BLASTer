// Mirrors the Rust types in src-tauri/src/model.rs, views.rs and blast.rs.

export type Dir = "F" | "R";
export type PrimerMode = "auto" | "preset" | "custom" | "none";
export type StrandHint = "forward" | "reverse" | "both" | "neither";
export type GroupStatus = "pending" | "submitted" | "ready" | "failed" | "expired" | "cancelled";
export type Flag =
  | "failedQc"
  | "primerMissing"
  | "length"
  | "stopCodon"
  | "ambiguous"
  | "directionUnknown"
  | "edited"
  | "lowIdentity"
  | "mixedSpecies"
  | "noHits"
  | "outdated"
  | "notSearched";

export interface PrimerSet {
  name: string;
  forwardName: string;
  forward: string;
  reverseName: string;
  reverse: string;
}

export interface BlastSettings {
  program: string;
  database: string;
  megablast: boolean;
  maxHits: number;
  organism: string;
  excludeModels: boolean;
  excludeUncultured: boolean;
  groupSize: number;
}

export interface Settings {
  email: string;
  language: string;
  hqQuality: number;
  hqThreshold: number;
  minLength: number;
  mottCutoff: number;
  singleLenMin: number;
  singleLenMax: number;
  consensusLenMin: number;
  consensusLenMax: number;
  ambiguityMaxPct: number;
  primerMode: PrimerMode;
  presetSet: string;
  customPrimers: PrimerSet[];
  maxMismatches: number;
  minPartial: number;
  frPattern: string;
  blast: BlastSettings;
  compileMinCover: number;
  lowIdentity: number;
}

export interface Range {
  start: number;
  end: number;
}

export interface SiteHit {
  primer: string;
  start: number;
  end: number;
  mismatches: number;
  matched: number;
  primerLength: number;
  partial: boolean;
}

export interface PrimerScan {
  setName: string | null;
  orientation: Dir | null;
  startSite: SiteHit | null;
  endSite: SiteHit | null;
  lo: number;
  hi: number;
}

export interface Derived {
  trim: Range | null;
  direction: Dir | null;
  finalSeq: string;
  finalLen: number;
  hqPct: number;
  gcPct: number;
  stopCodons: number;
  readingFrame: number;
  ambiguous: number;
  qcFailed: boolean;
  eligible: boolean;
  fingerprint: string;
  flags: Flag[];
}

export interface HitRow {
  rank: number;
  accession: string;
  title: string;
  sciname: string;
  taxid: number | null;
  maxScore: number;
  totalScore: number;
  queryCover: number;
  evalue: number;
  perIdent: number;
  accLen: number;
  hspCount: number;
}

export interface ReadView {
  id: string;
  fileName: string;
  sampleName: string | null;
  rawLength: number;
  rawHqPct: number;
  rawGcPct: number;
  rawAmbiguous: number;
  primer: PrimerScan;
  autoTrim: Range;
  manualTrim: Range | null;
  strandHint: StrandHint;
  directionAuto: Dir | null;
  directionManual: Dir | null;
  includeFailed: boolean;
  excluded: boolean;
  derived: Derived;
  flags: Flag[];
  searched: boolean;
  outdated: boolean;
  queued: boolean;
  topHit: HitRow | null;
}

export interface GroupView {
  id: string;
  sampleIds: string[];
  status: GroupStatus;
  rid: string | null;
  submittedAt: string | null;
  finishedAt: string | null;
  attempts: number;
  error: string | null;
}

export interface CompiledRow {
  sampleId: string;
  species: string | null;
  identity: number | null;
  queryCover: number | null;
  accession: string | null;
  title: string | null;
  runnerUp: string | null;
  runnerUpIdentity: number | null;
  hitsCounted: number;
  reason: string | null;
  flags: Flag[];
}

export interface SpeciesCount {
  species: string;
  count: number;
  pct: number;
}

export interface Compiled {
  createdAt: string;
  minCover: number;
  rows: CompiledRow[];
  chart: SpeciesCount[];
}

export interface SkippedFile {
  fileName: string;
  reason: string;
}

export interface ProjectView {
  path: string;
  name: string;
  createdAt: string;
  sourceZip: string;
  settings: Settings;
  reads: ReadView[];
  skipped: SkippedFile[];
  groups: GroupView[];
  blastRunning: boolean;
  blastSettingsUsed: BlastSettings | null;
  compiled: Compiled | null;
  recompileNeeded: boolean;
  primerSummary: { setName: string | null; reads: number }[];
}

export interface RecentProject {
  path: string;
  name: string;
  openedAt: string;
  exists: boolean;
}

export interface TraceView {
  bases: string;
  quals: number[];
  peaks: number[];
  traces: [number[], number[], number[], number[]];
}

export interface SampleResult {
  sampleId: string;
  jobTitle: string;
  rid: string | null;
  groupId: string;
  program: string;
  database: string;
  queryId: string;
  queryTitle: string;
  queryLen: number;
  moleculeType: string;
  message: string | null;
  hits: HitRow[];
  outdated: boolean;
  submittedAt: string | null;
}

export interface Hsp {
  bitScore: number;
  score: number;
  evalue: number;
  identity: number;
  alignLen: number;
  gaps: number;
  queryFrom: number;
  queryTo: number;
  hitFrom: number;
  hitTo: number;
  queryStrand: string;
  hitStrand: string;
  qseq: string;
  hseq: string;
  midline: string;
}

export interface HitDetail {
  accession: string;
  title: string;
  sciname: string;
  length: number;
  hsps: Hsp[];
}
