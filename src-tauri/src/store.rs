//! `.coiproj` project files: a zip archive holding
//! - `project.json`: settings, reads, per-read decisions, BLAST groups, compiled results
//! - `ab1/<sample>.ab1`: the original trace files
//! - `blast/<group>.raw.json`: NCBI's raw response per group (never rewritten)
//! - `blast/<group>.parsed.json`: the parsed hit tables per group
//!
//! Saving writes a new archive next to the old one and renames it into place.
//! Unchanged entries are copied without recompressing, so saves stay fast.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use zip::write::SimpleFileOptions;

use crate::blast::QueryResult;
use crate::model::{Project, PROJECT_FORMAT, Read};

pub const PROJECT_JSON: &str = "project.json";

pub fn ab1_entry(sample_id: &str) -> String {
    format!("ab1/{sample_id}.ab1")
}

pub fn raw_entry(group_id: &str) -> String {
    format!("blast/{group_id}.raw.json")
}

pub fn parsed_entry(group_id: &str) -> String {
    format!("blast/{group_id}.parsed.json")
}

pub struct ProjectHandle {
    pub path: PathBuf,
    pub project: Project,
    /// Parsed results per BLAST group, in submission order.
    pub results: HashMap<String, Vec<QueryResult>>,
    /// Archive entries not yet written to disk.
    pub pending: Vec<(String, Vec<u8>)>,
}

impl ProjectHandle {
    pub fn result_for(&self, read: &Read) -> Option<&QueryResult> {
        let r = read.result.as_ref()?;
        self.results.get(&r.group_id)?.get(r.index)
    }

    pub fn outdated(&self, read: &Read) -> bool {
        read.result.as_ref().is_some_and(|r| r.fingerprint != read.derived.fingerprint)
    }
}

fn options() -> SimpleFileOptions {
    SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).large_file(false)
}

/// Write `project` plus `pending` entries to `dest`, copying every other entry from
/// `source` (the current archive, if any) unchanged.
pub fn write(source: Option<&Path>, dest: &Path, project: &Project, pending: &[(String, Vec<u8>)]) -> Result<()> {
    let tmp = dest.with_extension("coiproj.saving");
    let result = (|| -> Result<()> {
        let mut writer = zip::ZipWriter::new(File::create(&tmp).context("cannot create the project file")?);
        writer.start_file(PROJECT_JSON, options())?;
        writer.write_all(&serde_json::to_vec(project)?)?;
        let pending_names: HashSet<&str> = pending.iter().map(|(n, _)| n.as_str()).collect();
        for (name, bytes) in pending {
            writer.start_file(name.as_str(), options())?;
            writer.write_all(bytes)?;
        }
        if let Some(src) = source.filter(|p| p.exists()) {
            let mut archive = zip::ZipArchive::new(File::open(src)?)?;
            for i in 0..archive.len() {
                let entry = archive.by_index_raw(i)?;
                let name = entry.name()?.to_string();
                if name == PROJECT_JSON || pending_names.contains(name.as_str()) {
                    continue;
                }
                writer.raw_copy_file(entry)?;
            }
        }
        writer.finish()?.sync_all()?;
        Ok(())
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    std::fs::rename(&tmp, dest).context("cannot replace the project file")?;
    Ok(())
}

pub fn create(path: &Path, project: Project, files: Vec<(String, Vec<u8>)>) -> Result<ProjectHandle> {
    let entries: Vec<(String, Vec<u8>)> = files.into_iter().map(|(id, b)| (ab1_entry(&id), b)).collect();
    write(None, path, &project, &entries)?;
    Ok(ProjectHandle { path: path.to_path_buf(), project, results: HashMap::new(), pending: Vec::new() })
}

pub fn read_entry(path: &Path, name: &str) -> Result<Vec<u8>> {
    let mut archive = zip::ZipArchive::new(File::open(path).context("cannot open the project file")?)?;
    let mut entry = archive.by_name(name).with_context(|| format!("{name} is missing from the project"))?;
    let mut out = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut out)?;
    Ok(out)
}

pub fn open(path: &Path) -> Result<ProjectHandle> {
    let file = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let mut archive = zip::ZipArchive::new(file).context("the project file is damaged or not a .coiproj file")?;
    let project: Project = {
        let mut entry = archive.by_name(PROJECT_JSON).context("the project file has no project.json")?;
        let mut text = String::new();
        entry.read_to_string(&mut text)?;
        serde_json::from_str(&text).context("project.json cannot be read")?
    };
    if project.format > PROJECT_FORMAT {
        bail!("this project was saved by a newer version of the app");
    }
    let mut results = HashMap::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name()?.to_string();
        if let Some(gid) = name.strip_prefix("blast/").and_then(|n| n.strip_suffix(".parsed.json")) {
            let mut text = String::new();
            entry.read_to_string(&mut text)?;
            let parsed: Vec<QueryResult> = serde_json::from_str(&text).with_context(|| format!("{name} is damaged"))?;
            results.insert(gid.to_string(), parsed);
        }
    }
    Ok(ProjectHandle { path: path.to_path_buf(), project, results, pending: Vec::new() })
}

/// Attach a finished group's results to its samples and queue the raw and parsed
/// responses for saving. Queries are matched to samples by the first word of their
/// title (the sample ID), falling back to submission order.
pub fn attach_results(h: &mut ProjectHandle, group_id: &str, parsed: Vec<QueryResult>, raw_json: String) {
    let Some(group) = h.project.blast.groups.iter().find(|g| g.id == group_id).cloned() else { return };
    for (i, q) in parsed.iter().enumerate() {
        let first = q.query_title.split_whitespace().next().unwrap_or_default();
        let pos = group.sample_ids.iter().position(|id| id == first).unwrap_or(i);
        let (Some(sample), Some(fp)) = (group.sample_ids.get(pos), group.fingerprints.get(pos)) else { continue };
        if let Some(r) = h.project.reads.iter_mut().find(|r| &r.id == sample) {
            r.result = Some(crate::model::ResultRef { group_id: group.id.clone(), index: i, fingerprint: fp.clone() });
        }
    }
    h.pending.push((parsed_entry(group_id), serde_json::to_vec(&parsed).unwrap_or_default()));
    h.pending.push((raw_entry(group_id), raw_json.into_bytes()));
    h.results.insert(group_id.to_string(), parsed);
    if let Some(g) = h.project.blast.groups.iter_mut().find(|g| g.id == group_id) {
        g.status = crate::model::GroupStatus::Ready;
        g.finished_at = Some(chrono::Local::now().to_rfc3339());
        g.error = None;
    }
}

/// A file name that does not exist yet: `name.coiproj`, `name (2).coiproj`, ...
pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let clean: String = name.chars().map(|c| if r#"<>:"/\|?*"#.contains(c) { '_' } else { c }).collect();
    let mut n = 1;
    loop {
        let candidate =
            if n == 1 { dir.join(format!("{clean}.coiproj")) } else { dir.join(format!("{clean} ({n}).coiproj")) };
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}
