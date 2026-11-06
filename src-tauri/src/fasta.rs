//! FASTA output. The first word of each header is the unique sample ID, which is
//! how BLAST results are matched back to samples.

use crate::model::Read;
use crate::primers::Dir;

pub fn header(read: &Read) -> String {
    let d = &read.derived;
    let range = d.trim.map(|r| format!("bases {}-{}", r.start + 1, r.end)).unwrap_or_default();
    let dir = match d.direction {
        Some(Dir::F) => "F",
        Some(Dir::R) => "R",
        None => "unknown",
    };
    let site = |s: &Option<crate::primers::SiteHit>| s.as_ref().map_or("none-found".to_string(), |h| h.primer.clone());
    format!(
        "{} {} dir:{} primers:{}/{}",
        read.id,
        range,
        dir,
        site(&read.primer.start_site),
        site(&read.primer.end_site)
    )
}

pub fn record(read: &Read) -> String {
    let mut out = format!(">{}\n", header(read));
    for line in read.derived.final_seq.as_bytes().chunks(60) {
        out.push_str(std::str::from_utf8(line).unwrap_or_default());
        out.push('\n');
    }
    out
}

pub fn multi<'a>(reads: impl IntoIterator<Item = &'a Read>) -> String {
    reads.into_iter().map(record).collect()
}
