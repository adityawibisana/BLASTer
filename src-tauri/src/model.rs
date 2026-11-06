//! Project data model, saved as `project.json` inside a `.coiproj` archive.

use serde::{Deserialize, Serialize};

use crate::primers::{Dir, PrimerScan, PrimerSet};

pub const PROJECT_FORMAT: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum PrimerMode {
    Auto,
    Preset,
    Custom,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct BlastSettings {
    pub program: String,
    pub database: String,
    pub megablast: bool,
    pub max_hits: u32,
    pub organism: String,
    pub exclude_models: bool,
    pub exclude_uncultured: bool,
    pub group_size: usize,
}

impl Default for BlastSettings {
    fn default() -> Self {
        Self {
            program: "blastn".into(),
            database: "core_nt".into(),
            megablast: true,
            max_hits: 100,
            organism: String::new(),
            exclude_models: false,
            exclude_uncultured: false,
            group_size: 20,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub email: String,
    pub language: String,
    /// Phred value at or above which a base counts as high quality.
    pub hq_quality: u8,
    /// Kept region must have more than this percentage of high-quality bases.
    pub hq_threshold: f64,
    pub min_length: usize,
    pub mott_cutoff: f64,
    pub single_len_min: usize,
    pub single_len_max: usize,
    pub consensus_len_min: usize,
    pub consensus_len_max: usize,
    pub ambiguity_max_pct: f64,
    pub primer_mode: PrimerMode,
    pub preset_set: String,
    pub custom_primers: Vec<PrimerSet>,
    pub max_mismatches: usize,
    pub min_partial: usize,
    pub fr_pattern: String,
    pub blast: BlastSettings,
    pub compile_min_cover: f64,
    pub low_identity: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            email: String::new(),
            language: "en".into(),
            hq_quality: 40,
            hq_threshold: 70.0,
            min_length: 300,
            mott_cutoff: 0.05,
            single_len_min: 450,
            single_len_max: 658,
            consensus_len_min: 600,
            consensus_len_max: 670,
            ambiguity_max_pct: 1.0,
            primer_mode: PrimerMode::Auto,
            preset_set: "Folmer et al. 1994".into(),
            custom_primers: Vec::new(),
            max_mismatches: 3,
            min_partial: 12,
            fr_pattern: String::new(),
            blast: BlastSettings::default(),
            compile_min_cover: 80.0,
            low_identity: 97.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Range {
    pub start: usize,
    pub end: usize,
}

impl Range {
    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }
}

/// What the six-frame translation says about a read's strand.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum StrandHint {
    Forward,
    Reverse,
    Both,
    #[default]
    Neither,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum Flag {
    FailedQc,
    PrimerMissing,
    Length,
    StopCodon,
    Ambiguous,
    DirectionUnknown,
    Edited,
    LowIdentity,
    MixedSpecies,
    NoHits,
    Outdated,
    NotSearched,
}

/// Values recomputed from the raw read and the user's choices.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Derived {
    pub trim: Option<Range>,
    pub direction: Option<Dir>,
    pub final_seq: String,
    pub final_len: usize,
    pub hq_pct: f64,
    pub gc_pct: f64,
    pub stop_codons: usize,
    pub reading_frame: usize,
    pub ambiguous: usize,
    pub qc_failed: bool,
    pub eligible: bool,
    pub fingerprint: String,
    pub flags: Vec<Flag>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResultRef {
    pub group_id: String,
    pub index: usize,
    /// Fingerprint of the sequence that was searched.
    pub fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Read {
    /// Unique sample ID, used as the FASTA ID.
    pub id: String,
    pub file_name: String,
    pub sample_name: Option<String>,
    pub bases: String,
    pub quals: Vec<u8>,
    pub raw_length: usize,
    pub raw_hq_pct: f64,
    pub raw_gc_pct: f64,
    pub raw_ambiguous: usize,
    pub primer: PrimerScan,
    pub auto_trim: Range,
    pub manual_trim: Option<Range>,
    pub strand_hint: StrandHint,
    pub direction_auto: Option<Dir>,
    pub direction_manual: Option<Dir>,
    /// Search this read even though it failed QC.
    pub include_failed: bool,
    /// Left out of BLAST by the user.
    pub excluded: bool,
    pub derived: Derived,
    pub result: Option<ResultRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SkippedFile {
    pub file_name: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum GroupStatus {
    Pending,
    Submitted,
    Ready,
    Failed,
    Expired,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BlastGroup {
    pub id: String,
    pub sample_ids: Vec<String>,
    pub fingerprints: Vec<String>,
    pub fasta: String,
    pub status: GroupStatus,
    pub rid: Option<String>,
    pub rtoe_secs: Option<u64>,
    pub submitted_at: Option<String>,
    pub finished_at: Option<String>,
    pub attempts: u32,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BlastState {
    pub groups: Vec<BlastGroup>,
    /// Settings used for the searches, recorded for the Run info sheet.
    pub settings_used: Option<BlastSettings>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CompiledRow {
    pub sample_id: String,
    pub species: Option<String>,
    pub identity: Option<f64>,
    pub query_cover: Option<f64>,
    pub accession: Option<String>,
    pub title: Option<String>,
    pub runner_up: Option<String>,
    pub runner_up_identity: Option<f64>,
    pub hits_counted: usize,
    /// Why there is no species: "noHits", "notSearched", "failedQc", ...
    pub reason: Option<String>,
    pub flags: Vec<Flag>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SpeciesCount {
    pub species: String,
    pub count: usize,
    pub pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Compiled {
    pub created_at: String,
    pub min_cover: f64,
    pub rows: Vec<CompiledRow>,
    pub chart: Vec<SpeciesCount>,
    /// Result fingerprint per sample at compile time, to detect "Recompile needed".
    pub based_on: Vec<(String, Option<String>)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub format: u32,
    pub name: String,
    pub created_at: String,
    pub source_zip: String,
    pub settings: Settings,
    pub reads: Vec<Read>,
    pub skipped: Vec<SkippedFile>,
    pub blast: BlastState,
    pub compiled: Option<Compiled>,
}
