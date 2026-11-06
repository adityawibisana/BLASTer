//! Checks against the example batch `COI_20_sequences.zip` and the values Geneious
//! shows for it in `references/IMG_20261106_191522.jpg`.

use std::path::PathBuf;

use coi_blast_lib::importer::import_zip;
use coi_blast_lib::model::{Flag, Settings};
use coi_blast_lib::primers::Dir;

fn sample_zip() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("COI_20_sequences.zip")
}

/// (sample, raw length, % HQ, % GC) as shown by Geneious.
const GENEIOUS: &[(&str, usize, f64, f64)] = &[
    ("VN220", 719, 86.2, 32.4),
    ("VN234-2", 720, 79.6, 31.9),
    ("VN236-1", 698, 78.1, 30.9),
    ("14-2022", 978, 60.2, 42.5),
    ("VN282-1", 1029, 57.0, 44.3),
    ("VN32-1", 1179, 52.8, 48.4),
    ("VN290-3", 1123, 52.6, 51.5),
    ("VN251", 1138, 51.1, 54.3),
    ("VN60-1", 1290, 48.6, 47.8),
    ("VN239-2", 1298, 44.5, 50.2),
    ("49-2022", 1346, 44.0, 47.5),
    ("30-2022", 1245, 42.6, 51.1),
    ("274-2022", 1124, 41.9, 49.2),
    ("VN241-2", 1201, 37.9, 51.7),
    ("VN242-2", 1165, 32.4, 46.2),
    ("VN222-2", 1878, 14.7, 53.5),
];

#[test]
fn raw_stats_match_geneious() {
    let imported = import_zip(&sample_zip(), &Settings::default()).expect("import");
    assert_eq!(imported.reads.len(), 20);
    assert!(imported.skipped.is_empty());
    for (id, len, hq, gc) in GENEIOUS {
        let r = imported.reads.iter().find(|r| r.id == *id).unwrap();
        assert_eq!(r.raw_length, *len, "{id} length");
        assert!((r.raw_hq_pct - hq).abs() < 0.05, "{id} %HQ {} vs Geneious {hq}", r.raw_hq_pct);
        assert!((r.raw_gc_pct - gc).abs() < 0.05, "{id} %GC {} vs Geneious {gc}", r.raw_gc_pct);
    }
}

#[test]
fn batch_qc_results() {
    let imported = import_zip(&sample_zip(), &Settings::default()).expect("import");
    let failed: Vec<&str> =
        imported.reads.iter().filter(|r| r.derived.qc_failed).map(|r| r.id.as_str()).collect();
    assert_eq!(failed, vec!["VN222-2", "VN242-2"], "the two reads Geneious also trims to short fragments");
    for r in imported.reads.iter().filter(|r| !r.derived.qc_failed) {
        let d = &r.derived;
        assert!(d.hq_pct > 70.0, "{}", r.id);
        assert!((450..=658).contains(&d.final_len), "{} length {}", r.id, d.final_len);
        assert_eq!(d.stop_codons, 0, "{} has stop codons", r.id);
        assert_eq!(d.direction, Some(Dir::F), "{}", r.id);
        assert!(r.primer.end_site.is_some(), "{}: reverse primer site expected", r.id);
        assert!(d.flags.is_empty(), "{} flags {:?}", r.id, d.flags);
        assert!(d.eligible);
    }
    // Any primer site found must have been cut off: no final sequence still contains
    // the 3' part of the reverse primer site.
    for r in &imported.reads {
        assert!(!r.derived.final_seq.contains("TGATTTTTTGGTCACCC"), "{} still contains HCO2198", r.id);
    }
    let vn222 = imported.reads.iter().find(|r| r.id == "VN222-2").unwrap();
    assert!(vn222.derived.flags.contains(&Flag::FailedQc));
}
