//! Importing folders, single files and mixed selections, and pairing real
//! forward/reverse traces from BOLD (`samples/sangeranalyseR`).

use std::path::PathBuf;

use coi_blast_lib::importer::{import, import_paths};
use coi_blast_lib::model::*;
use coi_blast_lib::pairs;
use coi_blast_lib::primers::Dir;

fn samples() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../samples")
}

fn project(reads: Vec<Read>) -> Project {
    Project {
        format: PROJECT_FORMAT,
        name: "t".into(),
        created_at: String::new(),
        source_zip: String::new(),
        settings: Settings::default(),
        reads,
        skipped: Vec::new(),
        blast: BlastState::default(),
        compiled: None,
        pair_suggestions: Vec::new(),
        rejected_pairs: Vec::new(),
    }
}

#[test]
fn folder_of_bold_pairs() {
    let s = Settings::default();
    let imported = import(&samples().join("sangeranalyseR"), &s).unwrap();
    assert_eq!(imported.reads.len(), 26);
    // The info file next to the traces is listed, not silently dropped.
    assert_eq!(imported.skipped.len(), 1);
    assert_eq!(imported.skipped[0].file_name, "TRACE_FILE_INFO.txt");
    assert_eq!(imported.files.len(), 26);

    // Every read's direction, found from its sequence, matches its file name.
    for r in &imported.reads {
        let named = pairs::name_marker(&r.id, "").map(|(_, d)| d);
        assert!(named.is_some(), "{}", r.id);
        assert_eq!(r.derived.direction, named, "{}", r.id);
        assert_eq!(r.derived.stop_codons, 0, "{}", r.id);
        assert!(r.primer.set_name.is_some(), "{}", r.id);
    }
    // All were sequenced with Folmer primers (or the LepFol cocktail that contains
    // them). Degenerate sets sharing Folmer's 3' ends are reported only where they
    // find a site that Folmer itself misses because of base-call errors.
    let folmer = imported.reads.iter().filter(|r| r.primer.set_name.as_deref() == Some("Folmer et al. 1994")).count();
    assert!(folmer >= 19, "{folmer} of 26 reads report Folmer");

    // BOLD names (X_1_F / X_2_R): all 13 pairs are suggested, none crosses specimens.
    let mut p = project(imported.reads);
    pairs::suggest(&mut p);
    assert_eq!(p.pair_suggestions.len(), 13);
    for sug in p.pair_suggestions.clone() {
        let stem = |id: &str| id.rsplitn(3, '_').nth(2).unwrap().to_string();
        assert_eq!(stem(&sug.forward_id), stem(&sug.reverse_id));
        assert!(sug.identity >= 96.0, "{sug:?}");
    }
    assert_eq!(pairs::confirm_all(&mut p), 13);
    assert!(p.pair_suggestions.is_empty());
    let samples: Vec<&Read> = p.reads.iter().filter(|r| r.paired_with.is_none()).collect();
    assert_eq!(samples.len(), 13);
    for r in samples {
        let pair = r.pair.as_ref().unwrap();
        assert!(!pair.no_overlap, "{}", r.id);
        assert!(r.derived.eligible, "{}", r.id);
        assert!(r.derived.final_len >= 450 && r.derived.final_len <= 658, "{} {}", r.id, r.derived.final_len);
        assert_eq!(r.derived.direction, Some(Dir::F));
    }
}

#[test]
fn mixed_selection() {
    let s = Settings::default();
    let dmel = samples().join("sangeranalyseR/Drosophila_melanogaster");
    let one = dmel.join("Dmel_BBDCN941-10_1_F.ab1");
    let paths = vec![
        samples().join("COI_20_sequences.zip"),
        one.clone(),
        samples().join("fasta/sangeranalyseR_reads.fa"),
        // Picked twice (once directly, once inside its folder): imported once.
        dmel.clone(),
    ];
    let imported = import_paths(&paths, &s).unwrap();
    let ab1 = imported.reads.iter().filter(|r| r.has_trace).count();
    let fasta = imported.reads.len() - ab1;
    assert_eq!(ab1, 20 + 10);
    assert_eq!(fasta, 8);
    assert_eq!(imported.reads.iter().filter(|r| r.id == "Dmel_BBDCN941-10_1_F").count(), 1);
    // The FASTA records repeat trace names; their IDs are made unique.
    assert!(imported.reads.iter().any(|r| r.id == "Achl_ACHLO006-09_1_F"));
    assert_eq!(imported.skipped.iter().map(|f| f.file_name.as_str()).collect::<Vec<_>>(), ["TRACE_FILE_INFO.txt"]);
}

#[test]
fn single_files_and_errors() {
    let s = Settings::default();
    let one = samples().join("sangeranalyseR/Drosophila_melanogaster/Dmel_PHDIP946-11_1_F.ab1");
    let imported = import(&one, &s).unwrap();
    assert_eq!(imported.reads.len(), 1);
    assert_eq!(imported.reads[0].id, "Dmel_PHDIP946-11_1_F");

    let info = samples().join("sangeranalyseR/Drosophila_melanogaster/TRACE_FILE_INFO.txt");
    let err = import(&info, &s).err().unwrap().to_string();
    assert!(err.contains("not a .zip, .ab1 or FASTA file"), "{err}");

    let err = import(&samples().join("no-such-folder"), &s).err().unwrap().to_string();
    assert!(err.contains("cannot find"), "{err}");

    let empty = std::env::temp_dir().join(format!("coi-empty-{}", std::process::id()));
    std::fs::create_dir_all(&empty).unwrap();
    let err = import(&empty, &s).err().unwrap().to_string();
    std::fs::remove_dir(&empty).ok();
    assert!(err.contains("the folder is empty"), "{err}");
}
