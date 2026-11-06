//! Importing FASTA: the example batch's final sequences, written out and read back,
//! come back unchanged with the same sample IDs.

use std::path::PathBuf;

use coi_blast_lib::importer::{import, import_zip};
use coi_blast_lib::model::{Flag, Settings};
use coi_blast_lib::primers::Dir;

#[test]
fn final_sequences_round_trip() {
    let zip = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../COI_20_sequences.zip");
    let settings = Settings::default();
    let batch = import_zip(&zip, &settings).unwrap();
    let passed: Vec<_> = batch.reads.iter().filter(|r| !r.derived.qc_failed).collect();
    let text: String = passed.iter().map(|r| format!(">{} from the zip\n{}\n", r.id, r.derived.final_seq)).collect();
    let path = std::env::temp_dir().join(format!("coi-fasta-import-{}.fasta", std::process::id()));
    std::fs::write(&path, format!("{text}>junk\nACGT!!\n")).unwrap();

    let imported = import(&path, &settings).unwrap();
    std::fs::remove_file(&path).ok();
    assert_eq!(imported.reads.len(), passed.len());
    assert_eq!(imported.skipped.len(), 1, "the record with invalid characters");
    assert_eq!(imported.files.len(), passed.len());
    let unknown = imported.reads.iter().filter(|r| r.derived.direction.is_none()).count();
    println!("{unknown} of {} with unknown direction", imported.reads.len());
    for (r, orig) in imported.reads.iter().zip(&passed) {
        assert_eq!(r.id, orig.id);
        assert!(!r.has_trace);
        assert_eq!(r.sample_name.as_deref(), Some("from the zip"));
        assert_eq!(r.derived.final_seq, orig.derived.final_seq, "{}", r.id);
        // With the primers already removed, translation alone can fit both strands.
        assert_ne!(r.derived.direction, Some(Dir::R), "{}", r.id);
        assert!(r.derived.eligible, "{}", r.id);
        assert!(r.derived.flags.iter().all(|f| *f == Flag::DirectionUnknown), "{} {:?}", r.id, r.derived.flags);
    }
}
