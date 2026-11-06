//! Exports: Excel workbook, FASTA, raw NCBI responses.

use std::io::Write as _;
use std::path::Path;

use anyhow::Result;
use rust_xlsxwriter::{Chart, ChartType, Color, Format, FormatBorder, Workbook, Worksheet};

use crate::model::{Flag, GroupStatus};
use crate::primers::Dir;
use crate::store::{self, ProjectHandle};
use crate::views;

pub fn flag_label(f: Flag) -> &'static str {
    match f {
        Flag::FailedQc => "Failed QC",
        Flag::PrimerMissing => "Primer not found where expected",
        Flag::Length => "Unexpected length",
        Flag::StopCodon => "Stop codon",
        Flag::Ambiguous => "Too many ambiguous bases",
        Flag::DirectionUnknown => "Direction unknown",
        Flag::Edited => "Edited",
        Flag::LowIdentity => "Low identity",
        Flag::MixedSpecies => "Mixed species",
        Flag::NoHits => "No hits",
        Flag::Outdated => "Outdated",
        Flag::NotSearched => "Not searched",
        Flag::FrConflict => "F/R conflict",
        Flag::NoOverlap => "F/R do not overlap",
    }
}

pub fn status_text(flags: &[Flag]) -> String {
    if flags.is_empty() { "OK".into() } else { flags.iter().map(|f| flag_label(*f)).collect::<Vec<_>>().join("; ") }
}

fn dir_text(d: Option<Dir>) -> &'static str {
    match d {
        Some(Dir::F) => "F",
        Some(Dir::R) => "R",
        None => "unknown",
    }
}

struct Formats {
    header: Format,
    warn: Format,
    pct: Format,
    num: Format,
    sci: Format,
}

fn formats() -> Formats {
    Formats {
        header: Format::new().set_bold().set_background_color(Color::RGB(0xDCE6F1)).set_border_bottom(FormatBorder::Thin),
        warn: Format::new().set_background_color(Color::RGB(0xFFF2CC)),
        pct: Format::new().set_num_format("0.00"),
        num: Format::new().set_num_format("0"),
        sci: Format::new().set_num_format("0.00E+00"),
    }
}

fn header(ws: &mut Worksheet, f: &Formats, cols: &[(&str, f64)]) -> Result<()> {
    for (i, (name, width)) in cols.iter().enumerate() {
        ws.write_string_with_format(0, i as u16, *name, &f.header)?;
        ws.set_column_width(i as u16, *width)?;
    }
    ws.set_freeze_panes(1, 0)?;
    Ok(())
}

fn opt_num(ws: &mut Worksheet, row: u32, col: u16, v: Option<f64>, fmt: &Format) -> Result<()> {
    if let Some(v) = v {
        ws.write_number_with_format(row, col, v, fmt)?;
    }
    Ok(())
}

pub fn excel(h: &ProjectHandle, path: &Path) -> Result<()> {
    let p = &h.project;
    let f = formats();
    let mut wb = Workbook::new();

    // Overview
    {
        let ws = wb.add_worksheet().set_name("Overview")?;
        header(
            ws,
            &f,
            &[
                ("Sample", 14.0),
                ("File", 16.0),
                ("Reads", 8.0),
                ("Primers", 26.0),
                ("Final length", 12.0),
                ("% HQ", 8.0),
                ("Stop codons", 11.0),
                ("Top hit species", 30.0),
                ("Per. Ident", 10.0),
                ("Query Cover", 11.0),
                ("Accession", 14.0),
                ("Status", 50.0),
            ],
        )?;
        for (i, r) in p.reads.iter().enumerate() {
            let row = i as u32 + 1;
            let flags = views::sample_flags(h, r);
            let top = h.result_for(r).and_then(|q| q.hits.first());
            ws.write_string(row, 0, &r.id)?;
            ws.write_string(row, 1, &r.file_name)?;
            ws.write_string(row, 2, reads_text(r))?;
            ws.write_string(row, 3, primer_text(r))?;
            ws.write_number(row, 4, r.derived.final_len as f64)?;
            if r.has_trace {
                ws.write_number_with_format(row, 5, r.derived.hq_pct, &f.pct)?;
            }
            ws.write_number(row, 6, r.derived.stop_codons as f64)?;
            if let Some(t) = top {
                ws.write_string(row, 7, crate::compile::species_of(t))?;
                ws.write_number_with_format(row, 8, t.per_ident, &f.pct)?;
                ws.write_number_with_format(row, 9, t.query_cover, &f.num)?;
                ws.write_string(row, 10, &t.accession)?;
            }
            let status = status_text(&flags);
            if flags.is_empty() {
                ws.write_string(row, 11, status)?;
            } else {
                ws.write_string_with_format(row, 11, status, &f.warn)?;
            }
        }
        ws.autofilter(0, 0, p.reads.len() as u32, 11)?;
    }

    // Compiled + chart
    if let Some(c) = &p.compiled {
        let ws = wb.add_worksheet().set_name("Compiled")?;
        header(
            ws,
            &f,
            &[
                ("Sample", 14.0),
                ("Closest species", 32.0),
                ("% Identity", 10.0),
                ("Query Cover", 11.0),
                ("Accession", 14.0),
                ("Runner-up species", 30.0),
                ("Runner-up %", 11.0),
                ("Hits counted", 11.0),
                ("Status", 40.0),
            ],
        )?;
        for (i, row_data) in c.rows.iter().enumerate() {
            let row = i as u32 + 1;
            ws.write_string(row, 0, &row_data.sample_id)?;
            match &row_data.species {
                Some(s) => ws.write_string(row, 1, s)?,
                None => ws.write_string_with_format(row, 1, reason_text(row_data.reason.as_deref()), &f.warn)?,
            };
            opt_num(ws, row, 2, row_data.identity, &f.pct)?;
            opt_num(ws, row, 3, row_data.query_cover, &f.num)?;
            if let Some(a) = &row_data.accession {
                ws.write_string(row, 4, a)?;
            }
            if let Some(s) = &row_data.runner_up {
                ws.write_string(row, 5, s)?;
            }
            opt_num(ws, row, 6, row_data.runner_up_identity, &f.pct)?;
            ws.write_number(row, 7, row_data.hits_counted as f64)?;
            ws.write_string(row, 8, status_text(&row_data.flags))?;
        }
        ws.autofilter(0, 0, c.rows.len().max(1) as u32, 8)?;

        // Species counts next to the table, and a bar chart built from them.
        let (col_s, col_n, col_p) = (10u16, 11u16, 12u16);
        ws.write_string_with_format(0, col_s, "Species", &f.header)?;
        ws.write_string_with_format(0, col_n, "Samples", &f.header)?;
        ws.write_string_with_format(0, col_p, "Share %", &f.header)?;
        ws.set_column_width(col_s, 32.0)?;
        for (i, sc) in c.chart.iter().enumerate() {
            let row = i as u32 + 1;
            ws.write_string(row, col_s, &sc.species)?;
            ws.write_number(row, col_n, sc.count as f64)?;
            ws.write_number_with_format(row, col_p, sc.pct, &f.pct)?;
        }
        if !c.chart.is_empty() {
            let last = c.chart.len() as u32;
            let mut chart = Chart::new(ChartType::Bar);
            chart
                .add_series()
                .set_categories(("Compiled", 1, col_s, last, col_s))
                .set_values(("Compiled", 1, col_n, last, col_n))
                .set_name("Samples");
            chart.title().set_name("Closest species per sample");
            chart.legend().set_hidden();
            // Largest bar at the top, as in the app.
            chart.y_axis().set_reverse();
            chart.x_axis().set_name("Samples");
            chart.set_width(640).set_height((120 + 26 * c.chart.len() as u32).min(900));
            ws.insert_chart(last + 3, col_s, &chart)?;
        }
    }

    // QC
    {
        let ws = wb.add_worksheet().set_name("QC")?;
        header(
            ws,
            &f,
            &[
                ("Sample", 14.0),
                ("File", 16.0),
                ("Raw length", 10.0),
                ("% HQ raw", 9.0),
                ("% GC raw", 9.0),
                ("Trim start", 10.0),
                ("Trim end", 10.0),
                ("Final length", 11.0),
                ("% HQ final", 10.0),
                ("Direction", 9.0),
                ("Primer set", 24.0),
                ("Start primer", 22.0),
                ("End primer", 22.0),
                ("Stop codons", 11.0),
                ("Ambiguous bases", 14.0),
                ("Manually edited", 14.0),
                ("Included in BLAST", 15.0),
                ("Forward/reverse pair", 60.0),
            ],
        )?;
        for (i, r) in p.reads.iter().enumerate() {
            let row = i as u32 + 1;
            let d = &r.derived;
            ws.write_string(row, 0, &r.id)?;
            ws.write_string(row, 1, &r.file_name)?;
            ws.write_number(row, 2, r.raw_length as f64)?;
            if r.has_trace {
                ws.write_number_with_format(row, 3, r.raw_hq_pct, &f.pct)?;
            }
            ws.write_number_with_format(row, 4, r.raw_gc_pct, &f.pct)?;
            if let Some(t) = d.trim {
                ws.write_number(row, 5, (t.start + 1) as f64)?;
                ws.write_number(row, 6, t.end as f64)?;
            }
            ws.write_number(row, 7, d.final_len as f64)?;
            if r.has_trace {
                ws.write_number_with_format(row, 8, d.hq_pct, &f.pct)?;
            }
            ws.write_string(row, 9, dir_text(d.direction))?;
            ws.write_string(row, 10, r.primer.set_name.as_deref().unwrap_or("none found"))?;
            ws.write_string(row, 11, site_text(&r.primer.start_site))?;
            ws.write_string(row, 12, site_text(&r.primer.end_site))?;
            ws.write_number(row, 13, d.stop_codons as f64)?;
            ws.write_number(row, 14, d.ambiguous as f64)?;
            ws.write_string(row, 15, if r.manual_trim.is_some() || r.direction_manual.is_some() { "yes" } else { "no" })?;
            ws.write_string(row, 16, if d.eligible { "yes" } else { "no" })?;
            ws.write_string(row, 17, pair_text(r))?;
        }
        ws.autofilter(0, 0, p.reads.len() as u32, 17)?;
    }

    // Hits
    {
        let ws = wb.add_worksheet().set_name("Hits")?;
        header(
            ws,
            &f,
            &[
                ("Sample", 14.0),
                ("Rank", 6.0),
                ("Description", 70.0),
                ("Scientific Name", 30.0),
                ("Max Score", 10.0),
                ("Total Score", 10.0),
                ("Query Cover", 11.0),
                ("E value", 10.0),
                ("Per. Ident", 10.0),
                ("Acc. Len", 9.0),
                ("Accession", 14.0),
            ],
        )?;
        let mut row = 1u32;
        for r in &p.reads {
            let Some(q) = h.result_for(r) else { continue };
            for hit in &q.hits {
                ws.write_string(row, 0, &r.id)?;
                ws.write_number(row, 1, hit.rank as f64)?;
                ws.write_string(row, 2, &hit.title)?;
                ws.write_string(row, 3, &hit.sciname)?;
                ws.write_number_with_format(row, 4, hit.max_score, &f.num)?;
                ws.write_number_with_format(row, 5, hit.total_score, &f.num)?;
                ws.write_number_with_format(row, 6, hit.query_cover, &f.num)?;
                ws.write_number_with_format(row, 7, hit.evalue, &f.sci)?;
                ws.write_number_with_format(row, 8, hit.per_ident, &f.pct)?;
                ws.write_number(row, 9, hit.acc_len as f64)?;
                ws.write_string(row, 10, &hit.accession)?;
                row += 1;
            }
        }
        ws.autofilter(0, 0, row.saturating_sub(1).max(1), 10)?;
    }

    // Sequences
    {
        let ws = wb.add_worksheet().set_name("Sequences")?;
        header(ws, &f, &[("Sample", 14.0), ("Length", 8.0), ("FASTA header", 60.0), ("Sequence", 120.0)])?;
        for (i, r) in p.reads.iter().enumerate() {
            let row = i as u32 + 1;
            ws.write_string(row, 0, &r.id)?;
            ws.write_number(row, 1, r.derived.final_len as f64)?;
            ws.write_string(row, 2, crate::fasta::header(r, crate::fasta::partner(&p.reads, r)))?;
            ws.write_string(row, 3, &r.derived.final_seq)?;
        }
    }

    // Run info
    {
        let ws = wb.add_worksheet().set_name("Run info")?;
        ws.set_column_width(0, 28.0)?;
        ws.set_column_width(1, 80.0)?;
        let s = &p.settings;
        let used = p.blast.settings_used.clone().unwrap_or_else(|| s.blast.clone());
        let rids: Vec<String> = p
            .blast
            .groups
            .iter()
            .filter(|g| g.status == GroupStatus::Ready)
            .filter_map(|g| g.rid.clone())
            .collect();
        let rows: Vec<(String, String)> = vec![
            ("Project".into(), p.name.clone()),
            ("Source zip".into(), p.source_zip.clone()),
            ("Exported".into(), chrono::Local::now().format("%Y-%m-%d %H:%M").to_string()),
            ("App version".into(), env!("CARGO_PKG_VERSION").into()),
            ("BLAST program".into(), format!("{}{}", used.program, if used.megablast { " (megablast)" } else { "" })),
            ("BLAST database".into(), used.database.clone()),
            ("Max hits per query".into(), used.max_hits.to_string()),
            ("Organism filter".into(), if used.organism.is_empty() { "none".into() } else { used.organism.clone() }),
            ("Exclude models (XM/XP)".into(), yes_no(used.exclude_models)),
            ("Exclude uncultured/environmental".into(), yes_no(used.exclude_uncultured)),
            ("RIDs".into(), rids.join(", ")),
            ("High-quality base".into(), format!("Phred >= {}", s.hq_quality)),
            ("HQ threshold".into(), format!("> {}%", s.hq_threshold)),
            ("Minimum length".into(), format!("{} bp", s.min_length)),
            ("Mott error limit".into(), s.mott_cutoff.to_string()),
            ("Primer option".into(), format!("{:?}", s.primer_mode)),
            ("Allowed primer mismatches".into(), s.max_mismatches.to_string()),
            ("Expected length (single read)".into(), format!("{}-{} bp", s.single_len_min, s.single_len_max)),
            ("Compile: minimum query cover".into(), format!("{}%", s.compile_min_cover)),
        ];
        for (i, (k, v)) in rows.iter().enumerate() {
            ws.write_string_with_format(i as u32, 0, k, &f.header)?;
            ws.write_string(i as u32, 1, v)?;
        }
    }

    wb.save(path)?;
    Ok(())
}

/// "single", "F + R", or "R of <forward ID>" for the reverse read of a pair.
pub fn reads_text(r: &crate::model::Read) -> String {
    match (&r.pair, &r.paired_with) {
        (Some(p), _) if p.no_overlap => match p.use_only {
            Some(Dir::F) => "F only (no overlap)".into(),
            Some(Dir::R) => "R only (no overlap)".into(),
            None => "F + R, no overlap".into(),
        },
        (Some(_), _) => "F + R".into(),
        (None, Some(f)) => format!("R of {f}"),
        _ => "single".into(),
    }
}

fn pair_text(r: &crate::model::Read) -> String {
    match &r.pair {
        Some(p) if p.no_overlap => format!("with {}: no overlap", p.reverse_id),
        Some(p) => format!(
            "with {}: overlap {} bp, {:.1}% identity, {} disagreements, {} conflicts",
            p.reverse_id,
            p.overlap,
            p.identity,
            p.disagreements,
            p.conflicts.len()
        ),
        None => r.paired_with.as_ref().map(|f| format!("reverse read of {f}")).unwrap_or_default(),
    }
}

fn yes_no(b: bool) -> String {
    if b { "yes".into() } else { "no".into() }
}

fn site_text(s: &Option<crate::primers::SiteHit>) -> String {
    match s {
        None => "not found".into(),
        Some(h) if h.partial => format!("{} partial ({} bp, {} mm) at {}", h.primer, h.matched, h.mismatches, h.start + 1),
        Some(h) => format!("{} ({} mm) at {}", h.primer, h.mismatches, h.start + 1),
    }
}

pub fn primer_text(r: &crate::model::Read) -> String {
    let name = |s: &Option<crate::primers::SiteHit>| s.as_ref().map_or("none".to_string(), |h| h.primer.clone());
    format!("{} / {}", name(&r.primer.start_site), name(&r.primer.end_site))
}

pub fn reason_text(reason: Option<&str>) -> &'static str {
    match reason {
        Some("noHits") => "No hit with enough query cover",
        Some("failedQc") => "Failed QC (not searched)",
        Some("excluded") => "Excluded by user",
        Some("notSearched") => "Not searched yet",
        _ => "",
    }
}

pub fn fasta(h: &ProjectHandle, path: &Path, only_eligible: bool) -> Result<usize> {
    let reads: Vec<_> = h.project.reads.iter().filter(|r| !only_eligible || r.derived.eligible).collect();
    std::fs::write(path, crate::fasta::multi(&h.project.reads, reads.iter().copied()))?;
    Ok(reads.len())
}

/// Zip of every raw NCBI response in the project.
pub fn raw_results(h: &ProjectHandle, path: &Path) -> Result<usize> {
    let mut writer = zip::ZipWriter::new(std::fs::File::create(path)?);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut n = 0;
    for g in h.project.blast.groups.iter().filter(|g| g.status == GroupStatus::Ready) {
        let name = store::raw_entry(&g.id);
        let bytes = match h.pending.iter().find(|(n, _)| *n == name) {
            Some((_, b)) => b.clone(),
            None => store::read_entry(&h.path, &name)?,
        };
        writer.start_file(format!("{}_{}.json", g.id, g.rid.as_deref().unwrap_or("rid")), opts)?;
        writer.write_all(&bytes)?;
        n += 1;
    }
    writer.finish()?;
    Ok(n)
}
