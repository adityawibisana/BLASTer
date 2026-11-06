//! PDF report, typeset with Typst from generated markup.

use std::collections::HashMap;

use anyhow::{Result, anyhow};
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime};
use typst::layout::PagedDocument;
use typst::syntax::{FileId, Source, VirtualPath};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};

use crate::blast::{self, HitDetail};
use crate::compile::species_of;
use crate::export::{primer_text, reason_text, status_text};
use crate::model::GroupStatus;
use crate::store::{self, ProjectHandle};
use crate::views;

/// Minimal Typst world: one in-memory source file and the bundled fonts.
struct ReportWorld {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
    source: Source,
}

impl ReportWorld {
    fn new(markup: String) -> Self {
        let fonts: Vec<Font> =
            typst_assets::fonts().flat_map(|data| Font::iter(Bytes::new(data))).collect();
        let book = FontBook::from_fonts(&fonts);
        let id = FileId::new(None, VirtualPath::new("report.typ"));
        Self {
            library: LazyHash::new(Library::default()),
            book: LazyHash::new(book),
            fonts,
            source: Source::new(id, markup),
        }
    }
}

impl World for ReportWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }
    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }
    fn main(&self) -> FileId {
        self.source.id()
    }
    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.source.id() {
            Ok(self.source.clone())
        } else {
            Err(FileError::NotFound(id.vpath().as_rootless_path().into()))
        }
    }
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        Err(FileError::NotFound(id.vpath().as_rootless_path().into()))
    }
    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }
    fn today(&self, _offset: Option<i64>) -> Option<Datetime> {
        None
    }
}

/// Lay out Typst markup into pages.
pub fn layout(markup: String) -> Result<PagedDocument> {
    let world = ReportWorld::new(markup);
    typst::compile(&world).output.map_err(|errs| {
        anyhow!(
            "PDF layout failed: {}",
            errs.iter().map(|e| e.message.to_string()).collect::<Vec<_>>().join("; ")
        )
    })
}

/// Typeset Typst markup to PDF bytes.
pub fn render(markup: String) -> Result<Vec<u8>> {
    let doc = layout(markup)?;
    typst_pdf::pdf(&doc, &typst_pdf::PdfOptions::default())
        .map_err(|errs| anyhow!("PDF export failed: {}", errs.iter().map(|e| e.message.to_string()).collect::<Vec<_>>().join("; ")))
}

/// A Typst string literal.
fn s(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A table cell holding plain text.
fn cell(text: &str) -> String {
    format!("[#{}]", s(text))
}

fn italic(text: &str) -> String {
    if text.is_empty() { "[]".into() } else { format!("[#emph({})]", s(text)) }
}

fn num(v: f64, digits: usize) -> String {
    format!("{v:.digits$}")
}

const PREAMBLE: &str = r##"
#set document(title: "COI Barcode BLAST report")
#set page(paper: "a4", margin: (x: 16mm, y: 18mm), numbering: "1 / 1", number-align: right)
#set text(font: "Libertinus Serif", size: 9pt)
#show raw: set text(font: "DejaVu Sans Mono", size: 6.9pt)
#show heading.where(level: 1): it => { set text(size: 15pt); block(above: 1.2em, below: 0.8em, it) }
#show heading.where(level: 2): it => { set text(size: 11.5pt); block(above: 1.2em, below: 0.6em, it) }
#let accent = rgb("#0d8a80")
#let soft = rgb("#e9eef3")
#let warn = rgb("#b45309")
#show raw.where(block: true): set par(leading: 0.48em)
// Tables with a header row; layout-only tables use plain `table` with stroke: none.
#let data-table(columns: auto, size: 8.5pt, ..cells) = {
  set text(size: size)
  table(
    columns: columns,
    stroke: (x, y) => if y == 0 { (bottom: 0.7pt) } else { (bottom: 0.3pt + soft) },
    inset: (x: 4pt, y: 2.6pt),
    ..cells,
  )
}
"##;

/// Bold header cells for a data table.
fn header_row(names: &[&str]) -> String {
    names.iter().map(|n| format!("[*{}*]", n)).collect::<Vec<_>>().join(", ") + ",\n"
}

pub struct Options {
    pub full: bool,
}

/// Build the report markup for the open project.
pub fn report_markup(h: &ProjectHandle, opts: &Options) -> Result<String> {
    let p = &h.project;
    let st = &p.settings;
    let used = p.blast.settings_used.clone().unwrap_or_else(|| st.blast.clone());
    let rids: Vec<String> =
        p.blast.groups.iter().filter(|g| g.status == GroupStatus::Ready).filter_map(|g| g.rid.clone()).collect();
    let searched = p.reads.iter().filter(|r| h.result_for(r).is_some()).count();
    let mut m = String::from(PREAMBLE);

    // Cover
    m.push_str(&format!(
        "#align(center)[#v(40mm)#text(size: 22pt, weight: \"bold\")[COI Barcode BLAST report]\\ #v(4mm)#text(size: 14pt)[#{}]\\ #v(2mm)#text(fill: luma(90))[#{}]]\n#v(18mm)\n",
        s(&p.name),
        s(&chrono::Local::now().format("%d %B %Y, %H:%M").to_string())
    ));
    let primer_mode = format!("{:?}", st.primer_mode).to_lowercase();
    let settings_rows = [
        ("Samples", format!("{} imported, {} searched", p.reads.len(), searched)),
        ("Source", p.source_zip.rsplit(['\\', '/']).next().unwrap_or_default().to_string()),
        ("High-quality base", format!("Phred >= {}", st.hq_quality)),
        ("HQ threshold", format!("kept region > {}% high-quality bases, at least {} bp", st.hq_threshold, st.min_length)),
        ("Primers", format!("{primer_mode}, up to {} mismatches", st.max_mismatches)),
        ("BLAST", format!("{}{} against {}, {} hits per sample", used.program, if used.megablast { " (megablast)" } else { "" }, used.database, used.max_hits)),
        ("Request IDs (RID)", if rids.is_empty() { "none".into() } else { rids.join(", ") }),
        ("Compile", format!("closest species = highest % identity among hits with query cover >= {}%", st.compile_min_cover)),
    ];
    m.push_str("#table(columns: (42mm, 1fr), stroke: none, inset: 3pt,\n");
    for (k, v) in settings_rows {
        m.push_str(&format!("[#text(weight: \"bold\")[#{}]], {},\n", s(k), cell(&v)));
    }
    m.push_str(")\n#pagebreak()\n");

    // Compiled species
    m.push_str("= Closest species per sample\n");
    match &p.compiled {
        None => m.push_str("_Compile results has not been run for this project._\n"),
        Some(c) => {
            let total: usize = c.chart.iter().map(|x| x.count).sum();
            let max = c.chart.iter().map(|x| x.count).max().unwrap_or(1).max(1);
            m.push_str(&format!(
                "{} samples compiled, {} species. Bars show how many samples have each species as their closest match.\n\n",
                total,
                c.chart.len()
            ));
            let shown = if opts.full || c.chart.len() <= 25 { c.chart.len() } else { 25 };
            m.push_str("#table(columns: (62mm, 1fr, 18mm), stroke: none, inset: (x: 3pt, y: 2pt),\n");
            for sc in c.chart.iter().take(shown) {
                m.push_str(&format!(
                    "{}, [#box(width: {:.1}%, height: 7pt, fill: accent, radius: 1pt)], {},\n",
                    italic(&sc.species),
                    sc.count as f64 * 100.0 / max as f64,
                    cell(&format!("{} ({:.0}%)", sc.count, sc.pct))
                ));
            }
            if shown < c.chart.len() {
                let rest: usize = c.chart[shown..].iter().map(|x| x.count).sum();
                m.push_str(&format!("{}, [], {},\n", cell(&format!("Other ({} species)", c.chart.len() - shown)), cell(&rest.to_string())));
            }
            m.push_str(")\n");
            m.push_str("== Per-sample result\n#data-table(columns: (auto, 1fr, auto, auto, auto, 1fr, auto),\n");
            m.push_str(&header_row(&["Sample", "Closest species", "% Ident.", "Cover", "Accession", "Runner-up", "%"]));
            for row in &c.rows {
                let species = match &row.species {
                    Some(sp) => italic(sp),
                    None => format!("[#text(fill: warn)[#{}]]", s(reason_text(row.reason.as_deref()))),
                };
                m.push_str(&format!(
                    "{}, {}, {}, {}, {}, {}, {},\n",
                    cell(&row.sample_id),
                    species,
                    cell(&row.identity.map(|v| num(v, 2)).unwrap_or_default()),
                    cell(&row.query_cover.map(|v| format!("{v:.0}%")).unwrap_or_default()),
                    cell(row.accession.as_deref().unwrap_or("")),
                    italic(row.runner_up.as_deref().unwrap_or("")),
                    cell(&row.runner_up_identity.map(|v| num(v, 2)).unwrap_or_default()),
                ));
            }
            m.push_str(")\n");
        }
    }

    // Batch overview
    m.push_str("#pagebreak()\n= Batch overview\n#data-table(columns: (auto, auto, auto, auto, 1fr, auto, auto, 1fr),\n");
    m.push_str(&header_row(&["Sample", "Reads", "Length", "% HQ", "Top hit species", "% Ident.", "Cover", "Status"]));
    for r in &p.reads {
        let top = h.result_for(r).and_then(|q| q.hits.first());
        m.push_str(&format!(
            "{}, {}, {}, {}, {}, {}, {}, {},\n",
            cell(&r.id),
            cell(&crate::export::reads_text(r)),
            cell(&r.derived.final_len.to_string()),
            cell(&if r.has_trace { num(r.derived.hq_pct, 1) } else { "-".into() }),
            italic(&top.map(species_of).unwrap_or_default()),
            cell(&top.map(|t| num(t.per_ident, 2)).unwrap_or_default()),
            cell(&top.map(|t| format!("{:.0}%", t.query_cover)).unwrap_or_default()),
            cell(&status_text(&views::sample_flags(h, r))),
        ));
    }
    m.push_str(")\n");

    // Per-sample sections
    if opts.full {
        let mut raw_cache: HashMap<String, String> = HashMap::new();
        for r in &p.reads {
            if r.paired_with.is_some() {
                continue;
            }
            m.push_str(&format!("#pagebreak()\n= {}\n", s(&r.id).trim_matches('"')));
            let d = &r.derived;
            let trim = d.trim.map(|t| format!("{}-{}", t.start + 1, t.end)).unwrap_or_default();
            let qc_rows = [
                ("File", match &r.pair {
                    Some(pair) => format!("{} + {} (assembled)", r.file_name, pair.reverse_id),
                    None => r.file_name.clone(),
                }),
                ("Kept region", if r.pair.is_some() { "consensus of both reads".into() } else { format!("bases {trim} of {}", r.raw_length) }),
                ("Length / % HQ", if r.has_trace {
                    format!("{} bp, {:.1}% high quality", d.final_len, d.hq_pct)
                } else {
                    format!("{} bp, from FASTA (no quality values)", d.final_len)
                }),
                ("Primer sites", primer_text(r)),
                ("Translation", if d.stop_codons == 0 { "no stop codons (table 5)".into() } else { format!("{} stop codon(s) in the best frame", d.stop_codons) }),
                ("Status", status_text(&views::sample_flags(h, r))),
            ];
            m.push_str("#table(columns: (34mm, 1fr), stroke: none, inset: 2.5pt,\n");
            for (k, v) in qc_rows {
                m.push_str(&format!("[#text(weight: \"bold\")[#{}]], {},\n", s(k), cell(&v)));
            }
            m.push_str(")\n");
            let Some(q) = h.result_for(r) else {
                m.push_str("_No BLAST results._\n");
                continue;
            };
            m.push_str("== Top hits\n#data-table(columns: (1fr, auto, auto, auto, auto, auto), size: 7.4pt,\n");
            m.push_str(&header_row(&["Description", "Max Score", "Cover", "E value", "% Ident.", "Accession"]));
            for hit in q.hits.iter().take(10) {
                // One line per hit: long GenBank titles are shortened.
                let title: String =
                    if hit.title.chars().count() > 92 { hit.title.chars().take(90).collect::<String>() + "…" } else { hit.title.clone() };
                m.push_str(&format!(
                    "{}, {}, {}, {}, {}, {},\n",
                    cell(&title),
                    cell(&format!("{}", hit.max_score.floor())),
                    cell(&format!("{:.0}%", hit.query_cover)),
                    cell(&if hit.evalue == 0.0 { "0.0".to_string() } else { format!("{:.0e}", hit.evalue) }),
                    cell(&num(hit.per_ident, 2)),
                    cell(&hit.accession),
                ));
            }
            m.push_str(")\n");
            if q.hits.is_empty() {
                continue;
            }
            let rref = r.result.as_ref().expect("result_for implies a result ref");
            let raw = match raw_cache.get(&rref.group_id) {
                Some(raw) => raw.clone(),
                None => {
                    let name = store::raw_entry(&rref.group_id);
                    let bytes = match h.pending.iter().find(|(n, _)| *n == name) {
                        Some((_, b)) => b.clone(),
                        None => store::read_entry(&h.path, &name)?,
                    };
                    let text = String::from_utf8(bytes)?;
                    raw_cache.insert(rref.group_id.clone(), text.clone());
                    text
                }
            };
            if let Ok(detail) = blast::hit_detail(&raw, rref.index, 1) {
                m.push_str(&alignment_markup(&detail));
            }
        }
    }

    // Appendix: FASTA
    m.push_str("#pagebreak()\n= Appendix: final sequences (FASTA)\n");
    let fasta = crate::fasta::multi(&p.reads, p.reads.iter().filter(|r| r.derived.eligible));
    m.push_str(&format!("#raw({}, block: true)\n", s(&fasta)));
    Ok(m)
}

fn alignment_markup(d: &HitDetail) -> String {
    let Some(hsp) = d.hsps.first() else { return String::new() };
    let mut text = String::new();
    let q_step: isize = if hsp.query_from <= hsp.query_to { 1 } else { -1 };
    let s_step: isize = if hsp.hit_strand == "Minus" || hsp.hit_from > hsp.hit_to { -1 } else { 1 };
    let (mut q, mut sb) = (hsp.query_from as isize, hsp.hit_from as isize);
    let chars = |x: &str| x.chars().collect::<Vec<_>>();
    let (qs, ms, hs) = (chars(&hsp.qseq), chars(&hsp.midline), chars(&hsp.hseq));
    for start in (0..qs.len()).step_by(60) {
        let end = (start + 60).min(qs.len());
        let qpart: String = qs[start..end].iter().collect();
        let hpart: String = hs[start.min(hs.len())..end.min(hs.len())].iter().collect();
        let mpart: String = ms[start.min(ms.len())..end.min(ms.len())].iter().collect();
        let qn = qpart.chars().filter(|c| *c != '-').count() as isize;
        let hn = hpart.chars().filter(|c| *c != '-').count() as isize;
        let q_end = q + q_step * (qn - 1);
        let s_end = sb + s_step * (hn - 1);
        text.push_str(&format!("Query  {q:<5} {qpart}  {q_end}\n             {mpart}\nSbjct  {sb:<5} {hpart}  {s_end}\n\n"));
        q = q_end + q_step;
        sb = s_end + s_step;
    }
    format!(
        "== Best hit alignment\n#text(weight: \"bold\")[#{}]\\ Sequence ID: {} · Length: {} · Score {} bits · Identities {}/{} ({}%) · Gaps {}/{} · Strand {}/{}\n#raw({}, block: true)\n",
        s(&d.title),
        s(&d.accession).trim_matches('"'),
        d.length,
        hsp.bit_score.floor(),
        hsp.identity,
        hsp.align_len,
        hsp.identity * 100 / hsp.align_len.max(1),
        hsp.gaps,
        hsp.align_len,
        hsp.query_strand,
        hsp.hit_strand,
        s(text.trim_end())
    )
}

pub fn write_report(h: &ProjectHandle, path: &std::path::Path, opts: &Options) -> Result<usize> {
    let bytes = render(report_markup(h, opts)?)?;
    std::fs::write(path, &bytes)?;
    Ok(bytes.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_escaped_text() {
        let markup = format!("{PREAMBLE}= Test\n#table(columns: 2, {}, {})\n", cell("a \"quoted\" \\ text #not-code"), italic("Aedes albopictus"));
        let pdf = render(markup).unwrap();
        assert!(pdf.starts_with(b"%PDF"));
    }
}
