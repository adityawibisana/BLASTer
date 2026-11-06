//! What the frontend receives. Raw bases and qualities stay in the backend.

use serde::Serialize;

use crate::blast::{HitRow, QueryResult};
use crate::compile;
use crate::model::*;
use crate::primers::{Dir, PrimerScan};
use crate::store::ProjectHandle;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadView<'a> {
    pub id: &'a str,
    pub file_name: &'a str,
    pub sample_name: &'a Option<String>,
    pub raw_length: usize,
    pub raw_hq_pct: f64,
    pub raw_gc_pct: f64,
    pub raw_ambiguous: usize,
    pub primer: &'a PrimerScan,
    pub auto_trim: Range,
    pub manual_trim: Option<Range>,
    pub strand_hint: StrandHint,
    pub direction_auto: Option<Dir>,
    pub direction_manual: Option<Dir>,
    pub include_failed: bool,
    pub excluded: bool,
    pub derived: &'a Derived,
    /// Derived flags plus the flags that depend on BLAST results.
    pub flags: Vec<Flag>,
    pub searched: bool,
    pub outdated: bool,
    pub queued: bool,
    pub top_hit: Option<&'a HitRow>,
    pub pair: &'a Option<PairInfo>,
    pub paired_with: &'a Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupView<'a> {
    pub id: &'a str,
    pub sample_ids: &'a [String],
    pub status: &'a GroupStatus,
    pub rid: &'a Option<String>,
    pub submitted_at: &'a Option<String>,
    pub finished_at: &'a Option<String>,
    pub attempts: u32,
    pub error: &'a Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrimerCount {
    pub set_name: Option<String>,
    pub reads: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectView<'a> {
    pub path: String,
    pub name: &'a str,
    pub created_at: &'a str,
    pub source_zip: &'a str,
    pub settings: &'a Settings,
    pub reads: Vec<ReadView<'a>>,
    pub skipped: &'a [SkippedFile],
    pub groups: Vec<GroupView<'a>>,
    pub blast_running: bool,
    pub blast_settings_used: &'a Option<BlastSettings>,
    pub compiled: &'a Option<Compiled>,
    pub recompile_needed: bool,
    pub primer_summary: Vec<PrimerCount>,
    pub pair_suggestions: &'a [PairSuggestion],
}

/// Every status flag for a sample: QC flags plus result flags.
pub fn sample_flags(h: &ProjectHandle, read: &Read) -> Vec<Flag> {
    let mut flags = read.derived.flags.clone();
    let s = &h.project.settings;
    match h.result_for(read) {
        None => {
            if read.derived.eligible {
                flags.push(Flag::NotSearched);
            }
        }
        Some(result) => {
            if h.outdated(read) {
                flags.push(Flag::Outdated);
            }
            match result.hits.first() {
                None => flags.push(Flag::NoHits),
                Some(top) => {
                    if top.per_ident < s.low_identity {
                        flags.push(Flag::LowIdentity);
                    }
                    if compile::mixed_species(&result.hits, s.compile_min_cover) {
                        flags.push(Flag::MixedSpecies);
                    }
                }
            }
        }
    }
    flags
}

/// Identifies the result a compiled row was based on; a change means "Recompile needed".
pub fn compile_key(h: &ProjectHandle, read: &Read) -> Option<String> {
    let r = read.result.as_ref()?;
    h.result_for(read)?;
    // The current sequence is part of the key, so an edit after compiling also counts.
    Some(format!("{}:{}:{}:{}", r.group_id, r.index, r.fingerprint, read.derived.fingerprint))
}

pub fn result_for_compile<'h>(h: &'h ProjectHandle, read: &Read) -> Option<(&'h QueryResult, String)> {
    if !read.derived.eligible {
        return None;
    }
    Some((h.result_for(read)?, compile_key(h, read)?))
}

pub fn recompile_needed(h: &ProjectHandle) -> bool {
    let Some(c) = &h.project.compiled else { return false };
    let current: Vec<(String, Option<String>)> = h
        .project
        .reads
        .iter()
        .map(|r| (r.id.clone(), result_for_compile(h, r).map(|(_, k)| k)))
        .collect();
    current != c.based_on
}

pub fn queued_ids(p: &Project) -> std::collections::HashSet<&str> {
    p.blast
        .groups
        .iter()
        .filter(|g| matches!(g.status, GroupStatus::Pending | GroupStatus::Submitted))
        .flat_map(|g| g.sample_ids.iter().map(String::as_str))
        .collect()
}

pub fn project_view(h: &ProjectHandle, blast_running: bool) -> ProjectView<'_> {
    let p = &h.project;
    let queued = queued_ids(p);
    let reads = p
        .reads
        .iter()
        .map(|r| ReadView {
            id: &r.id,
            file_name: &r.file_name,
            sample_name: &r.sample_name,
            raw_length: r.raw_length,
            raw_hq_pct: r.raw_hq_pct,
            raw_gc_pct: r.raw_gc_pct,
            raw_ambiguous: r.raw_ambiguous,
            primer: &r.primer,
            auto_trim: r.auto_trim,
            manual_trim: r.manual_trim,
            strand_hint: r.strand_hint,
            direction_auto: r.direction_auto,
            direction_manual: r.direction_manual,
            include_failed: r.include_failed,
            excluded: r.excluded,
            derived: &r.derived,
            flags: sample_flags(h, r),
            searched: h.result_for(r).is_some(),
            outdated: h.outdated(r),
            queued: queued.contains(r.id.as_str()),
            top_hit: h.result_for(r).and_then(|q| q.hits.first()),
            pair: &r.pair,
            paired_with: &r.paired_with,
        })
        .collect();
    let mut summary: Vec<PrimerCount> = Vec::new();
    for r in &p.reads {
        match summary.iter_mut().find(|c| c.set_name == r.primer.set_name) {
            Some(c) => c.reads += 1,
            None => summary.push(PrimerCount { set_name: r.primer.set_name.clone(), reads: 1 }),
        }
    }
    summary.sort_by(|a, b| b.reads.cmp(&a.reads));
    ProjectView {
        path: h.path.display().to_string(),
        name: &p.name,
        created_at: &p.created_at,
        source_zip: &p.source_zip,
        settings: &p.settings,
        reads,
        skipped: &p.skipped,
        groups: p
            .blast
            .groups
            .iter()
            .map(|g| GroupView {
                id: &g.id,
                sample_ids: &g.sample_ids,
                status: &g.status,
                rid: &g.rid,
                submitted_at: &g.submitted_at,
                finished_at: &g.finished_at,
                attempts: g.attempts,
                error: &g.error,
            })
            .collect(),
        blast_running,
        blast_settings_used: &p.blast.settings_used,
        compiled: &p.compiled,
        recompile_needed: recompile_needed(h),
        primer_summary: summary,
        pair_suggestions: &p.pair_suggestions,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SampleResult<'a> {
    pub sample_id: &'a str,
    pub job_title: &'a str,
    pub rid: Option<&'a str>,
    pub group_id: &'a str,
    pub program: &'a str,
    pub database: &'a str,
    pub query_id: &'a str,
    pub query_title: &'a str,
    pub query_len: usize,
    pub molecule_type: &'static str,
    pub message: &'a Option<String>,
    pub hits: &'a [HitRow],
    pub outdated: bool,
    pub submitted_at: Option<&'a str>,
}

pub fn sample_result<'a>(h: &'a ProjectHandle, read: &'a Read) -> Option<SampleResult<'a>> {
    let rref = read.result.as_ref()?;
    let q = h.result_for(read)?;
    let group = h.project.blast.groups.iter().find(|g| g.id == rref.group_id);
    Some(SampleResult {
        sample_id: &read.id,
        job_title: &h.project.name,
        rid: group.and_then(|g| g.rid.as_deref()),
        group_id: &rref.group_id,
        program: &q.program,
        database: &q.database,
        query_id: &q.query_id,
        query_title: &q.query_title,
        query_len: q.query_len,
        molecule_type: "dna",
        message: &q.message,
        hits: &q.hits,
        outdated: h.outdated(read),
        submitted_at: group.and_then(|g| g.submitted_at.as_deref()),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceView {
    pub bases: String,
    pub quals: Vec<u8>,
    pub peaks: Vec<u32>,
    /// A, C, G, T channels.
    pub traces: [Vec<i16>; 4],
}
