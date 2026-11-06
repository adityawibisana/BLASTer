//! Forward/reverse assembly on a realistic pair built from a real trace.
//!
//! The example batch only has single reads, so the reverse read is synthesised from
//! VN234-2: the template's other strand, read from the HCO2198 side, starting 25
//! bases after its primer, ending in revcomp(LCO1490) followed by noise.

use std::path::PathBuf;

use coi_blast_lib::abif::AbifTrace;
use coi_blast_lib::model::*;
use coi_blast_lib::primers::{Dir, builtin_library};
use coi_blast_lib::seq::reverse_complement;
use coi_blast_lib::{analysis, fasta, importer, pairs};

fn sample_read(id: &str) -> Read {
    let zip = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../COI_20_sequences.zip");
    importer::import_zip(&zip, &Settings::default()).unwrap().reads.into_iter().find(|r| r.id == id).unwrap()
}

fn trace(bases: Vec<u8>, quals: Vec<u8>) -> AbifTrace {
    AbifTrace { sample_name: None, bases, quals, peaks: Vec::new(), traces: Default::default() }
}

fn project(reads: Vec<Read>) -> Project {
    Project {
        format: PROJECT_FORMAT,
        name: "pairs".into(),
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

fn make_pair() -> (Read, Read, String) {
    let s = Settings::default();
    let sets = analysis::primer_sets(&s);
    let real = sample_read("VN234-2");
    let bases = real.bases.as_bytes();
    let hco = real.primer.end_site.as_ref().expect("VN234-2 has its HCO2198 site").start;
    // The full barcode between the primers: the real read from its first base up to the HCO site.
    let barcode = &bases[..hco];
    let lco = builtin_library()[0].forward.as_bytes().to_vec();

    // Forward read: the real trace.
    let mut f = analysis::new_read("S1_F".into(), "S1_F.ab1".into(), &trace(bases.to_vec(), real.quals.clone()));
    analysis::analyze(&mut f, &s, &sets);

    // Reverse read: revcomp(LCO + barcode), minus the first 25 bases after the HCO
    // primer, then noise. High quality throughout the barcode.
    let template = [lco.as_slice(), barcode].concat();
    let rc = reverse_complement(&template);
    let mut r_bases = rc[25..].to_vec();
    let mut r_quals = vec![52u8; r_bases.len()];
    r_bases.extend_from_slice(b"GGGTGGGGGAGTGGGG");
    r_quals.extend(std::iter::repeat_n(4u8, 16));
    let mut r = analysis::new_read("S1_R".into(), "S1_R.ab1".into(), &trace(r_bases, r_quals));
    analysis::analyze(&mut r, &s, &sets);
    (f, r, String::from_utf8(barcode.to_vec()).unwrap())
}

#[test]
fn reverse_read_is_detected_and_trimmed() {
    let (_, r, _) = make_pair();
    assert_eq!(r.derived.direction, Some(Dir::R));
    assert_eq!(r.primer.end_site.as_ref().map(|s| s.primer.as_str()), Some("LCO1490"));
    assert!(r.derived.flags.is_empty(), "{:?}", r.derived.flags);
}

#[test]
fn pair_is_suggested_confirmed_and_assembled() {
    let (f, r, barcode) = make_pair();
    let f_alone = f.derived.final_len;
    let other = sample_read("VN236-1");
    let mut p = project(vec![f, r, other]);
    pairs::suggest(&mut p);
    assert_eq!(p.pair_suggestions.len(), 1, "{:?}", p.pair_suggestions);
    let sug = &p.pair_suggestions[0];
    assert_eq!((sug.forward_id.as_str(), sug.reverse_id.as_str()), ("S1_F", "S1_R"));

    pairs::confirm(&mut p, "S1_F", "S1_R").unwrap();
    assert!(p.pair_suggestions.is_empty());
    let f = p.reads.iter().find(|x| x.id == "S1_F").unwrap();
    let r = p.reads.iter().find(|x| x.id == "S1_R").unwrap();
    let info = f.pair.as_ref().unwrap();
    assert!(!info.no_overlap);
    assert!(info.conflicts.is_empty(), "{:?}", info.conflicts);
    assert!(info.overlap > 500, "overlap {}", info.overlap);

    // The consensus reaches the start of the barcode that the forward read lost to trimming,
    // and ends where the forward read's HCO site starts.
    let d = &f.derived;
    assert!(d.final_len > f_alone, "consensus {} vs forward alone {f_alone}", d.final_len);
    assert!(barcode.ends_with(&d.final_seq[d.final_seq.len() - 200..]));
    assert!(barcode.contains(&d.final_seq[..200]), "consensus start is part of the barcode");
    assert!(!d.final_seq.contains("GGTCAACAAATCATAAAGATATTGG"), "LCO1490 must not be in the consensus");
    assert_eq!(d.stop_codons, 0);
    assert!(d.eligible);
    assert!(!r.derived.eligible, "the reverse read is never searched on its own");

    let header = fasta::header(f, fasta::partner(&p.reads, f));
    assert!(header.starts_with("S1_F consensus F:"), "{header}");
    assert!(header.ends_with("primers:LCO1490/HCO2198"), "{header}");

    // A trim edit on the reverse read flows into the consensus.
    let before = p.reads.iter().find(|x| x.id == "S1_F").unwrap().derived.final_len;
    let rr = p.reads.iter_mut().find(|x| x.id == "S1_R").unwrap();
    let t = rr.auto_trim;
    rr.manual_trim = Some(Range { start: t.start, end: t.end - 60 });
    pairs::refresh_read(&mut p, "S1_R");
    let after = p.reads.iter().find(|x| x.id == "S1_F").unwrap();
    assert!(after.derived.final_len < before);
    assert!(after.derived.flags.contains(&Flag::Edited));

    // Swap and unpair.
    pairs::swap(&mut p, "S1_F").unwrap();
    assert!(p.reads.iter().find(|x| x.id == "S1_R").unwrap().pair.is_some());
    pairs::unpair(&mut p, "S1_R");
    assert!(p.reads.iter().all(|x| x.pair.is_none() && x.paired_with.is_none()));
}

#[test]
fn phase1_project_files_still_load() {
    // A Phase 1 project.json has no pair fields at all.
    let (f, _, _) = make_pair();
    let mut v = serde_json::to_value(project(vec![f])).unwrap();
    v.as_object_mut().unwrap().remove("pairSuggestions");
    v.as_object_mut().unwrap().remove("rejectedPairs");
    for r in v["reads"].as_array_mut().unwrap() {
        r.as_object_mut().unwrap().remove("pair");
        r.as_object_mut().unwrap().remove("pairedWith");
    }
    let p: Project = serde_json::from_value(v).expect("old project format loads");
    assert!(p.pair_suggestions.is_empty() && p.reads[0].pair.is_none());
}

#[test]
fn reads_that_do_not_overlap_need_a_choice() {
    let (f, _, _) = make_pair();
    let mut p = project(vec![f, sample_read("VN24-1")]);
    // Pair the forward read with an unrelated Culex read by hand.
    p.reads[1].id = "S1_R".into();
    p.reads[1].direction_manual = Some(Dir::R);
    let s = p.settings.clone();
    analysis::refresh(&mut p.reads[1], &s);
    pairs::confirm(&mut p, "S1_F", "S1_R").unwrap();
    let f = &p.reads[0];
    assert!(f.pair.as_ref().unwrap().no_overlap);
    assert!(f.derived.flags.contains(&Flag::NoOverlap));
    assert!(!f.derived.eligible, "not searched until the user picks a read");
    pairs::set_use_only(&mut p, "S1_F", Some(Dir::F));
    assert!(p.reads[0].derived.eligible);
}
