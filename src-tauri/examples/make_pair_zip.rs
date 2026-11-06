//! Test data: a zip with a forward/reverse pair for trying pairing in the app.
//! S1_F.ab1 is VN234-2 unchanged; S1_R.ab1 is a synthetic reverse read of the same
//! template (read from the HCO2198 side, ending in revcomp(LCO1490)), written as a
//! valid ABIF file with drawn trace peaks. Three real single reads are added.
//! Usage: cargo run --example make_pair_zip -- <COI_20_sequences.zip> <out.zip>

use std::io::{Read as _, Write as _};

use coi_blast_lib::primers::builtin_library;
use coi_blast_lib::seq::reverse_complement;

fn entry(dir: &mut Vec<u8>, data: &mut Vec<u8>, base: usize, name: &[u8; 4], number: i32, etype: i16, esize: i16, payload: &[u8]) {
    let n = payload.len() / esize as usize;
    dir.extend_from_slice(name);
    dir.extend_from_slice(&number.to_be_bytes());
    dir.extend_from_slice(&etype.to_be_bytes());
    dir.extend_from_slice(&esize.to_be_bytes());
    dir.extend_from_slice(&(n as i32).to_be_bytes());
    dir.extend_from_slice(&(payload.len() as i32).to_be_bytes());
    if payload.len() <= 4 {
        let mut inline = [0u8; 4];
        inline[..payload.len()].copy_from_slice(payload);
        dir.extend_from_slice(&inline);
    } else {
        dir.extend_from_slice(&((base + data.len()) as i32).to_be_bytes());
        data.extend_from_slice(payload);
    }
    dir.extend_from_slice(&0i32.to_be_bytes());
}

/// Minimal ABIF file: base calls, qualities, peaks, four trace channels, sample name.
fn write_abif(name: &str, bases: &[u8], quals: &[u8]) -> Vec<u8> {
    let spacing = 12usize;
    let peaks: Vec<u16> = (0..bases.len()).map(|i| (i * spacing + 6) as u16).collect();
    let len = bases.len() * spacing + 12;
    let order = b"GATC";
    let mut channels = vec![vec![0i16; len]; 4];
    for (i, &b) in bases.iter().enumerate() {
        let height = 200.0 + quals[i] as f64 * 18.0;
        let ch = order.iter().position(|&o| o == b).unwrap_or(0);
        for x in 0..len {
            let dx = x as f64 - peaks[i] as f64;
            if dx.abs() < 14.0 {
                channels[ch][x] += (height * (-dx * dx / 18.0).exp()) as i16;
            }
        }
        if quals[i] < 20 {
            // Low quality: a competing peak in another channel.
            let other = (ch + 1 + i % 3) % 4;
            for x in 0..len {
                let dx = x as f64 - peaks[i] as f64;
                if dx.abs() < 14.0 {
                    channels[other][x] += (height * 0.7 * (-dx * dx / 18.0).exp()) as i16;
                }
            }
        }
    }
    let base = 128usize;
    let mut dir = Vec::new();
    let mut data = Vec::new();
    entry(&mut dir, &mut data, base, b"PBAS", 2, 2, 1, bases);
    entry(&mut dir, &mut data, base, b"PCON", 2, 2, 1, quals);
    let ploc: Vec<u8> = peaks.iter().flat_map(|p| p.to_be_bytes()).collect();
    entry(&mut dir, &mut data, base, b"PLOC", 2, 4, 2, &ploc);
    for (k, ch) in channels.iter().enumerate() {
        let bytes: Vec<u8> = ch.iter().flat_map(|v| v.to_be_bytes()).collect();
        entry(&mut dir, &mut data, base, b"DATA", 9 + k as i32, 4, 2, &bytes);
    }
    entry(&mut dir, &mut data, base, b"FWO_", 1, 2, 1, order);
    let mut smpl = vec![name.len() as u8];
    smpl.extend_from_slice(name.as_bytes());
    entry(&mut dir, &mut data, base, b"SMPL", 1, 18, 1, &smpl);

    let n_entries = dir.len() / 28;
    let mut out = Vec::new();
    out.extend_from_slice(b"ABIF");
    out.extend_from_slice(&101u16.to_be_bytes());
    out.extend_from_slice(b"tdir");
    out.extend_from_slice(&1i32.to_be_bytes());
    out.extend_from_slice(&1023i16.to_be_bytes());
    out.extend_from_slice(&28i16.to_be_bytes());
    out.extend_from_slice(&(n_entries as i32).to_be_bytes());
    out.extend_from_slice(&(dir.len() as i32).to_be_bytes());
    out.extend_from_slice(&((base + data.len()) as i32).to_be_bytes());
    out.extend_from_slice(&0i32.to_be_bytes());
    out.resize(base, 0);
    out.extend_from_slice(&data);
    out.extend_from_slice(&dir);
    out
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let mut src = zip::ZipArchive::new(std::fs::File::open(&args[1])?)?;
    let mut read_file = |name: &str| -> anyhow::Result<Vec<u8>> {
        let mut out = Vec::new();
        src.by_name(name)?.read_to_end(&mut out)?;
        Ok(out)
    };
    let f_bytes = read_file("VN234-2.ab1")?;
    let singles = [("VN24-1.ab1", read_file("VN24-1.ab1")?), ("VN248-2.ab1", read_file("VN248-2.ab1")?), ("VN220.ab1", read_file("VN220.ab1")?)];

    let trace = coi_blast_lib::abif::parse(&f_bytes)?;
    let s = coi_blast_lib::model::Settings::default();
    let mut f = coi_blast_lib::analysis::new_read("S1_F".into(), "S1_F.ab1".into(), &trace);
    coi_blast_lib::analysis::analyze(&mut f, &s, &coi_blast_lib::analysis::primer_sets(&s));
    let hco = f.primer.end_site.as_ref().expect("HCO site").start;
    let lco = builtin_library()[0].forward.as_bytes().to_vec();
    let template = [lco.as_slice(), &trace.bases[..hco]].concat();
    let rc = reverse_complement(&template);
    // 25 unreadable bases after the primer are not called; quality falls off at the far end.
    let mut bases = rc[25..].to_vec();
    let n = bases.len();
    let mut quals: Vec<u8> = (0..n).map(|i| if i < 15 { 18 + i as u8 * 2 } else if i > n - 40 { 38 } else { 52 }).collect();
    bases.extend_from_slice(b"GGGTGGGGGAGTGGGGTGGG");
    quals.extend(std::iter::repeat_n(4u8, 20));
    let r_bytes = write_abif("S1_R", &bases, &quals);

    let mut out = zip::ZipWriter::new(std::fs::File::create(&args[2])?);
    let opts = zip::write::SimpleFileOptions::default();
    for (name, bytes) in [("S1_F.ab1", f_bytes.as_slice()), ("S1_R.ab1", r_bytes.as_slice())]
        .into_iter()
        .chain(singles.iter().map(|(n, b)| (*n, b.as_slice())))
    {
        out.start_file(name, opts)?;
        out.write_all(bytes)?;
    }
    out.finish()?;
    println!("wrote {}", args[2]);
    Ok(())
}
