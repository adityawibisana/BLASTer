//! BLAST+ on this computer, end to end. Needs BLAST+: set `COI_BLASTN` to the
//! `blastn` program (makeblastdb must sit next to it); skipped otherwise.
//!
//! The example batch's eligible samples are made into a local database (in a folder
//! with a space in its name) and searched against it, so every sample finds itself.

use std::path::PathBuf;
use std::process::Command;

use coi_blast_lib::model::*;
use coi_blast_lib::{blast, fasta, importer, local_blast, store};

#[test]
fn searches_a_local_database() {
    let Ok(blastn) = std::env::var("COI_BLASTN") else {
        eprintln!("COI_BLASTN not set; skipping the BLAST+ test");
        return;
    };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let settings = Settings::default();
    let imported = importer::import_zip(&root.join("../samples/COI_20_sequences.zip"), &settings).unwrap();
    let reads = imported.reads;
    let eligible: Vec<&Read> = reads.iter().filter(|r| r.derived.eligible).collect();
    let query = fasta::multi(&reads, eligible.iter().copied());

    let base = std::env::temp_dir().join(format!("coi-local-blast-{}", std::process::id()));
    let dir = base.join("my refs");
    let build = base.join("build");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::create_dir_all(&build).unwrap();
    std::fs::write(build.join("refs.fasta"), &query).unwrap();
    // makeblastdb itself cannot write into a folder with a space; blastn can read from one.
    let makeblastdb = PathBuf::from(&blastn).with_file_name(if cfg!(windows) { "makeblastdb.exe" } else { "makeblastdb" });
    let out = Command::new(makeblastdb)
        .args(["-in", "refs.fasta", "-dbtype", "nucl", "-out", "coi_refs"])
        .current_dir(&build)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    for f in std::fs::read_dir(&build).unwrap().flatten() {
        if f.file_name().to_string_lossy().starts_with("coi_refs.") {
            std::fs::rename(f.path(), dir.join(f.file_name())).unwrap();
        }
    }

    let s = BlastSettings { blastn_path: blastn.clone(), local_db: dir.join("coi_refs").to_string_lossy().into(), ..Default::default() };
    let info = local_blast::check(&s).unwrap();
    assert!(info.starts_with("blastn: 2."), "{info}");

    let mut job = local_blast::Job::start(&s, &query, "test").unwrap();
    let status = job.child.wait().unwrap();
    let json = job.finish(status).unwrap();
    drop(job);
    let parsed = blast::parse_results(&json).unwrap();
    assert_eq!(parsed.len(), eligible.len());
    for (q, r) in parsed.iter().zip(&eligible) {
        assert_eq!(q.query_title.split_whitespace().next(), Some(r.id.as_str()));
        assert!(q.hits.len() <= 10 && !q.hits.is_empty(), "{}: {} hits", r.id, q.hits.len());
        // Each sample finds itself, identical over its whole length.
        let top = &q.hits[0];
        assert_eq!((top.per_ident, top.query_cover), (100.0, 100.0), "{}", r.id);
        assert!(q.hits.iter().any(|h| h.title.starts_with(&format!("{} ", r.id))), "{}", r.id);
    }

    // Results attach to the samples as NCBI's do.
    let project = Project {
        format: PROJECT_FORMAT,
        name: "t".into(),
        created_at: String::new(),
        source_zip: String::new(),
        settings,
        reads: reads.clone(),
        skipped: Vec::new(),
        blast: BlastState::default(),
        compiled: None,
        pair_suggestions: Vec::new(),
        rejected_pairs: Vec::new(),
    };
    let mut h = store::ProjectHandle { path: "x.coiproj".into(), project, results: Default::default(), pending: Vec::new() };
    h.project.blast.groups.push(BlastGroup {
        id: "g0001".into(),
        sample_ids: eligible.iter().map(|r| r.id.clone()).collect(),
        fingerprints: eligible.iter().map(|r| r.derived.fingerprint.clone()).collect(),
        fasta: query.clone(),
        status: GroupStatus::Submitted,
        rid: None,
        rtoe_secs: None,
        submitted_at: None,
        finished_at: None,
        attempts: 1,
        error: None,
        last_checked_at: None,
        last_check: None,
        source: GroupSource::Local,
        failures: 0,
    });
    store::attach_results(&mut h, "g0001", parsed, json.clone());
    for r in h.project.reads.iter().filter(|r| r.derived.eligible) {
        assert!(h.result_for(r).is_some(), "{} has no result", r.id);
    }
    assert!(blast::hit_detail(&json, 3, 1).unwrap().hsps[0].midline.chars().all(|c| c == '|'));
    let _ = std::fs::remove_dir_all(&base);
}
