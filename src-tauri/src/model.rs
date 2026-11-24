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

/// Where BLAST searches run.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum BlastLocation {
    /// The app sends each group to NCBI's BLAST service and fetches the results.
    #[default]
    Ncbi,
    /// The user runs each group on the NCBI BLAST website and imports the results file.
    Website,
    /// BLAST+ (`blastn`) on this computer, against a local database.
    Local,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct BlastSettings {
    pub program: String,
    pub database: String,
    pub megablast: bool,
    /// Hits kept per query. NCBI limits the BLAST compute per IP address; fewer hits
    /// make each search cheaper and its results smaller.
    pub max_hits: u32,
    pub organism: String,
    pub exclude_models: bool,
    pub exclude_uncultured: bool,
    pub group_size: usize,
    /// A search NCBI still reports as waiting after this many minutes is given up.
    pub give_up_minutes: u32,
    pub location: BlastLocation,
    /// `blastn` program for [`BlastLocation::Local`]; empty means `blastn` on the PATH.
    pub blastn_path: String,
    /// Local BLAST database (path and name, as for `blastn -db`).
    pub local_db: String,
}

/// Settings saved before the give-up time was in minutes hold `giveUpHours`;
/// turn it into `giveUpMinutes`. `blast` is a saved BlastSettings object.
pub fn migrate_blast_settings(blast: &mut serde_json::Value) {
    let Some(o) = blast.as_object_mut() else { return };
    if let Some(h) = o.remove("giveUpHours").and_then(|v| v.as_f64()) {
        if !o.contains_key("giveUpMinutes") && h.is_finite() {
            o.insert("giveUpMinutes".into(), ((h * 60.0).round().max(0.0) as u64).into());
        }
    }
}

/// [`migrate_blast_settings`] on every BLAST settings object of a saved project.
pub fn migrate_project(v: &mut serde_json::Value) {
    if let Some(b) = v.pointer_mut("/settings/blast") {
        migrate_blast_settings(b);
    }
    if let Some(b) = v.pointer_mut("/blast/settingsUsed") {
        migrate_blast_settings(b);
    }
}

/// Hits kept per query by default, as the Geneious guide on slow NCBI searches suggests.
pub const DEFAULT_MAX_HITS: u32 = 10;

impl Default for BlastSettings {
    fn default() -> Self {
        Self {
            program: "blastn".into(),
            database: "core_nt".into(),
            megablast: true,
            max_hits: DEFAULT_MAX_HITS,
            organism: String::new(),
            exclude_models: false,
            exclude_uncultured: false,
            group_size: 20,
            give_up_minutes: 120,
            location: BlastLocation::Ncbi,
            blastn_path: String::new(),
            local_db: String::new(),
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
    /// F and R disagree at a position where both are high quality.
    FrConflict,
    /// The F and R reads of a pair do not overlap.
    NoOverlap,
}

/// A confirmed forward/reverse pair, stored on the forward read.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PairInfo {
    pub reverse_id: String,
    /// Aligned columns where both reads have a base or a gap.
    pub overlap: usize,
    pub disagreements: usize,
    /// Consensus positions (0-based) where both reads were high quality but differed.
    pub conflicts: Vec<usize>,
    pub identity: f64,
    pub no_overlap: bool,
    /// When the reads do not overlap: search only this read.
    pub use_only: Option<Dir>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PairSuggestion {
    pub forward_id: String,
    pub reverse_id: String,
    pub overlap: usize,
    pub identity: f64,
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
    /// Set on the forward read of a confirmed pair; its derived values are the consensus.
    #[serde(default)]
    pub pair: Option<PairInfo>,
    /// Set on the reverse read of a confirmed pair: the forward read's ID.
    #[serde(default)]
    pub paired_with: Option<String>,
    /// False for a read imported from FASTA: there is no chromatogram, and every
    /// base has quality [`crate::fasta::FASTA_QUAL`].
    #[serde(default = "has_trace_default")]
    pub has_trace: bool,
}

fn has_trace_default() -> bool {
    true
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
    /// When the app last asked NCBI about this search, and NCBI's answer
    /// ("WAITING", "READY", ...). Shows the user that the search is still alive.
    #[serde(default)]
    pub last_checked_at: Option<String>,
    #[serde(default)]
    pub last_check: Option<String>,
    /// Where the group's search runs or its results came from.
    #[serde(default)]
    pub source: GroupSource,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum GroupSource {
    /// Sent to NCBI's BLAST service by the app.
    #[default]
    Ncbi,
    /// Run with BLAST+ on this computer.
    Local,
    /// Results imported from a file (e.g. downloaded from the NCBI BLAST website).
    File,
}

impl BlastGroup {
    /// Put the group back in the queue so its search is sent to NCBI again with a
    /// new RID. Only for a group still waiting or one that did not finish.
    pub fn reset_for_resend(&mut self) -> anyhow::Result<()> {
        match self.status {
            GroupStatus::Submitted | GroupStatus::Failed | GroupStatus::Expired | GroupStatus::Cancelled => {}
            GroupStatus::Pending => anyhow::bail!("group {} has not been sent yet", self.id),
            GroupStatus::Ready => anyhow::bail!("group {} already has its results", self.id),
        }
        self.status = GroupStatus::Pending;
        self.rid = None;
        self.rtoe_secs = None;
        self.submitted_at = None;
        self.finished_at = None;
        self.error = None;
        self.last_checked_at = None;
        self.last_check = None;
        Ok(())
    }
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
    #[serde(default)]
    pub pair_suggestions: Vec<PairSuggestion>,
    /// Suggestions the user rejected, as (forward ID, reverse ID); not suggested again.
    #[serde(default)]
    pub rejected_pairs: Vec<(String, String)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(status: GroupStatus) -> BlastGroup {
        BlastGroup {
            id: "g0002".into(),
            sample_ids: vec!["a".into()],
            fingerprints: vec!["f".into()],
            fasta: ">a\nACGT\n".into(),
            status,
            rid: Some("GANJXGSA016".into()),
            rtoe_secs: Some(30),
            submitted_at: Some("2026-11-24T16:18:22+07:00".into()),
            finished_at: None,
            attempts: 1,
            error: Some("NCBI did not finish".into()),
            last_checked_at: Some("2026-11-24T17:36:00+07:00".into()),
            last_check: Some("WAITING".into()),
            source: GroupSource::Ncbi,
        }
    }

    #[test]
    fn resend_puts_a_waiting_or_failed_group_back_in_the_queue() {
        for status in [GroupStatus::Submitted, GroupStatus::Failed, GroupStatus::Expired, GroupStatus::Cancelled] {
            let mut g = group(status);
            g.reset_for_resend().unwrap();
            assert_eq!(g.status, GroupStatus::Pending);
            assert_eq!((g.rid, g.submitted_at, g.error, g.last_check, g.last_checked_at), (None, None, None, None, None));
            // The query itself and the attempt count are kept.
            assert_eq!((g.fasta.as_str(), g.attempts, g.sample_ids.len()), (">a\nACGT\n", 1, 1));
        }
    }

    #[test]
    fn resend_leaves_finished_and_unsent_groups_alone() {
        for status in [GroupStatus::Ready, GroupStatus::Pending] {
            let mut g = group(status.clone());
            assert!(g.reset_for_resend().is_err());
            assert_eq!(g.status, status);
            assert_eq!(g.rid.as_deref(), Some("GANJXGSA016"));
        }
    }

    #[test]
    fn old_projects_without_check_fields_still_load() {
        let mut v = serde_json::to_value(group(GroupStatus::Submitted)).unwrap();
        let o = v.as_object_mut().unwrap();
        o.remove("lastCheckedAt");
        o.remove("lastCheck");
        let g: BlastGroup = serde_json::from_value(v).unwrap();
        assert_eq!(g.last_check, None);
        let s: BlastSettings = serde_json::from_str(r#"{"groupSize": 20}"#).unwrap();
        assert_eq!(s.give_up_minutes, 120);
    }

    #[test]
    fn give_up_hours_become_minutes() {
        let mut v = serde_json::json!({
            "settings": { "blast": { "giveUpHours": 1.5 } },
            "blast": { "settingsUsed": { "giveUpHours": 0.5, "groupSize": 20 } },
        });
        migrate_project(&mut v);
        let s: BlastSettings = serde_json::from_value(v["settings"]["blast"].clone()).unwrap();
        assert_eq!(s.give_up_minutes, 90);
        assert_eq!(v["blast"]["settingsUsed"], serde_json::json!({ "giveUpMinutes": 30, "groupSize": 20 }));
        // A value already in minutes wins.
        let mut b = serde_json::json!({ "giveUpHours": 3, "giveUpMinutes": 45 });
        migrate_blast_settings(&mut b);
        assert_eq!(b, serde_json::json!({ "giveUpMinutes": 45 }));
    }
}
