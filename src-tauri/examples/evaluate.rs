//! Runs a batch through the whole pipeline the way the app does: import, pair
//! suggestions (all confirmed), QC, optionally a real NCBI BLAST and compile. Then it
//! prints one row per sample and checks it against what the file names say.
//!
//! Usage:
//!   cargo run --example evaluate -- <path>... [--blast <cache.json> [--no-submit]] [--expect <prefix>=<species>]...
//!
//! `<path>` is anything the app imports: a folder, a zip, .ab1 or FASTA files.
//! `--blast` searches NCBI and saves the raw responses to `<cache.json>`, keyed by the
//! FASTA submitted; a group already in that file is not searched again. With
//! `--no-submit`, a group missing from the cache is an error instead of a search.
//! NCBI asks for a contact email; this tool sends none (`Settings::default()`), so
//! NCBI may run its searches with low priority.
//! `--expect Achl=Allolobophora chlorotica` says which species samples whose ID
//! starts with `Achl` should be.

use std::collections::HashMap;
use std::path::PathBuf;

use coi_blast_lib::blast::{self, Client, SearchStatus};
use coi_blast_lib::model::*;
use coi_blast_lib::primers::Dir;
use coi_blast_lib::store::ProjectHandle;
use coi_blast_lib::{compile, fasta, importer, pairs, views};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut paths = Vec::new();
    let mut cache: Option<PathBuf> = None;
    let mut no_submit = false;
    let mut expect: Vec<(String, String)> = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--blast" => cache = args.next().map(PathBuf::from),
            "--no-submit" => no_submit = true,
            "--expect" => {
                let e = args.next().unwrap_or_default();
                let (k, v) = e.split_once('=').expect("--expect prefix=species");
                expect.push((k.to_string(), v.to_string()));
            }
            _ => paths.push(PathBuf::from(a)),
        }
    }
    let settings = Settings::default();
    let imported = importer::import_paths(&paths, &settings)?;
    let mut project = Project {
        format: PROJECT_FORMAT,
        name: importer::project_name(&paths),
        created_at: String::new(),
        source_zip: importer::describe(&paths),
        settings,
        reads: imported.reads,
        skipped: imported.skipped,
        blast: BlastState::default(),
        compiled: None,
        pair_suggestions: Vec::new(),
        rejected_pairs: Vec::new(),
    };
    println!("{}: {} reads, {} skipped", project.source_zip, project.reads.len(), project.skipped.len());
    for s in &project.skipped {
        println!("  skipped {}: {}", s.file_name, s.reason);
    }

    // Direction from the sequence against the direction the file name claims.
    println!("\n== Reads");
    println!("{:<24} {:>4} {:>6} {:<22} {:>3} {:>9} {:>4} {:>5}  flags", "read", "raw", "rawHQ", "primers", "dir", "trim", "len", "HQ%");
    let mut dir_wrong = 0;
    for r in &project.reads {
        let named = pairs::name_marker(&r.id, "").map(|(_, d)| d);
        let d = r.derived.direction;
        if named.is_some() && d.is_some() && named != d {
            dir_wrong += 1;
        }
        let sites = format!(
            "{}{}{}",
            r.primer.set_name.as_deref().map(short_set).unwrap_or("-"),
            if r.primer.start_site.is_some() { " start" } else { "" },
            if r.primer.end_site.is_some() { " end" } else { "" }
        );
        let trim = r.derived.trim.map(|t| format!("{}-{}", t.start, t.end)).unwrap_or_else(|| "-".into());
        println!(
            "{:<24} {:>4} {:>6.1} {:<22} {:>3} {:>9} {:>4} {:>5.1}  {:?}{}",
            r.id,
            r.raw_length,
            r.raw_hq_pct,
            sites,
            dir_str(d) + if named.is_some() && d.is_some() && named != d { "!" } else { "" },
            trim,
            r.derived.final_len,
            r.derived.hq_pct,
            r.derived.flags,
            if r.derived.stop_codons > 0 { format!(" stops={}", r.derived.stop_codons) } else { String::new() },
        );
    }

    // Pairs, as suggested and then confirmed.
    pairs::suggest(&mut project);
    let suggestions = project.pair_suggestions.clone();
    println!("\n== Pair suggestions: {}", suggestions.len());
    for s in &suggestions {
        pairs::confirm(&mut project, &s.forward_id, &s.reverse_id)?;
        let f = project.reads.iter().find(|r| r.id == s.forward_id).unwrap();
        let p = f.pair.as_ref().unwrap();
        println!(
            "{:<24} + {:<24} overlap {:>3}  identity {:>5.1}%  disagree {:>2}  conflicts {:>2}  consensus {:>3} bp  stops {}  {:?}",
            s.forward_id, s.reverse_id, p.overlap, p.identity, p.disagreements, p.conflicts.len(), f.derived.final_len,
            f.derived.stop_codons, f.derived.flags
        );
    }

    let mut h = ProjectHandle { path: PathBuf::new(), project, results: HashMap::new(), pending: Vec::new() };
    if let Some(cache) = cache {
        blast_all(&mut h, &cache, no_submit).await?;
    }
    let compiled = compile::compile(
        &h.project.reads,
        |r| views::result_for_compile(&h, r),
        |r| views::sample_flags(&h, r),
        h.project.settings.compile_min_cover,
    );

    println!("\n== Samples");
    let mut species_wrong = 0;
    let mut eligible = 0;
    for row in &compiled.rows {
        let read = h.project.reads.iter().find(|r| r.id == row.sample_id).unwrap();
        if read.paired_with.is_some() {
            continue;
        }
        eligible += read.derived.eligible as usize;
        let want = expect.iter().find(|(k, _)| row.sample_id.starts_with(k.as_str())).map(|(_, v)| v.as_str());
        let verdict = match (want, &row.species) {
            (Some(w), Some(s)) if s == w => "ok",
            (Some(_), Some(_)) => {
                species_wrong += 1;
                "WRONG"
            }
            _ => "",
        };
        println!(
            "{:<24} {:<4} {:>3} bp  {:<28} {:>6} {:>5}  {:<12} runner-up {:<26} {:?} {}",
            row.sample_id,
            if read.pair.is_some() { "pair" } else { "read" },
            read.derived.final_len,
            row.species.as_deref().or(row.reason.as_deref()).unwrap_or("-"),
            row.identity.map(|v| format!("{v:.2}")).unwrap_or_default(),
            row.query_cover.map(|v| format!("{v:.0}")).unwrap_or_default(),
            row.accession.as_deref().unwrap_or(""),
            match (&row.runner_up, row.runner_up_identity) {
                (Some(s), Some(i)) => format!("{s} {i:.2}"),
                _ => String::new(),
            },
            row.flags,
            verdict
        );
    }
    println!("\n== Chart");
    for c in &compiled.chart {
        println!("{:>3} {:>5.1}%  {}", c.count, c.pct, c.species);
    }
    println!(
        "\nSummary: {} samples ({} pairs), {} eligible, {} direction mismatches against the names, {} wrong species",
        compiled.rows.len() - suggestions.len(),
        suggestions.len(),
        eligible,
        dir_wrong,
        species_wrong
    );
    Ok(())
}

fn short_set(name: &str) -> &str {
    name.split_whitespace().next().unwrap_or(name)
}

fn dir_str(d: Option<Dir>) -> String {
    match d {
        Some(Dir::F) => "F".into(),
        Some(Dir::R) => "R".into(),
        None => "?".into(),
    }
}

/// Search every eligible sample the way the app's runner does (groups of
/// `group_size`), or read the saved responses back.
async fn blast_all(h: &mut ProjectHandle, cache: &PathBuf, no_submit: bool) -> anyhow::Result<()> {
    let s = h.project.settings.clone();
    let selected: Vec<Read> = h.project.reads.iter().filter(|r| r.derived.eligible).cloned().collect();
    let groups: Vec<BlastGroup> = selected
        .chunks(s.blast.group_size.clamp(1, 100))
        .enumerate()
        .map(|(i, chunk)| BlastGroup {
            id: format!("g{:04}", i + 1),
            sample_ids: chunk.iter().map(|r| r.id.clone()).collect(),
            fingerprints: chunk.iter().map(|r| r.derived.fingerprint.clone()).collect(),
            fasta: fasta::multi(&h.project.reads, chunk.iter()),
            status: GroupStatus::Pending,
            rid: None,
            rtoe_secs: None,
            submitted_at: None,
            finished_at: None,
            attempts: 0,
            error: None,
            last_checked_at: None,
            last_check: None,
        })
        .collect();
    h.project.blast.groups = groups.clone();
    h.project.blast.settings_used = Some(s.blast.clone());

    // Saved responses, keyed by the FASTA that was submitted.
    let mut saved: HashMap<String, String> =
        std::fs::read_to_string(cache).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let mut client = None;
    for g in &groups {
        if saved.contains_key(&g.fasta) {
            continue;
        }
        if no_submit {
            anyhow::bail!("{} is not in {cache:?} and --no-submit was given", g.id);
        }
        let client = match &client {
            Some(c) => c,
            None => client.insert(Client::new()?),
        };
        println!("submitting {} ({} sequences)", g.id, g.sample_ids.len());
        let (rid, rtoe) = client.put(&g.fasta, &s.blast, &s.email).await?;
        println!("  RID {rid}, estimated {rtoe}s");
        tokio::time::sleep(blast::POLL_INTERVAL.max(std::time::Duration::from_secs(rtoe))).await;
        loop {
            match client.status(&rid).await? {
                SearchStatus::Waiting => {
                    println!("  waiting");
                    tokio::time::sleep(blast::POLL_INTERVAL).await;
                }
                SearchStatus::Ready { .. } => break,
                other => anyhow::bail!("search ended: {other:?}"),
            }
        }
        saved.insert(g.fasta.clone(), client.fetch_json(&rid).await?);
        std::fs::write(cache, serde_json::to_string(&saved)?)?;
        tokio::time::sleep(blast::SUBMIT_INTERVAL).await;
    }
    for g in &groups {
        let json = &saved[&g.fasta];
        coi_blast_lib::store::attach_results(h, &g.id, blast::parse_results(json)?, json.clone());
    }
    Ok(())
}
