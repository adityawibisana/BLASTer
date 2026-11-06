//! Project lifecycle with a real NCBI response (`tests/fixtures/ncbi_18_samples.json`,
//! RID EVN9M6ED014, the 18 samples of the example batch that pass QC):
//! create, attach results, save, reopen offline, compile, export, outdated detection.

use std::io::Read as _;
use std::path::PathBuf;

use coi_blast_lib::model::*;
use coi_blast_lib::{analysis, blast, compile, export, fasta, importer, store, views};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("coi-blast-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn new_project(dir: &std::path::Path) -> store::ProjectHandle {
    let settings = Settings::default();
    let imported = importer::import_zip(&root().join("../COI_20_sequences.zip"), &settings).unwrap();
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
    };
    store::create(&dir.join("test.coiproj"), project, imported.files).unwrap()
}

fn add_fixture_group(h: &mut store::ProjectHandle) {
    let eligible: Vec<&Read> = h.project.reads.iter().filter(|r| r.derived.eligible).collect();
    assert_eq!(eligible.len(), 18);
    h.project.blast.groups.push(BlastGroup {
        id: "g0001".into(),
        sample_ids: eligible.iter().map(|r| r.id.clone()).collect(),
        fingerprints: eligible.iter().map(|r| r.derived.fingerprint.clone()).collect(),
        fasta: fasta::multi(eligible.iter().copied()),
        status: GroupStatus::Submitted,
        rid: Some("EVN9M6ED014".into()),
        rtoe_secs: Some(30),
        submitted_at: Some(chrono::Local::now().to_rfc3339()),
        finished_at: None,
        attempts: 1,
        error: None,
    });
    let json = std::fs::read_to_string(root().join("tests/fixtures/ncbi_18_samples.json")).unwrap();
    let parsed = blast::parse_results(&json).unwrap();
    store::attach_results(h, "g0001", parsed, json);
}

#[test]
fn full_project_lifecycle() {
    let dir = temp_dir("lifecycle");
    let mut h = new_project(&dir);
    add_fixture_group(&mut h);

    for r in h.project.reads.iter().filter(|r| r.derived.eligible) {
        let q = h.result_for(r).unwrap_or_else(|| panic!("{} has no result", r.id));
        assert!(q.query_title.starts_with(&r.id), "{} matched to {}", r.id, q.query_title);
        assert_eq!(q.hits.len(), 100);
        assert!(!h.outdated(r));
    }

    // Save, reopen without network, results are still there.
    store::write(Some(&h.path), &h.path, &h.project, &h.pending).unwrap();
    let mut h = store::open(&dir.join("test.coiproj")).unwrap();
    assert_eq!(h.results.len(), 1);
    assert_eq!(h.project.blast.groups[0].status, GroupStatus::Ready);

    // Compile: one species per searched sample, counts add up.
    let c = {
        let hr = &h;
        compile::compile(
            &hr.project.reads,
            |r| views::result_for_compile(hr, r),
            |r| views::sample_flags(hr, r),
            hr.project.settings.compile_min_cover,
        )
    };
    let species: Vec<_> = c.rows.iter().filter(|r| r.species.is_some()).collect();
    assert_eq!(species.len(), 18);
    assert_eq!(c.chart.iter().map(|s| s.count).sum::<usize>(), 18);
    let get = |id: &str| c.rows.iter().find(|r| r.sample_id == id).unwrap();
    assert_eq!(get("VN234-2").species.as_deref(), Some("Aedes albopictus"));
    assert_eq!(get("VN24-1").species.as_deref(), Some("Culex tritaeniorhynchus"));
    assert_eq!(get("49-2022").species.as_deref(), Some("Armigeres subalbatus"));
    assert_eq!(get("VN222-2").reason.as_deref(), Some("failedQc"));
    // The rules hold for every row: no counted hit has higher identity than the winner.
    for row in species {
        let read = h.project.reads.iter().find(|r| r.id == row.sample_id).unwrap();
        let q = h.result_for(read).unwrap();
        let best = q.hits.iter().filter(|x| x.query_cover >= 80.0).map(|x| x.per_ident).fold(0.0, f64::max);
        assert!((row.identity.unwrap() - best).abs() < 1e-9, "{}", row.sample_id);
    }
    for row in &c.chart {
        println!("{:>3}  {:5.1}%  {}", row.count, row.pct, row.species);
    }
    h.project.compiled = Some(c);
    assert!(!views::recompile_needed(&h));

    // Alignment detail comes from the raw response inside the archive.
    let read = h.project.reads.iter().find(|r| r.id == "VN234-2").unwrap();
    let raw = store::read_entry(&h.path, &store::raw_entry("g0001")).unwrap();
    let detail = blast::hit_detail(std::str::from_utf8(&raw).unwrap(), read.result.as_ref().unwrap().index, 1).unwrap();
    assert_eq!(detail.hsps[0].qseq.len(), detail.hsps[0].hseq.len());
    assert!(!detail.hsps[0].midline.is_empty());

    // Excel export has every sheet.
    let xlsx = dir.join("export.xlsx");
    export::excel(&h, &xlsx).unwrap();
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&xlsx).unwrap()).unwrap();
    let mut wb = String::new();
    zip.by_name("xl/workbook.xml").unwrap().read_to_string(&mut wb).unwrap();
    for sheet in ["Overview", "Compiled", "QC", "Hits", "Sequences", "Run info"] {
        assert!(wb.contains(&format!("name=\"{sheet}\"")), "missing sheet {sheet}");
    }
    assert!(zip.by_name("xl/charts/chart1.xml").is_ok(), "species chart missing");

    // FASTA export and raw results.
    assert_eq!(export::fasta(&h, &dir.join("out.fasta"), true).unwrap(), 18);
    assert_eq!(export::raw_results(&h, &dir.join("raw.zip")).unwrap(), 1);

    // A manual trim makes that sample's result outdated and the compile stale.
    let settings = h.project.settings.clone();
    let r = h.project.reads.iter_mut().find(|r| r.id == "VN31-1").unwrap();
    let t = r.auto_trim;
    r.manual_trim = Some(Range { start: t.start + 6, end: t.end });
    analysis::refresh(r, &settings);
    assert!(r.derived.flags.contains(&Flag::Edited));
    let r = h.project.reads.iter().find(|r| r.id == "VN31-1").unwrap();
    assert!(h.outdated(r));
    assert!(views::sample_flags(&h, r).contains(&Flag::Outdated));
    assert!(views::recompile_needed(&h));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn compile_scales_to_1000_samples() {
    // 1000 samples built from the 18 real results (names made unique), compiled from saved results.
    let dir = temp_dir("scale");
    let mut h = new_project(&dir);
    add_fixture_group(&mut h);
    let base: Vec<Read> = h.project.reads.iter().filter(|r| r.derived.eligible).cloned().collect();
    let mut reads = Vec::new();
    for i in 0..1000 {
        let mut r = base[i % base.len()].clone();
        r.id = format!("{}_{i}", r.id);
        reads.push(r);
    }
    h.project.reads = reads;
    let start = std::time::Instant::now();
    let c = {
        let hr = &h;
        compile::compile(&hr.project.reads, |r| views::result_for_compile(hr, r), |r| views::sample_flags(hr, r), 80.0)
    };
    let elapsed = start.elapsed();
    assert_eq!(c.rows.len(), 1000);
    assert_eq!(c.chart.iter().map(|s| s.count).sum::<usize>(), 1000);
    assert!(elapsed.as_secs_f64() < 10.0, "compile took {elapsed:?}");
    println!("compiled 1000 samples in {elapsed:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
