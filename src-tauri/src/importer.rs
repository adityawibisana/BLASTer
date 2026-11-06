//! Import a `.zip` of `.ab1` and/or FASTA files, or a single FASTA file.

use std::io::Read as _;
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::analysis;
use crate::fasta::{self, Record};
use crate::model::{Read, Settings, SkippedFile};
use crate::store;

pub const MAX_SAMPLES: usize = 1000;
const MAX_ENTRY_BYTES: u64 = 50 * 1024 * 1024;

pub struct Imported {
    pub reads: Vec<Read>,
    pub skipped: Vec<SkippedFile>,
    /// Original data per sample as (archive entry name, bytes), stored in the project archive.
    pub files: Vec<(String, Vec<u8>)>,
}

/// One sample before analysis.
enum Item {
    Ab1 { file: String, bytes: Vec<u8> },
    Fasta { file: String, record: Record },
}

impl Item {
    /// The name the sample ID is made from.
    fn id_source(&self) -> String {
        match self {
            Item::Ab1 { file, .. } => file.clone(),
            Item::Fasta { record, .. } => record.name.replace(['/', '\\'], "_"),
        }
    }
}

fn is_junk(name: &str) -> bool {
    let base = name.rsplit('/').next().unwrap_or(name);
    name.starts_with("__MACOSX/") || base.starts_with("._") || base.is_empty() || base.eq_ignore_ascii_case("Thumbs.db")
        || base.eq_ignore_ascii_case(".DS_Store")
}

/// Import a `.zip`, or a FASTA file when the extension says so.
pub fn import(path: &Path, settings: &Settings) -> Result<Imported> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if fasta::is_fasta_name(&name) { import_fasta(path, settings) } else { import_zip(path, settings) }
}

/// Add the records of one FASTA file to `items`, and its bad records to `skipped`.
fn add_fasta(file: &str, bytes: &[u8], items: &mut Vec<Item>, skipped: &mut Vec<SkippedFile>) -> Result<(), String> {
    for rec in fasta::parse(bytes)? {
        match rec {
            Ok(record) => items.push(Item::Fasta { file: file.to_string(), record }),
            Err((name, reason)) => skipped.push(SkippedFile { file_name: format!("{file}: {name}"), reason }),
        }
    }
    Ok(())
}

pub fn import_fasta(path: &Path, settings: &Settings) -> Result<Imported> {
    let size = std::fs::metadata(path).with_context(|| format!("cannot open {}", path.display()))?.len();
    if size > MAX_ENTRY_BYTES {
        bail!("the FASTA file is too large");
    }
    let bytes = std::fs::read(path).with_context(|| format!("cannot open {}", path.display()))?;
    let file = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let (mut items, mut skipped) = (Vec::new(), Vec::new());
    add_fasta(&file, &bytes, &mut items, &mut skipped).map_err(anyhow::Error::msg)?;
    build(items, skipped, settings, "the FASTA file")
}

pub fn import_zip(path: &Path, settings: &Settings) -> Result<Imported> {
    let file = std::fs::File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let mut archive = zip::ZipArchive::new(file).context("the file is not a valid zip archive, or it is corrupt")?;

    let mut ab1: Vec<Item> = Vec::new();
    let mut fastas: Vec<(String, Vec<u8>)> = Vec::new();
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
        let is_fasta = fasta::is_fasta_name(&base);
        if !is_fasta && !base.to_ascii_lowercase().ends_with(".ab1") {
            skipped.push(SkippedFile { file_name: base, reason: "not an .ab1 or FASTA file".into() });
            continue;
        }
        if entry.size() > MAX_ENTRY_BYTES {
            let kind = if is_fasta { "a FASTA file" } else { "an .ab1 trace" };
            skipped.push(SkippedFile { file_name: base, reason: format!("file is too large for {kind}") });
            continue;
        }
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        if let Err(e) = entry.read_to_end(&mut bytes) {
            skipped.push(SkippedFile { file_name: base, reason: format!("cannot be read from the zip: {e}") });
            continue;
        }
        if is_fasta {
            fastas.push((base, bytes));
        } else {
            ab1.push(Item::Ab1 { file: base, bytes });
        }
    }
    if ab1.is_empty() && fastas.is_empty() && skipped.is_empty() {
        bail!("the zip is empty");
    }
    // Traces in name order, then FASTA files in name order with their records in file order.
    ab1.sort_by_key(|item| natural_key(&item.id_source()));
    fastas.sort_by(|a, b| natural_key(&a.0).cmp(&natural_key(&b.0)));
    let mut items = ab1;
    for (file, bytes) in fastas {
        if let Err(reason) = add_fasta(&file, &bytes, &mut items, &mut skipped) {
            skipped.push(SkippedFile { file_name: file, reason });
        }
    }
    build(items, skipped, settings, "the zip")
}

fn build(items: Vec<Item>, mut skipped: Vec<SkippedFile>, settings: &Settings, source: &str) -> Result<Imported> {
    if items.len() > MAX_SAMPLES {
        bail!("{source} holds {} samples; a batch can hold at most {MAX_SAMPLES}", items.len());
    }
    let names: Vec<String> = items.iter().map(Item::id_source).collect();
    let ids = analysis::unique_ids(&names);
    let sets = analysis::primer_sets(settings);
    let mut reads = Vec::new();
    let mut files = Vec::new();
    for (item, id) in items.into_iter().zip(ids) {
        let mut read = match item {
            Item::Ab1 { file, bytes } => match crate::abif::parse(&bytes) {
                Ok(trace) if !trace.bases.is_empty() => {
                    files.push((store::ab1_entry(&id), bytes));
                    analysis::new_read(id, file, &trace)
                }
                Ok(_) => {
                    skipped.push(SkippedFile { file_name: file, reason: "the trace has no base calls".into() });
                    continue;
                }
                Err(e) => {
                    skipped.push(SkippedFile { file_name: file, reason: format!("cannot be parsed: {e}") });
                    continue;
                }
            },
            Item::Fasta { file, record } => {
                let header = match &record.description {
                    Some(d) => format!("{} {d}", record.name),
                    None => record.name.clone(),
                };
                let original = format!(">{header}\n{}\n", String::from_utf8_lossy(&record.bases));
                files.push((store::fasta_entry(&id), original.into_bytes()));
                analysis::new_fasta_read(id, file, record.description, &record.bases)
            }
        };
        analysis::analyze(&mut read, settings, &sets);
        reads.push(read);
    }
    if reads.is_empty() {
        bail!("no readable .ab1 traces or FASTA sequences were found in {source}");
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
