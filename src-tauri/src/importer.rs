//! Import a `.zip` of `.ab1` files.

use std::io::Read as _;
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::analysis;
use crate::model::{Read, Settings, SkippedFile};

pub const MAX_SAMPLES: usize = 1000;
const MAX_ENTRY_BYTES: u64 = 50 * 1024 * 1024;

pub struct Imported {
    pub reads: Vec<Read>,
    pub skipped: Vec<SkippedFile>,
    /// Original file bytes per sample ID, stored in the project archive.
    pub files: Vec<(String, Vec<u8>)>,
}

fn is_junk(name: &str) -> bool {
    let base = name.rsplit('/').next().unwrap_or(name);
    name.starts_with("__MACOSX/") || base.starts_with("._") || base.is_empty() || base.eq_ignore_ascii_case("Thumbs.db")
        || base.eq_ignore_ascii_case(".DS_Store")
}

pub fn import_zip(path: &Path, settings: &Settings) -> Result<Imported> {
    let file = std::fs::File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let mut archive = zip::ZipArchive::new(file).context("the file is not a valid zip archive, or it is corrupt")?;

    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    let mut skipped = Vec::new();
    for i in 0..archive.len() {
        let mut entry = match archive.by_index(i) {
            Ok(e) => e,
            Err(zip::result::ZipError::UnsupportedArchive(msg)) if msg.contains("assword") => {
                bail!("the zip is password-protected; please unzip it or create it without a password")
            }
            Err(e) => bail!("the zip is corrupt: {e}"),
        };
        if entry.is_dir() {
            continue;
        }
        if entry.encrypted() {
            bail!("the zip is password-protected; please unzip it or create it without a password");
        }
        let name = entry.name().context("the zip has an unreadable file name")?.replace('\\', "/");
        if is_junk(&name) {
            continue;
        }
        let base = name.rsplit('/').next().unwrap_or(&name).to_string();
        if !base.to_ascii_lowercase().ends_with(".ab1") {
            skipped.push(SkippedFile { file_name: base, reason: "not an .ab1 file".into() });
            continue;
        }
        if entry.size() > MAX_ENTRY_BYTES {
            skipped.push(SkippedFile { file_name: base, reason: "file is too large for an .ab1 trace".into() });
            continue;
        }
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        if let Err(e) = entry.read_to_end(&mut bytes) {
            skipped.push(SkippedFile { file_name: base, reason: format!("cannot be read from the zip: {e}") });
            continue;
        }
        entries.push((base, bytes));
    }
    if entries.is_empty() && skipped.is_empty() {
        bail!("the zip is empty");
    }
    if entries.len() > MAX_SAMPLES {
        bail!("the zip holds {} .ab1 files; a batch can hold at most {MAX_SAMPLES}", entries.len());
    }
    entries.sort_by(|a, b| natural_key(&a.0).cmp(&natural_key(&b.0)));

    let names: Vec<String> = entries.iter().map(|e| e.0.clone()).collect();
    let ids = analysis::unique_ids(&names);
    let sets = analysis::primer_sets(settings);
    let mut reads = Vec::new();
    let mut files = Vec::new();
    for ((name, bytes), id) in entries.into_iter().zip(ids) {
        match crate::abif::parse(&bytes) {
            Ok(trace) if !trace.bases.is_empty() => {
                let mut read = analysis::new_read(id.clone(), name, &trace);
                analysis::analyze(&mut read, settings, &sets);
                reads.push(read);
                files.push((id, bytes));
            }
            Ok(_) => skipped.push(SkippedFile { file_name: name, reason: "the trace has no base calls".into() }),
            Err(e) => skipped.push(SkippedFile { file_name: name, reason: format!("cannot be parsed: {e}") }),
        }
    }
    if reads.is_empty() {
        bail!("no readable .ab1 files were found in the zip");
    }
    Ok(Imported { reads, skipped, files })
}

/// Sort key that orders "VN24" before "VN220".
fn natural_key(s: &str) -> Vec<(u8, u64, String)> {
    let mut out = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_ascii_digit() {
            let mut n = String::new();
            while let Some(&d) = chars.peek().filter(|d| d.is_ascii_digit()) {
                n.push(d);
                chars.next();
            }
            out.push((0, n.parse().unwrap_or(u64::MAX), String::new()));
        } else {
            let mut t = String::new();
            while let Some(&d) = chars.peek().filter(|d| !d.is_ascii_digit()) {
                t.push(d.to_ascii_lowercase());
                chars.next();
            }
            out.push((1, 0, t));
        }
    }
    out
}
