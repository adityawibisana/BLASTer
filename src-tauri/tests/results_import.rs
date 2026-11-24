//! Importing BLAST results files: the formats the NCBI website offers give the same
//! hit tables, and imported results attach to the project's samples.
//!
//! `ncbi_2_queries.*` are one real NCBI search (RID GAXN5YNT014, two reads of
//! `samples/fasta/sangeranalyseR_reads.fa`, 3 hits each) fetched as JSON2_S, XML
//! and XML2_S. `ncbi_18_samples.json` is the example batch's real response.

use std::path::PathBuf;

use coi_blast_lib::model::*;
use coi_blast_lib::{blast, fasta, importer, results_file, store};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(root().join("tests/fixtures").join(name)).unwrap()
}

fn tables(bytes: &[u8]) -> Vec<blast::QueryResult> {
    blast::parse_results(&results_file::document(&results_file::reports(bytes).unwrap())).unwrap()
}

#[test]
fn every_ncbi_download_format_gives_the_same_hits() {
    let json = tables(&fixture("ncbi_2_queries.json"));
    let xml2 = tables(&fixture("ncbi_2_queries.xml2.xml"));
    let legacy = tables(&fixture("ncbi_2_queries.xml"));
    assert_eq!(json.len(), 2);
    assert_eq!(json[0].hits.len(), 3);
    assert_eq!(xml2, json);
    // Legacy XML has no species name or taxid; the species falls back to the title.
    let mut no_names = json.clone();
    for q in &mut no_names {
        for h in &mut q.hits {
            h.sciname.clear();
            h.taxid = None;
        }
    }
    assert_eq!(legacy, no_names);
    // Alignments read the same, too.
    let doc = |b: &[u8]| results_file::document(&results_file::reports(b).unwrap());
    assert_eq!(
        blast::hit_detail(&doc(&fixture("ncbi_2_queries.xml")), 1, 2).unwrap().hsps,
        blast::hit_detail(&doc(&fixture("ncbi_2_queries.json")), 1, 2).unwrap().hsps
    );
}

fn new_project() -> store::ProjectHandle {
    let settings = Settings::default();
    let imported = importer::import_zip(&root().join("../samples/COI_20_sequences.zip"), &settings).unwrap();
    let project = Project {
        format: PROJECT_FORMAT,
        name: "COI_20_sequences".into(),
        created_at: chrono::Local::now().to_rfc3339(),
        source_zip: "COI_20_sequences.zip".into(),
        settings,
        reads: imported.reads,
        skipped: imported.skipped,
        blast: BlastState::default(),
        compiled: None,
        pair_suggestions: Vec::new(),
        rejected_pairs: Vec::new(),
    };
    store::ProjectHandle { path: PathBuf::from("unused.coiproj"), project, results: Default::default(), pending: Vec::new() }
}

fn group(h: &store::ProjectHandle, id: &str, ids: &[&str], status: GroupStatus) -> BlastGroup {
    let reads: Vec<&Read> = ids.iter().map(|s| h.project.reads.iter().find(|r| r.id == *s).unwrap()).collect();
    BlastGroup {
        id: id.into(),
        sample_ids: reads.iter().map(|r| r.id.clone()).collect(),
        fingerprints: reads.iter().map(|r| r.derived.fingerprint.clone()).collect(),
        fasta: fasta::multi(&h.project.reads, reads.iter().copied()),
        status,
        rid: None,
        rtoe_secs: None,
        submitted_at: None,
        finished_at: None,
        attempts: 0,
        error: None,
        last_checked_at: None,
        last_check: None,
        source: GroupSource::Ncbi,
    }
}

#[test]
fn imported_results_attach_to_the_samples() {
    let mut h = new_project();
    let eligible: Vec<String> = h.project.reads.iter().filter(|r| r.derived.eligible).map(|r| r.id.clone()).collect();
    let e: Vec<&str> = eligible.iter().map(String::as_str).collect();
    // g0001: fully in the file, still waiting at NCBI. g0002: queued, partly in the
    // file after one sample is removed below. g0003: already done.
    h.project.blast.groups.push(group(&h, "g0001", &e[0..5], GroupStatus::Submitted));
    h.project.blast.groups.push(group(&h, "g0002", &e[5..10], GroupStatus::Pending));

    // The file holds all 18 samples except the last one.
    let mut reports = results_file::reports(&fixture("ncbi_18_samples.json")).unwrap();
    let last = eligible.last().unwrap().clone();
    reports.retain(|r| !r["results"]["search"]["query_title"].as_str().unwrap().starts_with(&format!("{last} ")));
    assert_eq!(reports.len(), 17);
    let sum = results_file::import_into(&mut h, reports.clone()).unwrap();
    assert_eq!(sum.samples, 17);
    assert!(sum.unknown.is_empty() && sum.changed.is_empty() && sum.already.is_empty());
    assert_eq!(sum.still_missing, 1);

    // g0001 and g0002 were fully covered and took their samples; the rest went to g0003.
    assert_eq!(sum.groups, ["g0001", "g0002", "g0003"]);
    for g in &h.project.blast.groups {
        assert_eq!((g.status.clone(), g.source), (GroupStatus::Ready, GroupSource::File), "{}", g.id);
    }
    for r in h.project.reads.iter().filter(|r| r.derived.eligible && r.id != last) {
        let q = h.result_for(r).unwrap_or_else(|| panic!("{} has no result", r.id));
        assert!(q.query_title.starts_with(&format!("{} ", r.id)));
        assert!(!h.outdated(r));
    }
    // Hit details read from the stored raw entry.
    let r = h.project.reads.iter().find(|r| r.id == e[6]).unwrap();
    let rr = r.result.as_ref().unwrap();
    let raw = h.pending.iter().find(|(n, _)| n == &store::raw_entry(&rr.group_id)).unwrap();
    let d = blast::hit_detail(std::str::from_utf8(&raw.1).unwrap(), rr.index, 1).unwrap();
    assert_eq!(d.accession, h.result_for(r).unwrap().hits[0].accession);

    // Importing the same file again changes nothing.
    let again = results_file::import_into(&mut h, reports).unwrap();
    assert_eq!((again.samples, again.already.len()), (0, 17));
}

#[test]
fn imported_samples_leave_queued_groups() {
    let mut h = new_project();
    let eligible: Vec<String> = h.project.reads.iter().filter(|r| r.derived.eligible).map(|r| r.id.clone()).collect();
    let e: Vec<&str> = eligible.iter().map(String::as_str).collect();
    h.project.blast.groups.push(group(&h, "g0001", &e[0..10], GroupStatus::Pending));
    h.project.blast.groups.push(group(&h, "g0002", &e[10..], GroupStatus::Failed));

    let reports: Vec<_> = results_file::reports(&fixture("ncbi_18_samples.json"))
        .unwrap()
        .into_iter()
        .filter(|r| e[7..12].iter().any(|id| r["results"]["search"]["query_title"].as_str().unwrap().starts_with(&format!("{id} "))))
        .collect();
    let sum = results_file::import_into(&mut h, reports).unwrap();
    assert_eq!((sum.samples, sum.groups.as_slice()), (5, ["g0003".to_string()].as_slice()));
    let g1 = &h.project.blast.groups[0];
    assert_eq!(g1.sample_ids, e[0..7]);
    assert_eq!(g1.fingerprints.len(), 7);
    assert_eq!(g1.fasta.matches('>').count(), 7);
    assert_eq!(h.project.blast.groups[1].sample_ids, e[12..]);
    assert_eq!(h.project.blast.groups[1].status, GroupStatus::Failed);
}

#[test]
fn queries_that_do_not_match_are_reported() {
    let mut h = new_project();
    let sum = results_file::import_into(&mut h, results_file::reports(&fixture("ncbi_2_queries.json")).unwrap()).unwrap();
    assert_eq!(sum.samples, 0);
    assert_eq!(sum.unknown, ["Achl_ACHLO006-09_1_F", "Achl_ACHLO007-09_1_F"]);
    assert!(h.project.blast.groups.is_empty());
}
