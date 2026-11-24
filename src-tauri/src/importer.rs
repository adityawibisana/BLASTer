//! Import `.ab1` traces and FASTA files from any mix of zips, single files and folders.

use std::collections::HashSet;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::analysis;
use crate::fasta::{self, Record};
use crate::model::{Read, Settings, SkippedFile};
use crate::store;

pub const MAX_SAMPLES: usize = 1000;
const MAX_ENTRY_BYTES: u64 = 50 * 1024 * 1024;
/// Folders deeper than this are not searched (guards against link loops).
const MAX_DEPTH: usize = 16;
/// Stop walking a folder tree after this many entries (e.g. a whole drive was picked).
const MAX_WALK_ENTRIES: usize = 100_000;

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

fn is_zip_name(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".zip")
}

fn is_ab1_name(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".ab1")
}

fn base_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Import one selected path: a `.zip`, a single `.ab1`, a FASTA file or a folder.
pub fn import(path: &Path, settings: &Settings) -> Result<Imported> {
    import_paths(&[path.to_path_buf()], settings)
}

/// Import any mix of `.zip` archives, `.ab1` traces, FASTA files and folders.
/// Folders are searched recursively, and zips found in them are opened.
pub fn import_paths(paths: &[PathBuf], settings: &Settings) -> Result<Imported> {
    if paths.is_empty() {
        bail!("nothing was selected");
    }
    // A single selected file reports its own problem as the error.
    let strict = paths.len() == 1;
    let mut c = Collector::default();
    for path in paths {
        if path.is_dir() {
            c.add_dir(path, 0)?;
        } else {
            c.add_path(path, strict)?;
        }
    }
    let source = match paths {
        [p] if p.is_dir() => "the folder",
        [p] if is_zip_name(&base_name(p)) => "the zip",
        [p] if fasta::is_fasta_name(&base_name(p)) => "the FASTA file",
        [_] => "the file",
        _ => "the selection",
    };
    if c.is_empty() {
        bail!("{source} is empty");
    }
    c.finish(settings, source, strict)
}

/// Import a `.zip` of `.ab1` and/or FASTA files.
pub fn import_zip(path: &Path, settings: &Settings) -> Result<Imported> {
    let mut c = Collector::default();
    c.add_zip(path)?;
    if c.is_empty() {
        bail!("the zip is empty");
    }
    c.finish(settings, "the zip", false)
}

/// Import a single FASTA file.
pub fn import_fasta(path: &Path, settings: &Settings) -> Result<Imported> {
    let mut c = Collector::default();
    c.add_path(path, true)?;
    c.finish(settings, "the FASTA file", true)
}

/// A short description of a selection, e.g. "samples (folder)" or "a.ab1 (+25 more)".
pub fn describe(paths: &[PathBuf]) -> String {
    let name = |p: &PathBuf| {
        let n = base_name(p);
        if n.is_empty() { p.display().to_string() } else { n }
    };
    match paths {
        [] => String::new(),
        [p] if p.is_dir() => format!("{} (folder)", name(p)),
        [p] => name(p),
        [p, rest @ ..] => format!("{} (+{} more)", name(p), rest.len()),
    }
}

/// A name for a project made from a selection: the file or folder name for one
/// path, otherwise the name of the folder the selected items are in.
pub fn project_name(paths: &[PathBuf]) -> String {
    let name = match paths {
        [p] if p.is_dir() => Some(base_name(p)),
        [p] => p.file_stem().map(|s| s.to_string_lossy().into_owned()),
        [first, ..] => {
            let parent = first.parent();
            if paths.iter().all(|p| p.parent() == parent) { parent.map(base_name) } else { None }
        }
        [] => None,
    };
    name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| "Batch".into())
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

/// Files gathered from a selection, before analysis.
#[derive(Default)]
struct Collector {
    ab1: Vec<Item>,
    fastas: Vec<(String, Vec<u8>)>,
    skipped: Vec<SkippedFile>,
    /// Files already taken from disk, so a file picked twice is imported once.
    seen: HashSet<PathBuf>,
    walked: usize,
}

impl Collector {
    fn is_empty(&self) -> bool {
        self.ab1.is_empty() && self.fastas.is_empty() && self.skipped.is_empty()
    }

    fn skip(&mut self, file_name: String, reason: impl Into<String>) {
        self.skipped.push(SkippedFile { file_name, reason: reason.into() });
    }

    /// Check one file by its base name and size, then read it with `read`.
    fn add_file(
        &mut self,
        base: String,
        size: u64,
        read: impl FnOnce() -> std::io::Result<Vec<u8>>,
        from: &str,
    ) -> Result<()> {
        let is_fasta = fasta::is_fasta_name(&base);
        if !is_fasta && !is_ab1_name(&base) {
            self.skip(base, "not an .ab1 or FASTA file");
            return Ok(());
        }
        if size > MAX_ENTRY_BYTES {
            let kind = if is_fasta { "a FASTA file" } else { "an .ab1 trace" };
            self.skip(base, format!("file is too large for {kind}"));
            return Ok(());
        }
        match read() {
            Ok(bytes) if is_fasta => self.fastas.push((base, bytes)),
            Ok(bytes) => {
                self.ab1.push(Item::Ab1 { file: base, bytes });
                if self.ab1.len() > MAX_SAMPLES {
                    bail!("the selection holds more than {MAX_SAMPLES} traces; a batch can hold at most {MAX_SAMPLES}");
                }
            }
            Err(e) => self.skip(base, format!("cannot be read{from}: {e}")),
        }
        Ok(())
    }

    /// Add a file from disk. A zip is opened; anything else is checked like a zip entry.
    /// With `strict` (the only selected item), problems are errors instead of skipped files.
    fn add_path(&mut self, path: &Path, strict: bool) -> Result<()> {
        let base = base_name(path);
        if is_junk(&base) {
            return Ok(());
        }
        if !path.exists() {
            if strict {
                bail!("cannot find {}", path.display());
            }
            self.skip(base, "file not found");
            return Ok(());
        }
        let key = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        if !self.seen.insert(key) {
            return Ok(());
        }
        if is_zip_name(&base) {
            return match self.add_zip(path) {
                Err(e) if !strict => {
                    self.skip(base, format!("{e:#}"));
                    Ok(())
                }
                other => other,
            };
        }
        if strict && !fasta::is_fasta_name(&base) && !is_ab1_name(&base) {
            bail!("{base} is not a .zip, .ab1 or FASTA file");
        }
        let size = match std::fs::metadata(path) {
            Ok(m) => m.len(),
            Err(e) if strict => return Err(e).with_context(|| format!("cannot open {}", path.display())),
            Err(e) => {
                self.skip(base, format!("cannot be read: {e}"));
                return Ok(());
            }
        };
        self.add_file(base, size, || std::fs::read(path), "")
    }

    /// Add everything in a folder and its subfolders. Hidden folders are skipped.
    fn add_dir(&mut self, dir: &Path, depth: usize) -> Result<()> {
        if depth > MAX_DEPTH {
            return Ok(());
        }
        let entries = std::fs::read_dir(dir).with_context(|| format!("cannot open the folder {}", dir.display()))?;
        let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
        paths.sort_by_cached_key(|p| natural_key(&base_name(p)));
        for path in paths {
            self.walked += 1;
            if self.walked > MAX_WALK_ENTRIES {
                bail!("the folder holds too many files; pick the folder that holds the sequences");
            }
            if path.is_dir() {
                let base = base_name(&path);
                if !base.starts_with('.') && !base.eq_ignore_ascii_case("__MACOSX") {
                    self.add_dir(&path, depth + 1)?;
                }
            } else {
                self.add_path(&path, false)?;
            }
        }
        Ok(())
    }

    fn add_zip(&mut self, path: &Path) -> Result<()> {
        let file = std::fs::File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
        let mut archive = zip::ZipArchive::new(file).context("the file is not a valid zip archive, or it is corrupt")?;
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
            let size = entry.size();
            let read = || {
                let mut bytes = Vec::with_capacity(size.min(MAX_ENTRY_BYTES) as usize);
                entry.read_to_end(&mut bytes).map(|_| bytes)
            };
            self.add_file(base, size, read, " from the zip")?;
        }
        Ok(())
    }

    /// Order the samples and analyze them. With `strict`, a single FASTA file that
    /// cannot be parsed is an error.
    fn finish(self, settings: &Settings, source: &str, strict: bool) -> Result<Imported> {
        let Collector { mut ab1, mut fastas, mut skipped, .. } = self;
        // Traces in name order, then FASTA files in name order with their records in file order.
        ab1.sort_by_key(|item| natural_key(&item.id_source()));
        fastas.sort_by(|a, b| natural_key(&a.0).cmp(&natural_key(&b.0)));
        let mut items = ab1;
        for (file, bytes) in fastas {
            if let Err(reason) = add_fasta(&file, &bytes, &mut items, &mut skipped) {
                if strict {
                    bail!(reason);
                }
                skipped.push(SkippedFile { file_name: file, reason });
            }
        }
        build(items, skipped, settings, source)
    }
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
        let why = match skipped.as_slice() {
            [one] => format!(" ({}: {})", one.file_name, one.reason),
            _ => String::new(),
        };
        bail!("no readable .ab1 traces or FASTA sequences were found in {source}{why}");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_a_selection() {
        let dir = std::env::temp_dir();
        assert_eq!(project_name(&[dir.join("COI_20_sequences.zip")]), "COI_20_sequences");
        assert_eq!(project_name(&[dir.join("run7").join("a.ab1"), dir.join("run7").join("b.ab1")]), "run7");
        assert_eq!(project_name(&[]), "Batch");
        assert_eq!(describe(&[dir.join("a.ab1"), dir.join("b.ab1"), dir.join("c.ab1")]), "a.ab1 (+2 more)");
    }
}
