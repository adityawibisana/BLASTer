//! FASTA output. The first word of each header is the unique sample ID, which is
//! how BLAST results are matched back to samples.

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
