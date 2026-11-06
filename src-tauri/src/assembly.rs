//! Forward/reverse assembly: overlap alignment of the forward read with the
//! reverse-complemented reverse read, then a quality-weighted consensus.

use crate::seq::iupac_mask;

/// Minimum aligned columns and identity for two reads to count as overlapping.
pub const MIN_OVERLAP: usize = 50;
pub const MIN_IDENTITY: f64 = 90.0;

const MATCH: i32 = 2;
const MISMATCH: i32 = -3;
const GAP: i32 = -5;

#[derive(Debug, Clone, PartialEq)]
pub struct Consensus {
    pub seq: Vec<u8>,
    pub quals: Vec<u8>,
    pub overlap: usize,
    pub identity: f64,
    pub disagreements: usize,
    /// Consensus positions where both reads were high quality but disagreed.
    pub conflicts: Vec<usize>,
    /// Region of each input covered by the overlap, `[start, end)`.
    pub forward_span: (usize, usize),
    pub reverse_span: (usize, usize),
}

impl Consensus {
    pub fn overlaps(&self) -> bool {
        self.overlap >= MIN_OVERLAP && self.identity >= MIN_IDENTITY
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Step {
    Diag,
    Up,   // base from `a` only (gap in b)
    Left, // base from `b` only (gap in a)
}

/// IUPAC code for two different bases.
fn ambiguity(x: u8, y: u8) -> u8 {
    let mask = iupac_mask(x) | iupac_mask(y);
    match mask {
        1 => b'A',
        2 => b'C',
        4 => b'G',
        8 => b'T',
        5 => b'R',
        10 => b'Y',
        6 => b'S',
        9 => b'W',
        12 => b'K',
        3 => b'M',
        14 => b'B',
        13 => b'D',
        11 => b'H',
        7 => b'V',
        _ => b'N',
    }
}

/// Assemble forward read `a` with the reverse read `b`, both already oriented
/// 5'->3' along the gene (`b` reverse-complemented, its qualities reversed).
pub fn assemble(a: &[u8], qa: &[u8], b: &[u8], qb: &[u8], hq: u8) -> Consensus {
    let (n, m) = (a.len(), b.len());
    let w = m + 1;
    // Overlap alignment: leading and trailing overhangs are free.
    let mut score = vec![0i32; (n + 1) * w];
    let mut step = vec![Step::Diag; (n + 1) * w];
    for i in 1..=n {
        step[i * w] = Step::Up;
        for j in 1..=m {
            let same = iupac_mask(a[i - 1]) & iupac_mask(b[j - 1]) != 0;
            let d = score[(i - 1) * w + j - 1] + if same { MATCH } else { MISMATCH };
            let u = score[(i - 1) * w + j] + GAP;
            let l = score[i * w + j - 1] + GAP;
            let (s, st) = if d >= u && d >= l { (d, Step::Diag) } else if u >= l { (u, Step::Up) } else { (l, Step::Left) };
            score[i * w + j] = s;
            step[i * w + j] = st;
        }
    }
    for j in 1..=m {
        step[j] = Step::Left;
    }
    // Best end on the last row or last column.
    let mut end = (n, m);
    let mut best = i32::MIN;
    for j in 0..=m {
        if score[n * w + j] > best {
            best = score[n * w + j];
            end = (n, j);
        }
    }
    for i in 0..=n {
        if score[i * w + m] > best {
            best = score[i * w + m];
            end = (i, m);
        }
    }

    // Trace back the aligned columns.
    let mut cols: Vec<(Option<usize>, Option<usize>)> = Vec::new();
    let (mut i, mut j) = end;
    while i > 0 && j > 0 {
        match step[i * w + j] {
            Step::Diag => {
                cols.push((Some(i - 1), Some(j - 1)));
                i -= 1;
                j -= 1;
            }
            Step::Up => {
                cols.push((Some(i - 1), None));
                i -= 1;
            }
            Step::Left => {
                cols.push((None, Some(j - 1)));
                j -= 1;
            }
        }
    }
    cols.reverse();
    let (start_a, start_b) = (i, j);
    let (end_a, end_b) = end;

    let mut seq = Vec::with_capacity(n + m);
    let mut quals = Vec::with_capacity(n + m);
    // Overhang before the overlap: only one read has bases there.
    if start_a > 0 {
        seq.extend_from_slice(&a[..start_a]);
        quals.extend_from_slice(&qa[..start_a]);
    } else {
        seq.extend_from_slice(&b[..start_b]);
        quals.extend_from_slice(&qb[..start_b]);
    }
    let mut matches = 0;
    let mut disagreements = 0;
    let mut conflicts = Vec::new();
    for &(ca, cb) in &cols {
        match (ca, cb) {
            (Some(x), Some(y)) => {
                let (ba, bb, xa, xb) = (a[x], b[y], qa[x], qb[y]);
                if iupac_mask(ba) & iupac_mask(bb) != 0 && ba == bb {
                    matches += 1;
                    seq.push(ba);
                    quals.push(xa.max(xb));
                } else {
                    disagreements += 1;
                    if xa >= hq && xb >= hq {
                        conflicts.push(seq.len());
                    }
                    let base = if xa > xb {
                        ba
                    } else if xb > xa {
                        bb
                    } else {
                        ambiguity(ba, bb)
                    };
                    seq.push(base);
                    quals.push(xa.abs_diff(xb));
                }
            }
            // Base in one read, gap in the other: keep it only if it is reasonably sure.
            (Some(x), None) => {
                disagreements += 1;
                if qa[x] >= 20 {
                    seq.push(a[x]);
                    quals.push(qa[x]);
                }
            }
            (None, Some(y)) => {
                disagreements += 1;
                if qb[y] >= 20 {
                    seq.push(b[y]);
                    quals.push(qb[y]);
                }
            }
            (None, None) => {}
        }
    }
    // Overhang after the overlap.
    if end_a < n {
        seq.extend_from_slice(&a[end_a..]);
        quals.extend_from_slice(&qa[end_a..]);
    } else {
        seq.extend_from_slice(&b[end_b..]);
        quals.extend_from_slice(&qb[end_b..]);
    }
    let overlap = cols.len();
    Consensus {
        seq,
        quals,
        overlap,
        identity: if overlap > 0 { matches as f64 * 100.0 / overlap as f64 } else { 0.0 },
        disagreements,
        conflicts,
        forward_span: (start_a, end_a),
        reverse_span: (start_b, end_b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pseudo(len: usize, seed: u32) -> Vec<u8> {
        let mut s = seed;
        (0..len)
            .map(|_| {
                s = s.wrapping_mul(1103515245).wrapping_add(12345);
                b"ACGT"[((s >> 16) % 4) as usize]
            })
            .collect()
    }

    #[test]
    fn joins_offset_reads_into_full_barcode() {
        let barcode = pseudo(658, 3);
        let f = &barcode[..620]; // forward read misses the end
        let r = &barcode[40..]; // reverse read misses the start
        let c = assemble(f, &vec![50; f.len()], r, &vec![50; r.len()], 40);
        assert_eq!(c.seq, barcode);
        assert_eq!(c.overlap, 580);
        assert_eq!(c.disagreements, 0);
        assert!(c.overlaps());
    }

    #[test]
    fn higher_quality_base_wins_and_conflicts_are_recorded() {
        let barcode = pseudo(400, 9);
        let mut f = barcode.clone();
        let mut qf = vec![50u8; 400];
        f[100] = if f[100] == b'A' { b'C' } else { b'A' };
        qf[100] = 12; // low-quality error in F: R's base wins, no conflict
        let mut r = barcode.clone();
        let qr = vec![50u8; 400];
        r[200] = if r[200] == b'G' { b'T' } else { b'G' }; // high-quality disagreement
        let c = assemble(&f, &qf, &r, &qr, 40);
        assert_eq!(c.seq[100], barcode[100]);
        assert_eq!(c.disagreements, 2);
        assert_eq!(c.conflicts, vec![200]);
    }

    #[test]
    fn insertion_in_one_read_is_dropped_when_low_quality() {
        let barcode = pseudo(300, 5);
        let mut f = barcode.clone();
        f.insert(150, b'T');
        let mut qf = vec![50u8; 301];
        qf[150] = 8;
        let c = assemble(&f, &qf, &barcode, &vec![50; 300], 40);
        assert_eq!(c.seq, barcode);
    }

    #[test]
    fn unrelated_reads_do_not_overlap() {
        let a = pseudo(500, 1);
        let b = pseudo(500, 2);
        assert!(!assemble(&a, &vec![50; 500], &b, &vec![50; 500], 40).overlaps());
    }
}
