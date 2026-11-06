//! FASTA input and output. On output, the first word of each header is the unique
//! sample ID, which is how BLAST results are matched back to samples.

use crate::model::Read;
use crate::primers::{Dir, SiteHit};

fn site(s: &Option<SiteHit>) -> String {
    s.as_ref().map_or("none-found".to_string(), |h| h.primer.clone())
}

fn range(read: &Read) -> String {
    read.derived.trim.map(|r| format!("{}-{}", r.start + 1, r.end)).unwrap_or_default()
}

/// Header for a sample. `partner` is the reverse read when `read` is the forward
/// read of an assembled pair.
pub fn header(read: &Read, partner: Option<&Read>) -> String {
    if let (Some(info), Some(rev)) = (&read.pair, partner) {
        if !info.no_overlap {
            return format!(
                "{} consensus F:{} R:{} overlap={} primers:{}/{}",
                read.id,
                range(read),
                range(rev),
                info.overlap,
                site(&rev.primer.end_site),
                site(&read.primer.end_site)
            );
        }
        if info.use_only == Some(Dir::R) {
            return format!("{} reverse-only {} bases {} primers:{}/{}", read.id, rev.id, range(rev), site(&rev.primer.start_site), site(&rev.primer.end_site));
        }
    }
    let dir = match read.derived.direction {
        Some(Dir::F) => "F",
        Some(Dir::R) => "R",
        None => "unknown",
    };
    format!(
        "{} bases {} dir:{} primers:{}/{}",
        read.id,
        range(read),
        dir,
        site(&read.primer.start_site),
        site(&read.primer.end_site)
    )
}

pub fn partner<'a>(all: &'a [Read], read: &Read) -> Option<&'a Read> {
    let id = &read.pair.as_ref()?.reverse_id;
    all.iter().find(|r| &r.id == id)
}

pub fn record(all: &[Read], read: &Read) -> String {
    let mut out = format!(">{}\n", header(read, partner(all, read)));
    for line in read.derived.final_seq.as_bytes().chunks(60) {
        out.push_str(std::str::from_utf8(line).unwrap_or_default());
        out.push('\n');
    }
    out
}

/// Records for `selected`, looking up pair partners in `all`.
pub fn multi<'a>(all: &[Read], selected: impl IntoIterator<Item = &'a Read>) -> String {
    selected.into_iter().map(|r| record(all, r)).collect()
}

// ---------- input ----------

/// Quality given to every base of a FASTA read: the highest Phred value the
/// high-quality setting allows, so every base counts as high quality.
pub const FASTA_QUAL: u8 = 60;

pub const EXTENSIONS: &[&str] = &["fasta", "fas", "fa", "fna", "ffn", "seq"];

pub fn is_fasta_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    EXTENSIONS.iter().any(|e| lower.ends_with(&format!(".{e}")))
}

#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    /// First word of the header.
    pub name: String,
    /// Rest of the header, if any.
    pub description: Option<String>,
    /// Uppercase IUPAC bases; gaps removed and U read as T.
    pub bases: Vec<u8>,
}

/// Parse a (multi-)FASTA file. A record that is empty or holds characters that are
/// not DNA bases comes back as `Err((name, reason))`; the others are kept.
pub fn parse(bytes: &[u8]) -> Result<Vec<Result<Record, (String, String)>>, String> {
    let text = String::from_utf8_lossy(bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let body = text.trim_start();
    if !body.starts_with('>') {
        return Err("not a FASTA file: it does not start with a '>' header line".into());
    }
    let mut out = Vec::new();
    for (n, chunk) in body[1..].split("\n>").enumerate() {
        let (header, seq) = chunk.split_once('\n').unwrap_or((chunk, ""));
        let header = header.trim();
        let (name, description) = match header.split_once(char::is_whitespace) {
            Some((a, b)) => (a.to_string(), Some(b.trim().to_string()).filter(|d| !d.is_empty())),
            None => (header.to_string(), None),
        };
        let name = if name.is_empty() { format!("sequence_{}", n + 1) } else { name };
        let mut bases = Vec::with_capacity(seq.len());
        let mut bad = None;
        for c in seq.bytes() {
            match c.to_ascii_uppercase() {
                c if c.is_ascii_whitespace() || matches!(c, b'-' | b'.' | b'*' | b'0'..=b'9') => {}
                b'U' => bases.push(b'T'),
                c @ (b'A' | b'C' | b'G' | b'T' | b'R' | b'Y' | b'K' | b'M' | b'S' | b'W' | b'B' | b'D' | b'H' | b'V'
                | b'N') => bases.push(c),
                c => {
                    bad = Some(c);
                    break;
                }
            }
        }
        out.push(match bad {
            Some(c) => Err((name, format!("'{}' is not a DNA base", c as char))),
            None if bases.is_empty() => Err((name, "the record has no sequence".into())),
            None => Ok(Record { name, description, bases }),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multi_fasta() {
        let text = b"\xef\xbb\xbf\r\n>S01_F some sample\r\nacgt-NN\r\nUUAC\r\n>S02\nACGT\n>bad\nACGXT\n>empty\n\n";
        let recs = parse(text).unwrap();
        assert_eq!(
            recs[0],
            Ok(Record { name: "S01_F".into(), description: Some("some sample".into()), bases: b"ACGTNNTTAC".to_vec() })
        );
        assert_eq!(recs[1].as_ref().unwrap().bases, b"ACGT");
        assert!(recs[2].as_ref().unwrap_err().1.contains("'X'"));
        assert!(recs[3].is_err());
    }

    #[test]
    fn rejects_non_fasta() {
        assert!(parse(b"ACGT").is_err());
        assert!(is_fasta_name("a/B.FASTA") && is_fasta_name("x.fa") && !is_fasta_name("x.ab1"));
    }
}
