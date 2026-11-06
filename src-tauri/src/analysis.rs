//! Per-read processing: primer search, quality trim, direction, QC flags.

use crate::abif::AbifTrace;
use crate::model::{Derived, Flag, PrimerMode, Range, Read, Settings, StrandHint};
use crate::primers::{self, Dir, MatchParams, PrimerSet};
use crate::seq::{self, reverse_complement};
use crate::trim;

/// The primer sets searched under the current primer option.
pub fn primer_sets(s: &Settings) -> Vec<PrimerSet> {
    let library = primers::builtin_library();
    match s.primer_mode {
        PrimerMode::None => Vec::new(),
        PrimerMode::Auto => library.into_iter().chain(s.custom_primers.iter().cloned()).collect(),
        PrimerMode::Preset => {
            library.into_iter().chain(s.custom_primers.iter().cloned()).filter(|p| p.name == s.preset_set).collect()
        }
        PrimerMode::Custom => s.custom_primers.clone(),
    }
}

pub fn new_read(id: String, file_name: String, trace: &AbifTrace) -> Read {
    let bases = String::from_utf8_lossy(&trace.bases).into_owned();
    Read {
        id,
        file_name,
        sample_name: trace.sample_name.clone(),
        raw_length: trace.bases.len(),
        raw_hq_pct: 0.0,
        raw_gc_pct: seq::gc_percent(&trace.bases),
        raw_ambiguous: trace.bases.iter().filter(|&&c| seq::is_ambiguous(c)).count(),
        bases,
        quals: trace.quals.clone(),
        primer: Default::default(),
        auto_trim: Range { start: 0, end: 0 },
        manual_trim: None,
        strand_hint: StrandHint::Neither,
        direction_auto: None,
        direction_manual: None,
        include_failed: false,
        excluded: false,
        derived: Derived::default(),
        result: None,
        pair: None,
        paired_with: None,
        has_trace: true,
    }
}

/// A read from a FASTA record. Every base gets quality [`crate::fasta::FASTA_QUAL`],
/// so quality trimming keeps the whole read and only primers are removed.
pub fn new_fasta_read(id: String, file_name: String, description: Option<String>, bases: &[u8]) -> Read {
    let quals = vec![crate::fasta::FASTA_QUAL; bases.len()];
    let trace = AbifTrace { sample_name: description, bases: bases.to_vec(), quals, peaks: Vec::new(), traces: Default::default() };
    Read { has_trace: false, ..new_read(id, file_name, &trace) }
}

fn strand_hint(region: &[u8]) -> StrandHint {
    if region.len() < 90 {
        return StrandHint::Neither;
    }
    let fwd = seq::best_forward_frame(region).0 == 0;
    let rev = seq::best_forward_frame(&reverse_complement(region)).0 == 0;
    match (fwd, rev) {
        (true, false) => StrandHint::Forward,
        (false, true) => StrandHint::Reverse,
        (true, true) => StrandHint::Both,
        (false, false) => StrandHint::Neither,
    }
}

/// Combine primer evidence and translation evidence. Disagreement means unknown.
pub fn decide_direction(primer: Option<Dir>, hint: StrandHint) -> Option<Dir> {
    match (primer, hint) {
        (Some(Dir::F), StrandHint::Reverse) | (Some(Dir::R), StrandHint::Forward) => None,
        (Some(d), _) => Some(d),
        (None, StrandHint::Forward) => Some(Dir::F),
        (None, StrandHint::Reverse) => Some(Dir::R),
        (None, _) => None,
    }
}

/// Run every automatic step for one read, then refresh its derived values.
pub fn analyze(read: &mut Read, s: &Settings, sets: &[PrimerSet]) {
    let bases = read.bases.as_bytes();
    read.raw_hq_pct = trim::hq_percent(&read.quals, s.hq_quality);
    read.primer = primers::scan(
        bases,
        sets,
        MatchParams { max_mismatches: s.max_mismatches, min_partial: s.min_partial, min_region: s.min_length },
    );
    let (start, end) =
        trim::auto_trim(&read.quals, read.primer.lo, read.primer.hi, s.hq_quality, s.hq_threshold, s.mott_cutoff);
    let mut range = Range { start, end };
    read.strand_hint = strand_hint(&bases[start..end]);
    read.direction_auto = decide_direction(read.primer.orientation, read.strand_hint);
    if let Some(dir) = read.direction_auto {
        let tidied = trim_stops_near_ends(bases, range, dir, s.min_length);
        if tidied != range {
            range = tidied;
            read.strand_hint = strand_hint(&bases[range.start..range.end]);
            read.direction_auto = decide_direction(read.primer.orientation, read.strand_hint);
        }
    }
    read.auto_trim = range;
    refresh(read, s);
}

/// Bases at each end where a stop codon is treated as a base-call error and trimmed off.
pub const STOP_ZONE: usize = 80;

/// COI has no stop codons, so stops in the best reading frame that all lie close to
/// an end mark base-call errors there: move that end past them. Stops further inside
/// are left alone and reported by the stop codon check.
fn trim_stops_near_ends(bases: &[u8], range: Range, dir: Dir, min_length: usize) -> Range {
    let raw = &bases[range.start..range.end];
    let oriented = if dir == Dir::R { reverse_complement(raw) } else { raw.to_vec() };
    let (stops, frame) = seq::best_forward_frame(&oriented);
    if stops == 0 {
        return range;
    }
    let len = oriented.len();
    let positions: Vec<usize> = (frame..len.saturating_sub(2))
        .step_by(3)
        .filter(|&p| oriented[p] == b'T' && oriented[p + 1] == b'A' && matches!(oriented[p + 2], b'A' | b'G'))
        .collect();
    let head = positions.iter().filter(|&&p| p < STOP_ZONE).max().map(|&p| p + 3);
    let tail = positions.iter().filter(|&&p| p + 3 > len.saturating_sub(STOP_ZONE)).min().copied();
    let middle = positions.iter().any(|&p| p >= STOP_ZONE && p + 3 <= len.saturating_sub(STOP_ZONE));
    if middle {
        return range;
    }
    let (o_start, o_end) = (head.unwrap_or(0), tail.unwrap_or(len));
    if o_end <= o_start || o_end - o_start < min_length {
        return range;
    }
    match dir {
        Dir::F => Range { start: range.start + o_start, end: range.start + o_end },
        Dir::R => Range { start: range.end - o_end, end: range.end - o_start },
    }
}

/// Recompute everything that follows from the raw read, the automatic results and
/// the user's choices (manual trim, direction, include/exclude).
pub fn refresh(read: &mut Read, s: &Settings) {
    let range = read.manual_trim.unwrap_or(read.auto_trim);
    let range = Range { start: range.start.min(read.raw_length), end: range.end.min(read.raw_length) };
    let direction = read.direction_manual.or(read.direction_auto);
    let raw = &read.bases.as_bytes()[range.start..range.end.max(range.start)];
    let oriented = match direction {
        Some(Dir::R) => reverse_complement(raw),
        _ => raw.to_vec(),
    };
    let quals = &read.quals[range.start..range.end.max(range.start)];
    let hq_pct = trim::hq_percent(quals, s.hq_quality);
    let (stops, frame) = match direction {
        Some(_) => seq::best_forward_frame(&oriented),
        None => seq::best_forward_frame(&oriented).min(seq::best_forward_frame(&reverse_complement(&oriented))),
    };
    let ambiguous = oriented.iter().filter(|&&c| seq::is_ambiguous(c)).count();
    let len = oriented.len();
    let qc_failed = len < s.min_length || hq_pct <= s.hq_threshold;

    let mut flags = Vec::new();
    if qc_failed {
        flags.push(Flag::FailedQc);
    }
    if len > 0 {
        let no_end_primer = read.primer.end_site.is_none() || read.primer.start_site.is_none();
        if len > s.single_len_max && s.primer_mode != crate::model::PrimerMode::None && no_end_primer {
            flags.push(Flag::PrimerMissing);
        } else if len < s.single_len_min || len > s.single_len_max {
            flags.push(Flag::Length);
        }
        if stops > 0 {
            flags.push(Flag::StopCodon);
        }
        if ambiguous as f64 * 100.0 / len as f64 > s.ambiguity_max_pct {
            flags.push(Flag::Ambiguous);
        }
    }
    if direction.is_none() {
        flags.push(Flag::DirectionUnknown);
    }
    if read.manual_trim.is_some() || read.direction_manual.is_some() {
        flags.push(Flag::Edited);
    }

    read.derived = Derived {
        trim: Some(range),
        direction,
        final_len: len,
        hq_pct,
        gc_pct: seq::gc_percent(&oriented),
        stop_codons: stops,
        reading_frame: frame,
        ambiguous,
        qc_failed,
        eligible: !read.excluded && len > 0 && (!qc_failed || read.include_failed),
        fingerprint: seq::fingerprint(&oriented),
        final_seq: String::from_utf8(oriented).unwrap_or_default(),
        flags,
    };
}

/// Stats for a trim range the user is dragging, without changing the read.
pub fn preview(read: &Read, s: &Settings, range: Range) -> Derived {
    let mut copy = read.clone();
    copy.manual_trim = Some(range);
    refresh(&mut copy, s);
    copy.derived
}

/// Turn file names into unique FASTA-safe IDs: the name without `.ab1`, other
/// characters replaced by `_`, duplicates suffixed `_2`, `_3`, ...
pub fn unique_ids(file_names: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashMap::<String, usize>::new();
    file_names
        .iter()
        .map(|f| {
            let stem = std::path::Path::new(f).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let stem = stem.strip_suffix(".ab1").or_else(|| stem.strip_suffix(".AB1")).unwrap_or(&stem);
            let mut id: String =
                stem.chars().map(|c| if c.is_ascii_alphanumeric() || "-_.".contains(c) { c } else { '_' }).collect();
            if id.is_empty() {
                id = "sample".into();
            }
            let n = seen.entry(id.to_lowercase()).or_insert(0);
            *n += 1;
            if *n > 1 { format!("{id}_{n}") } else { id }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_safe() {
        let names: Vec<String> = ["a b.ab1", "x/a b.ab1", "VN220.ab1"].iter().map(|s| s.to_string()).collect();
        assert_eq!(unique_ids(&names), vec!["a_b", "a_b_2", "VN220"]);
    }

    #[test]
    fn stops_near_start_are_trimmed_off() {
        // Frame 0: a stop at codon 2, then clean codons; frames 1 and 2 are full of stops.
        let seqs = [b"GCTTAAGCT".as_slice(), &b"CTAGCTAAG".repeat(60)].concat();
        let r = trim_stops_near_ends(&seqs, Range { start: 0, end: seqs.len() }, Dir::F, 300);
        assert_eq!(r.start, 6);
        let rc = reverse_complement(&seqs);
        let r = trim_stops_near_ends(&rc, Range { start: 0, end: rc.len() }, Dir::R, 300);
        assert_eq!(r.end, rc.len() - 6);
    }

    #[test]
    fn conflicting_evidence_is_unknown() {
        assert_eq!(decide_direction(Some(Dir::F), StrandHint::Reverse), None);
        assert_eq!(decide_direction(Some(Dir::R), StrandHint::Neither), Some(Dir::R));
        assert_eq!(decide_direction(None, StrandHint::Both), None);
    }
}
