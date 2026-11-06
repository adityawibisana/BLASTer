//! Quality trimming: Mott's modified algorithm, then the longest region whose
//! share of high-quality bases is above the threshold.

/// Percentage of bases with quality >= `hq`.
pub fn hq_percent(quals: &[u8], hq: u8) -> f64 {
    if quals.is_empty() {
        return 0.0;
    }
    quals.iter().filter(|&&q| q >= hq).count() as f64 * 100.0 / quals.len() as f64
}

/// Mott's modified trimming within `[lo, hi)`: the segment with the highest sum of
/// `cutoff - error_probability(q)`. Returns an empty range at `lo` if no base scores
/// above the cutoff.
pub fn mott(quals: &[u8], lo: usize, hi: usize, cutoff: f64) -> (usize, usize) {
    let mut best = (0.0, lo, lo);
    let mut run = 0.0;
    let mut run_start = lo;
    for i in lo..hi {
        let score = cutoff - 10f64.powf(-(quals[i] as f64) / 10.0);
        if run <= 0.0 {
            run = 0.0;
            run_start = i;
        }
        run += score;
        if run > best.0 {
            best = (run, run_start, i + 1);
        }
    }
    (best.1, best.2)
}

/// Longest region within `[lo, hi)` whose share of bases with quality >= `hq` is
/// strictly above `threshold_pct`. Ties go to the region with the higher share.
///
/// Each base scores `1 - t` if high quality and `-t` otherwise; a region passes
/// exactly when its total is positive. The longest positive-sum region is found in
/// linear time from prefix sums.
pub fn longest_hq_region(quals: &[u8], lo: usize, hi: usize, hq: u8, threshold_pct: f64) -> (usize, usize) {
    if hi <= lo {
        return (lo, lo);
    }
    let t = (threshold_pct * 10.0).round() as i64; // per mille
    let w = |q: u8| if q >= hq { 1000 - t } else { -t };
    let n = hi - lo;
    let mut prefix = Vec::with_capacity(n + 1);
    prefix.push(0i64);
    for &q in &quals[lo..hi] {
        prefix.push(prefix.last().unwrap() + w(q));
    }
    // Candidate starts: prefix values strictly lower than every earlier one.
    let mut starts: Vec<usize> = Vec::new();
    for i in 0..=n {
        if starts.last().is_none_or(|&s| prefix[i] < prefix[s]) {
            starts.push(i);
        }
    }
    let mut best_len = 0;
    for j in (0..=n).rev() {
        while let Some(&s) = starts.last() {
            if s >= j {
                // A start at or after this end can't pair with it or any earlier end.
                starts.pop();
            } else if prefix[j] > prefix[s] {
                best_len = best_len.max(j - s);
                starts.pop();
            } else {
                break;
            }
        }
    }
    if best_len == 0 {
        return (lo, lo);
    }
    let mut best = (i64::MIN, 0);
    for i in 0..=n - best_len {
        let sum = prefix[i + best_len] - prefix[i];
        if sum > 0 && sum > best.0 {
            best = (sum, i);
        }
    }
    (lo + best.1, lo + best.1 + best_len)
}

/// Bases at each end of the kept region that must also be above the threshold.
pub const END_WINDOW: usize = 20;

/// Move each end inward until its outermost `END_WINDOW` bases are above the
/// threshold too. Base-call errors cluster at read ends, so a region can pass
/// overall while a weak stretch sits at one end.
pub fn tidy_ends(quals: &[u8], mut s: usize, mut e: usize, hq: u8, threshold_pct: f64) -> (usize, usize) {
    while e - s >= END_WINDOW && hq_percent(&quals[s..s + END_WINDOW], hq) <= threshold_pct {
        s += 1;
    }
    while e - s >= END_WINDOW && hq_percent(&quals[e - END_WINDOW..e], hq) <= threshold_pct {
        e -= 1;
    }
    if e - s < END_WINDOW { (s, s) } else { (s, e) }
}

/// Full automatic trim inside the primer limits.
pub fn auto_trim(quals: &[u8], lo: usize, hi: usize, hq: u8, threshold_pct: f64, mott_cutoff: f64) -> (usize, usize) {
    let (ms, me) = mott(quals, lo, hi.min(quals.len()), mott_cutoff);
    let (s, e) = longest_hq_region(quals, ms, me, hq, threshold_pct);
    tidy_ends(quals, s, e, hq, threshold_pct)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brute(quals: &[u8], hq: u8, t: f64) -> usize {
        let mut best = 0;
        for i in 0..quals.len() {
            for j in i + 1..=quals.len() {
                if hq_percent(&quals[i..j], hq) > t {
                    best = best.max(j - i);
                }
            }
        }
        best
    }

    #[test]
    fn longest_region_matches_brute_force() {
        let mut seed = 7u32;
        for _ in 0..200 {
            let quals: Vec<u8> = (0..60)
                .map(|_| {
                    seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                    if (seed >> 16) % 3 == 0 { 10 } else { 45 }
                })
                .collect();
            let (s, e) = longest_hq_region(&quals, 0, quals.len(), 40, 70.0);
            assert_eq!(e - s, brute(&quals, 40, 70.0));
            if e > s {
                assert!(hq_percent(&quals[s..e], 40) > 70.0);
            }
        }
    }

    #[test]
    fn mott_drops_low_quality_ends() {
        let quals = [[5u8; 20].as_slice(), &[50; 100], &[5; 30]].concat();
        assert_eq!(mott(&quals, 0, quals.len(), 0.05), (20, 120));
    }

    #[test]
    fn weak_end_is_tidied() {
        // 30 alternating bases (50% HQ) then 200 good ones: overall passes, start must move.
        let quals: Vec<u8> = (0..30).map(|i| if i % 2 == 0 { 45 } else { 25 }).chain([50; 200]).collect();
        let (s, e) = auto_trim(&quals, 0, quals.len(), 40, 70.0, 0.05);
        assert!(s >= 20, "start {s}");
        assert_eq!(e, 230);
        assert!(hq_percent(&quals[s..s + END_WINDOW], 40) > 70.0);
    }

    #[test]
    fn exact_threshold_is_not_above() {
        // 7 of 10 high quality is exactly 70%, which does not pass "> 70%".
        let quals = [45, 45, 45, 45, 45, 45, 45, 10, 10, 10];
        let (s, e) = longest_hq_region(&quals, 0, 10, 40, 70.0);
        assert!(hq_percent(&quals[s..e], 40) > 70.0);
        assert!(e - s < 10);
    }
}
