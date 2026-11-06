//! Forward/reverse pairs: suggestions, confirmation, and the consensus that
//! replaces the forward read's derived values.

use anyhow::{Result, bail};

use crate::analysis;
use crate::assembly::{self, Consensus};
use crate::model::{Derived, Flag, PairInfo, PairSuggestion, Project, Read, Settings};
use crate::primers::Dir;
use crate::seq;

const MARKERS_F: &[&str] = &["F", "FWD", "FORWARD", "LCO", "LCO1490", "LEPF1"];
const MARKERS_R: &[&str] = &["R", "REV", "REVERSE", "HCO", "HCO2198", "LEPR1"];

/// Split a sample ID into its stem and the direction its name claims, e.g.
/// "S01_F" -> ("S01", F), "VN12-HCO" -> ("VN12", R). With a custom pattern
/// ("_F,_R") only that pattern is used. IDs without a marker give `None`.
pub fn name_marker(id: &str, pattern: &str) -> Option<(String, Dir)> {
    if let Some((f, r)) = pattern.split_once(',') {
        let (f, r) = (f.trim(), r.trim());
        if !f.is_empty() && !r.is_empty() {
            for (token, dir) in [(f, Dir::F), (r, Dir::R)] {
                if id.len() > token.len() && id.to_lowercase().ends_with(&token.to_lowercase()) {
                    return Some((id[..id.len() - token.len()].to_string(), dir));
                }
            }
            return None;
        }
    }
    let cut = id.rfind(['-', '_', '.', ' '])?;
    let (stem, token) = (&id[..cut], id[cut + 1..].to_ascii_uppercase());
    if stem.is_empty() {
        return None;
    }
    if MARKERS_F.contains(&token.as_str()) {
        Some((stem.to_string(), Dir::F))
    } else if MARKERS_R.contains(&token.as_str()) {
        Some((stem.to_string(), Dir::R))
    } else {
        None
    }
}

/// A read's final sequence and qualities, 5'->3' along the gene (single-read view).
fn oriented(read: &Read) -> (Vec<u8>, Vec<u8>) {
    let d = &read.derived;
    let Some(t) = d.trim else { return (Vec::new(), Vec::new()) };
    let mut quals = read.quals[t.start.min(read.quals.len())..t.end.min(read.quals.len())].to_vec();
    if d.direction == Some(Dir::R) {
        quals.reverse();
    }
    (d.final_seq.as_bytes().to_vec(), quals)
}

fn try_assemble(f: &Read, r: &Read, s: &Settings) -> Consensus {
    let (a, qa) = oriented(f);
    let (b, qb) = oriented(r);
    assembly::assemble(&a, &qa, &b, &qb, s.hq_quality)
}

fn is_free(p: &Project, id: &str) -> bool {
    p.reads.iter().any(|r| r.id == id && r.pair.is_none() && r.paired_with.is_none())
}

/// Recompute the list of suggested pairs.
pub fn suggest(p: &mut Project) {
    let s = p.settings.clone();
    let mut out = Vec::new();
    let free: Vec<&Read> = p.reads.iter().filter(|r| r.pair.is_none() && r.paired_with.is_none()).collect();
    for f in free.iter().filter(|r| r.derived.direction == Some(Dir::F)) {
        let Some((stem_f, _)) = name_marker(&f.id, &s.fr_pattern) else { continue };
        for r in free.iter().filter(|r| r.derived.direction == Some(Dir::R)) {
            let Some((stem_r, _)) = name_marker(&r.id, &s.fr_pattern) else { continue };
            if !stem_f.eq_ignore_ascii_case(&stem_r) {
                continue;
            }
            if p.rejected_pairs.iter().any(|(a, b)| *a == f.id && *b == r.id) {
                continue;
            }
            let c = try_assemble(f, r, &s);
            if c.overlaps() {
                out.push(PairSuggestion {
                    forward_id: f.id.clone(),
                    reverse_id: r.id.clone(),
                    overlap: c.overlap,
                    identity: c.identity,
                });
            }
        }
    }
    p.pair_suggestions = out;
}

/// Pair two reads: `forward_id` becomes the sample, `reverse_id` is folded into it.
/// The reads' directions are set to match their roles.
pub fn confirm(p: &mut Project, forward_id: &str, reverse_id: &str) -> Result<()> {
    if forward_id == reverse_id {
        bail!("a read cannot be paired with itself");
    }
    for id in [forward_id, reverse_id] {
        if !is_free(p, id) {
            bail!("{id} is already in a pair");
        }
    }
    let s = p.settings.clone();
    for (id, dir) in [(forward_id, Dir::F), (reverse_id, Dir::R)] {
        let r = p.reads.iter_mut().find(|r| r.id == id).unwrap();
        if r.derived.direction != Some(dir) {
            r.direction_manual = Some(dir);
            analysis::refresh(r, &s);
        }
    }
    let f = p.reads.iter_mut().find(|r| r.id == forward_id).unwrap();
    f.pair = Some(PairInfo { reverse_id: reverse_id.to_string(), ..Default::default() });
    let r = p.reads.iter_mut().find(|r| r.id == reverse_id).unwrap();
    r.paired_with = Some(forward_id.to_string());
    refresh_pair(p, forward_id);
    p.pair_suggestions.retain(|x| x.forward_id != forward_id && x.reverse_id != reverse_id);
    Ok(())
}

pub fn reject(p: &mut Project, forward_id: &str, reverse_id: &str) {
    p.rejected_pairs.push((forward_id.to_string(), reverse_id.to_string()));
    p.pair_suggestions.retain(|x| !(x.forward_id == forward_id && x.reverse_id == reverse_id));
}

/// Split a pair back into two single reads.
pub fn unpair(p: &mut Project, forward_id: &str) {
    let s = p.settings.clone();
    let Some(rev) = p.reads.iter_mut().find(|r| r.id == forward_id).and_then(|f| f.pair.take()).map(|x| x.reverse_id)
    else {
        return;
    };
    for id in [forward_id, rev.as_str()] {
        if let Some(r) = p.reads.iter_mut().find(|r| r.id == id) {
            r.paired_with = None;
            analysis::refresh(r, &s);
        }
    }
    suggest(p);
}

/// Swap the roles: the reverse read becomes the forward one and vice versa.
pub fn swap(p: &mut Project, forward_id: &str) -> Result<()> {
    let Some(rev) = p.reads.iter().find(|r| r.id == forward_id).and_then(|f| f.pair.clone()).map(|x| x.reverse_id)
    else {
        bail!("{forward_id} is not in a pair");
    };
    let s = p.settings.clone();
    if let Some(f) = p.reads.iter_mut().find(|r| r.id == forward_id) {
        f.pair = None;
    }
    for id in [forward_id, rev.as_str()] {
        if let Some(r) = p.reads.iter_mut().find(|r| r.id == id) {
            r.paired_with = None;
            r.direction_manual = None;
            analysis::refresh(r, &s);
        }
    }
    confirm(p, &rev, forward_id)
}

pub fn set_use_only(p: &mut Project, forward_id: &str, use_only: Option<Dir>) {
    if let Some(info) = p.reads.iter_mut().find(|r| r.id == forward_id).and_then(|f| f.pair.as_mut()) {
        info.use_only = use_only;
    }
    refresh_pair(p, forward_id);
}

fn consensus_derived(single: &Derived, c: &Consensus, f: &Read, r: &Read, s: &Settings) -> Derived {
    let len = c.seq.len();
    let hq_pct = crate::trim::hq_percent(&c.quals, s.hq_quality);
    let (stops, frame) = seq::best_forward_frame(&c.seq);
    let ambiguous = c.seq.iter().filter(|&&b| seq::is_ambiguous(b)).count();
    let qc_failed = len < s.min_length || hq_pct <= s.hq_threshold;
    let mut flags = Vec::new();
    if qc_failed {
        flags.push(Flag::FailedQc);
    }
    if len > 0 {
        if len < s.consensus_len_min || len > s.consensus_len_max {
            flags.push(Flag::Length);
        }
        if stops > 0 {
            flags.push(Flag::StopCodon);
        }
        if ambiguous as f64 * 100.0 / len as f64 > s.ambiguity_max_pct {
            flags.push(Flag::Ambiguous);
        }
    }
    if !c.conflicts.is_empty() {
        flags.push(Flag::FrConflict);
    }
    let edited = |x: &Read| x.manual_trim.is_some() || x.direction_manual.is_some();
    if edited(f) || edited(r) {
        flags.push(Flag::Edited);
    }
    Derived {
        trim: single.trim,
        direction: Some(Dir::F),
        final_len: len,
        hq_pct,
        gc_pct: seq::gc_percent(&c.seq),
        stop_codons: stops,
        reading_frame: frame,
        ambiguous,
        qc_failed,
        eligible: !f.excluded && len > 0 && (!qc_failed || f.include_failed),
        fingerprint: seq::fingerprint(&c.seq),
        final_seq: String::from_utf8_lossy(&c.seq).into_owned(),
        flags,
    }
}

/// Recompute a pair's consensus and store it as the forward read's derived values.
pub fn refresh_pair(p: &mut Project, forward_id: &str) {
    let s = p.settings.clone();
    let Some(fi) = p.reads.iter().position(|r| r.id == forward_id) else { return };
    let Some(info) = p.reads[fi].pair.clone() else { return };
    let Some(ri) = p.reads.iter().position(|r| r.id == info.reverse_id) else {
        p.reads[fi].pair = None;
        return;
    };
    analysis::refresh(&mut p.reads[fi], &s);
    analysis::refresh(&mut p.reads[ri], &s);
    let c = try_assemble(&p.reads[fi], &p.reads[ri], &s);
    let overlaps = c.overlaps();
    let derived = if overlaps {
        consensus_derived(&p.reads[fi].derived, &c, &p.reads[fi], &p.reads[ri], &s)
    } else {
        let mut d = match info.use_only {
            Some(Dir::R) => {
                // Search the reverse read alone, under the forward read's sample ID.
                let mut d = p.reads[ri].derived.clone();
                d.eligible = !p.reads[fi].excluded && d.final_len > 0 && (!d.qc_failed || p.reads[fi].include_failed);
                d
            }
            Some(Dir::F) => p.reads[fi].derived.clone(),
            None => {
                let mut d = p.reads[fi].derived.clone();
                d.eligible = false;
                d
            }
        };
        if info.use_only.is_none() {
            d.flags.push(Flag::NoOverlap);
        }
        d
    };
    let rd = &mut p.reads[ri].derived;
    rd.eligible = false;
    let f = &mut p.reads[fi];
    f.derived = derived;
    f.pair = Some(PairInfo {
        reverse_id: info.reverse_id,
        overlap: c.overlap,
        disagreements: c.disagreements,
        conflicts: c.conflicts,
        identity: c.identity,
        no_overlap: !overlaps,
        use_only: if overlaps { None } else { info.use_only },
    });
}

/// Refresh one read after a change, including the pair it belongs to.
pub fn refresh_read(p: &mut Project, id: &str) {
    let s = p.settings.clone();
    let (forward, in_pair) = match p.reads.iter().find(|r| r.id == id) {
        Some(r) if r.pair.is_some() => (r.id.clone(), true),
        Some(r) if r.paired_with.is_some() => (r.paired_with.clone().unwrap_or_default(), true),
        Some(_) => (String::new(), false),
        None => return,
    };
    if in_pair {
        refresh_pair(p, &forward);
    } else if let Some(r) = p.reads.iter_mut().find(|r| r.id == id) {
        analysis::refresh(r, &s);
    }
}

/// Refresh every pair (after settings change or loading).
pub fn refresh_all_pairs(p: &mut Project) {
    let ids: Vec<String> = p.reads.iter().filter(|r| r.pair.is_some()).map(|r| r.id.clone()).collect();
    for id in ids {
        refresh_pair(p, &id);
    }
    for r in p.reads.iter_mut().filter(|r| r.paired_with.is_some()) {
        r.derived.eligible = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_direction_markers() {
        assert_eq!(name_marker("S01_F", ""), Some(("S01".into(), Dir::F)));
        assert_eq!(name_marker("VN12-HCO", ""), Some(("VN12".into(), Dir::R)));
        assert_eq!(name_marker("A.rev", ""), Some(("A".into(), Dir::R)));
        // Isolate numbers are not markers.
        assert_eq!(name_marker("VN236-1", ""), None);
        assert_eq!(name_marker("VN220", ""), None);
        // A custom pattern replaces the built-in markers.
        assert_eq!(name_marker("S7fw", "fw,rv"), Some(("S7".into(), Dir::F)));
        assert_eq!(name_marker("S7_F", "fw,rv"), None);
    }
}
