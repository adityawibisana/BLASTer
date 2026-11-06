//! Nucleotide helpers: IUPAC codes, reverse complement, translation.

/// Bit mask of the bases an IUPAC code stands for (A=1, C=2, G=4, T=8).
pub fn iupac_mask(c: u8) -> u8 {
    match c.to_ascii_uppercase() {
        b'A' => 1,
        b'C' => 2,
        b'G' => 4,
        b'T' | b'U' => 8,
        b'R' => 1 | 4,
        b'Y' => 2 | 8,
        b'S' => 2 | 4,
        b'W' => 1 | 8,
        b'K' => 4 | 8,
        b'M' => 1 | 2,
        b'B' => 2 | 4 | 8,
        b'D' => 1 | 4 | 8,
        b'H' => 1 | 2 | 8,
        b'V' => 1 | 2 | 4,
        b'N' => 15,
        _ => 0,
    }
}

pub fn is_iupac(c: u8) -> bool {
    iupac_mask(c) != 0
}

/// A base is ambiguous when it is anything other than A, C, G or T.
pub fn is_ambiguous(c: u8) -> bool {
    !matches!(c, b'A' | b'C' | b'G' | b'T')
}

fn complement(c: u8) -> u8 {
    match c {
        b'A' => b'T',
        b'T' => b'A',
        b'C' => b'G',
        b'G' => b'C',
        b'R' => b'Y',
        b'Y' => b'R',
        b'K' => b'M',
        b'M' => b'K',
        b'B' => b'V',
        b'V' => b'B',
        b'D' => b'H',
        b'H' => b'D',
        other => other, // S, W, N are their own complement
    }
}

pub fn reverse_complement(seq: &[u8]) -> Vec<u8> {
    seq.iter().rev().map(|&c| complement(c.to_ascii_uppercase())).collect()
}

pub fn gc_percent(seq: &[u8]) -> f64 {
    let acgt = seq.iter().filter(|c| matches!(c, b'A' | b'C' | b'G' | b'T')).count();
    if acgt == 0 {
        return 0.0;
    }
    let gc = seq.iter().filter(|c| matches!(c, b'G' | b'C')).count();
    gc as f64 * 100.0 / acgt as f64
}

/// Number of stop codons when translating `seq` from `frame` (0..3) with the
/// invertebrate mitochondrial code (NCBI table 5), whose only stops are TAA and TAG.
pub fn stop_codons(seq: &[u8], frame: usize) -> usize {
    seq.get(frame..)
        .unwrap_or(&[])
        .chunks_exact(3)
        .filter(|c| c[0] == b'T' && c[1] == b'A' && (c[2] == b'A' || c[2] == b'G'))
        .count()
}

/// Fewest stop codons over the three forward frames, and the frame that has it.
pub fn best_forward_frame(seq: &[u8]) -> (usize, usize) {
    (0..3).map(|f| (stop_codons(seq, f), f)).min().unwrap_or((0, 0))
}

/// Translate with NCBI table 5. Codons containing ambiguity codes become `X`.
pub fn translate_table5(seq: &[u8], frame: usize) -> String {
    const BASES: &[u8; 4] = b"TCAG";
    // Standard code in TCAG order, with the table 5 changes applied below.
    const STANDARD: &[u8; 64] = b"FFLLSSSSYY**CC*WLLLLPPPPHHQQRRRRIIIMTTTTNNKKSSRRVVVVAAAADDEEGGGG";
    let idx = |c: u8| BASES.iter().position(|&b| b == c);
    seq.get(frame..)
        .unwrap_or(&[])
        .chunks_exact(3)
        .map(|c| match (idx(c[0]), idx(c[1]), idx(c[2])) {
            (Some(a), Some(b), Some(d)) => {
                let codon = [c[0], c[1], c[2]];
                match &codon {
                    b"TGA" => 'W',
                    b"ATA" => 'M',
                    b"AGA" | b"AGG" => 'S',
                    _ => STANDARD[a * 16 + b * 4 + d] as char,
                }
            }
            _ => 'X',
        })
        .collect()
}

/// FNV-1a hash, stable across builds; used to notice when a sequence changes.
pub fn fingerprint(seq: &[u8]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in seq {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revcomp_handles_iupac() {
        assert_eq!(reverse_complement(b"ACGTRYN"), b"NRYACGT");
    }

    #[test]
    fn table5_differs_from_standard() {
        assert_eq!(translate_table5(b"TGAATAAGATAA", 0), "WMS*");
        assert_eq!(stop_codons(b"TGAATAAGATAA", 0), 1);
    }

    #[test]
    fn gc_ignores_ambiguous() {
        assert_eq!(gc_percent(b"GGCCAATTNN"), 50.0);
    }
}
