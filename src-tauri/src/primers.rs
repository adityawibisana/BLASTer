//! Primer library and primer-site search in raw reads.
//!
//! An amplicon reads, on the forward strand: [F primer][barcode][revcomp(R primer)].
//! A forward read therefore may contain F (start) and revcomp(R) (end); a reverse
//! read may contain R (start) and revcomp(F) (end). Single Sanger reads normally
//! only show the end site, because the sequencing primer is not in its own read.

use serde::{Deserialize, Serialize};

use crate::seq::{iupac_mask, reverse_complement};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PrimerSet {
    pub name: String,
    pub forward_name: String,
    pub forward: String,
    pub reverse_name: String,
    pub reverse: String,
}

fn set(name: &str, fname: &str, f: &str, rname: &str, r: &str) -> PrimerSet {
    PrimerSet {
        name: name.into(),
        forward_name: fname.into(),
        forward: f.into(),
        reverse_name: rname.into(),
        reverse: r.into(),
    }
}

/// Built-in COI primer sets (Folmer barcode region). To be checked against the
/// original papers before release (README, "To verify" #3).
pub fn builtin_library() -> Vec<PrimerSet> {
    vec![
        set("Folmer et al. 1994", "LCO1490", "GGTCAACAAATCATAAAGATATTGG", "HCO2198", "TAAACTTCAGGGTGACCAAAAAATCA"),
        set("Meyer 2003 (degenerate)", "dgLCO1490", "GGTCAACAAATCATAAAGAYATYGG", "dgHCO2198", "TAAACTTCAGGGTGACCAAARAAYCA"),
        set("Astrin & Stüben 2008", "LCO1490-JJ", "CHACWAAYCATAAAGATATYGG", "HCO2198-JJ", "AWACTTCVGGRTGVCCAAARAATCA"),
        set("Hebert et al. 2004", "LepF1", "ATTCAACCAATCATAAAGATATTGG", "LepR1", "TAAACTTCTGGATGTCCAAAAAATCA"),
    ]
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Dir {
    F,
    R,
}

/// One primer site found in a raw read, in raw read coordinates.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SiteHit {
    pub primer: String,
    pub start: usize,
    pub end: usize,
    pub mismatches: usize,
    /// Primer bases that lie inside the read (less than the full length for a partial match).
    pub matched: usize,
    pub primer_length: usize,
    pub partial: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PrimerScan {
    pub set_name: Option<String>,
    /// Read direction implied by the primer sites found.
    pub orientation: Option<Dir>,
    /// Site near the start of the raw read.
    pub start_site: Option<SiteHit>,
    /// Site near the end of the raw read.
    pub end_site: Option<SiteHit>,
    /// Region between the primer sites, raw coordinates `[lo, hi)`.
    pub lo: usize,
    pub hi: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct MatchParams {
    pub max_mismatches: usize,
    pub min_partial: usize,
    /// Sites that would leave less than this many bases between them and the
    /// other end of the read are ignored (false matches in noisy read ends).
    pub min_region: usize,
}

/// Length of the primer's 3' part that is matched on its own when the rest of the
/// site is unreadable. That part decides where the cut falls.
const ANCHORED: usize = 16;

/// Which end of the pattern touches the barcode. That end is the primer's 3' end,
/// where mismatches matter most.
#[derive(Clone, Copy, PartialEq)]
enum Anchor {
    /// Primer as written (start of read): barcode follows its right end.
    Right,
    /// Reverse-complemented primer (end of read): barcode precedes its left end.
    Left,
}

const CORE: usize = 5;

/// Mismatch count between equal-length read and pattern slices, or `None` once it exceeds `limit`.
fn mismatches(read: &[u8], pat: &[u8], limit: usize) -> Option<usize> {
    let mut total = 0;
    for (r, p) in read.iter().zip(pat) {
        if iupac_mask(*r) & iupac_mask(*p) == 0 {
            total += 1;
            if total > limit {
                return None;
            }
        }
    }
    Some(total)
}

/// Every read substring that matches `pat` with at most `max_edits` substitutions,
/// insertions or deletions, as `(start, end, edits)`; one per end position.
fn edit_matches(read: &[u8], pat: &[u8], max_edits: usize) -> Vec<(usize, usize, usize)> {
    let k = pat.len();
    // Column j of the semi-global DP: cost and start of the best alignment of
    // pat[..i] ending at read position j (the start in the read is free).
    let mut cost: Vec<usize> = (0..=k).collect();
    let mut start: Vec<usize> = vec![0; k + 1];
    let mut out = Vec::new();
    for j in 1..=read.len() {
        let (mut diag_cost, mut diag_start) = (cost[0], start[0]);
        cost[0] = 0;
        start[0] = j;
        for i in 1..=k {
            let sub = diag_cost + usize::from(iupac_mask(read[j - 1]) & iupac_mask(pat[i - 1]) == 0);
            let skip_read = cost[i] + 1; // read base not in pattern
            let skip_pat = cost[i - 1] + 1; // pattern base missing from read
            let (c, s) = if sub <= skip_read && sub <= skip_pat {
                (sub, diag_start)
            } else if skip_read <= skip_pat {
                (skip_read, start[i])
            } else {
                (skip_pat, start[i - 1])
            };
            diag_cost = cost[i];
            diag_start = start[i];
            cost[i] = c;
            start[i] = s;
        }
        if cost[k] <= max_edits {
            out.push((start[k], j, cost[k]));
        }
    }
    out
}

/// Mismatches within the CORE bases at the primer's 3' end.
fn core_for(read: &[u8], pat: &[u8], anchor: Anchor) -> usize {
    let n = pat.len();
    let range = match anchor {
        Anchor::Right => n.saturating_sub(CORE)..n,
        Anchor::Left => 0..CORE.min(n),
    };
    range.filter(|&i| iupac_mask(read[i]) & iupac_mask(pat[i]) == 0).count()
}

fn find_site(read: &[u8], pat: &[u8], name: &str, anchor: Anchor, p: MatchParams) -> Option<SiteHit> {
    let m = pat.len();
    let n = read.len();
    // Allowed window starts, keeping at least `min_region` bases of barcode.
    let allowed = |pos: usize, len: usize| match anchor {
        Anchor::Right => pos + len + p.min_region <= n,
        Anchor::Left => pos >= p.min_region,
    };
    let mut best: Option<SiteHit> = None;
    if n >= m {
        for pos in (0..=n - m).filter(|&pos| allowed(pos, m)) {
            let window = &read[pos..pos + m];
            let Some(mm) = mismatches(window, pat, p.max_mismatches) else { continue };
            if core_for(window, pat, anchor) > 1 {
                continue;
            }
            if best.as_ref().is_none_or(|b| mm < b.mismatches) {
                best = Some(SiteHit {
                    primer: name.into(),
                    start: pos,
                    end: pos + m,
                    mismatches: mm,
                    matched: m,
                    primer_length: m,
                    partial: false,
                });
                if mm == 0 {
                    break;
                }
            }
        }
    }
    if best.is_some() {
        return best;
    }
    // Primer 3' part only: near the end of a read the far half of the site is often
    // unreadable, while the part next to the barcode is still clean.
    // One edit is allowed here, including an inserted or missing base, which is
    // common in homopolymer runs such as TTTTTT.
    if m > ANCHORED && n >= ANCHORED {
        let pat_part = match anchor {
            Anchor::Right => &pat[m - ANCHORED..],
            Anchor::Left => &pat[..ANCHORED],
        };
        let found = edit_matches(read, pat_part, 1)
            .into_iter()
            .filter(|&(s, e, _)| allowed(s, e - s))
            .min_by_key(|&(s, _, edits)| (edits, s));
        if let Some((s, e, edits)) = found {
            let (start, end) = match anchor {
                Anchor::Right => (s.saturating_sub(m - ANCHORED), e),
                Anchor::Left => (s, (s + m).min(n)),
            };
            return Some(SiteHit {
                primer: name.into(),
                start,
                end,
                mismatches: edits,
                matched: ANCHORED,
                primer_length: m,
                partial: true,
            });
        }
    }
    // Partial primer cut off by the read edge: only the part next to the barcode remains.
    for len in (p.min_partial..m.min(n + 1)).rev() {
        let limit = p.max_mismatches * len / m;
        let (read_part, pat_part, start) = match anchor {
            Anchor::Right => (&read[..len], &pat[m - len..], 0),
            Anchor::Left => (&read[n - len..], &pat[..len], n - len),
        };
        if !allowed(start, len) {
            continue;
        }
        if let Some(mm) = mismatches(read_part, pat_part, limit) {
            if core_for(read_part, pat_part, anchor) > 1 {
                continue;
            }
            return Some(SiteHit {
                primer: name.into(),
                start,
                end: start + len,
                mismatches: mm,
                matched: len,
                primer_length: m,
                partial: true,
            });
        }
    }
    None
}

fn upper(s: &str) -> Vec<u8> {
    s.bytes().map(|c| c.to_ascii_uppercase()).filter(|c| c.is_ascii_alphabetic()).collect()
}

struct Candidate {
    scan: PrimerScan,
    score: (usize, usize, isize, usize),
}

fn scan_orientation(read: &[u8], set: &PrimerSet, dir: Dir, p: MatchParams, set_rank: usize) -> Candidate {
    let (start_name, start_pat, end_name, end_pat) = match dir {
        Dir::F => (&set.forward_name, upper(&set.forward), &set.reverse_name, reverse_complement(&upper(&set.reverse))),
        Dir::R => (&set.reverse_name, upper(&set.reverse), &set.forward_name, reverse_complement(&upper(&set.forward))),
    };
    let mut start_site = find_site(read, &start_pat, start_name, Anchor::Right, p);
    let mut end_site = find_site(read, &end_pat, end_name, Anchor::Left, p);
    if let (Some(s), Some(e)) = (&start_site, &end_site) {
        if s.end > e.start {
            // Overlapping sites cannot both be real; keep the better one.
            if (s.partial, s.mismatches) <= (e.partial, e.mismatches) {
                end_site = None;
            } else {
                start_site = None;
            }
        }
    }
    let sites = [&start_site, &end_site];
    let found = sites.iter().filter(|s| s.is_some()).count();
    let full = sites.iter().filter(|s| s.as_ref().is_some_and(|h| !h.partial)).count();
    let mm: usize = sites.iter().filter_map(|s| s.as_ref().map(|h| h.mismatches)).sum();
    let lo = start_site.as_ref().map_or(0, |s| s.end);
    let hi = end_site.as_ref().map_or(read.len(), |s| s.start);
    Candidate {
        scan: PrimerScan {
            set_name: (found > 0).then(|| set.name.clone()),
            orientation: (found > 0).then_some(dir),
            start_site,
            end_site,
            lo,
            hi,
        },
        // More sites, more full sites, fewer mismatches, earlier library entry.
        score: (found, full, -(mm as isize), usize::MAX - set_rank),
    }
}

/// Search a raw read for the sites of every given primer set in both orientations
/// and return the best-supported result. With no sets (primer option "none"),
/// the whole read is kept.
pub fn scan(read: &[u8], sets: &[PrimerSet], p: MatchParams) -> PrimerScan {
    let mut best: Option<Candidate> = None;
    for (rank, set) in sets.iter().enumerate() {
        for dir in [Dir::F, Dir::R] {
            let c = scan_orientation(read, set, dir, p, rank);
            if c.score.0 > 0 && best.as_ref().is_none_or(|b| c.score > b.score) {
                best = Some(c);
            }
        }
    }
    best.map(|c| c.scan).unwrap_or(PrimerScan { lo: 0, hi: read.len(), ..Default::default() })
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: MatchParams = MatchParams { max_mismatches: 3, min_partial: 12, min_region: 100 };

    #[test]
    fn anchored_part_found_when_far_half_is_noise() {
        let lib = builtin_library();
        let barcode = b"ACGTTGCAACGTAGCTAGCTAGGATCGATCGATTTACG".repeat(5);
        // VN220's site: clean next to the barcode, four errors further out.
        let read = [barcode.as_slice(), b"TGATTTTTATGGTCACCAAAAAGTTTAAAAAA"].concat();
        let s = scan(&read, &lib[..1], P);
        let site = s.end_site.unwrap();
        assert!(site.partial);
        assert_eq!(s.hi, barcode.len());
    }

    #[test]
    fn edit_match_tolerates_one_insertion() {
        let hits = edit_matches(b"CCCCTGATTTTTATGGTCACCAAAA", b"TGATTTTTTGGTCACC", 1);
        assert!(hits.iter().any(|&(s, e, edits)| s == 4 && e == 21 && edits == 1));
        assert!(edit_matches(b"CCCCTGAAAAAATGGTCACCAAAA", b"TGATTTTTTGGTCACC", 1).is_empty());
    }

    #[test]
    fn ignores_site_that_leaves_too_little_barcode() {
        let lib = builtin_library();
        let end = reverse_complement(lib[0].reverse.as_bytes());
        let read = [b"ACGTACGTAC".as_slice(), &end, &b"ACGTTGCAACGTAGCTAGCTAGGATCG".repeat(10)].concat();
        assert_eq!(scan(&read, &lib[..1], P).end_site, None);
    }

    #[test]
    fn finds_reverse_primer_site_in_forward_read() {
        let lib = builtin_library();
        let barcode = b"ACGTTGCAACGTAGCTAGCTAGGATCGATCGATTTACG".repeat(5);
        let end = reverse_complement(lib[0].reverse.as_bytes());
        let read = [barcode.as_slice(), &end, b"NNNNACGTAC"].concat();
        let s = scan(&read, &lib[..1], P);
        assert_eq!(s.orientation, Some(Dir::F));
        assert_eq!(s.hi, barcode.len());
        assert_eq!(s.end_site.unwrap().mismatches, 0);
    }

    #[test]
    fn finds_partial_site_at_read_end() {
        let lib = builtin_library();
        let barcode = b"ACGTTGCAACGTAGCTAGCTAGGATCGATCGATTTACG".repeat(5);
        let end = reverse_complement(lib[0].forward.as_bytes());
        let read = [barcode.as_slice(), &end[..15]].concat();
        let s = scan(&read, &lib[..1], P);
        assert_eq!(s.orientation, Some(Dir::R));
        assert!(s.end_site.unwrap().partial);
        assert_eq!(s.hi, barcode.len());
    }

    #[test]
    fn nothing_found_keeps_whole_read() {
        let read = b"ACGT".repeat(100);
        let s = scan(&read, &builtin_library(), P);
        assert_eq!((s.lo, s.hi, s.orientation), (0, 400, None));
    }
}
