//! BLAST+ on this computer: `blastn` against a local database, writing NCBI's
//! single-file JSON (`-outfmt 15`), so the results are stored and read exactly as
//! results fetched from NCBI. No per-IP limit applies, which suits large batches.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use anyhow::{Context, Result, bail};

use crate::model::BlastSettings;

/// Extensions a nucleotide BLAST database (or its alias file) has.
const DB_EXTS: &[&str] = &["nal", "nsq", "nin", "nhr", "ndb", "njs"];

/// The `blastn` program: the setting (a file, or the folder holding it), else `blastn` on the PATH.
pub fn blastn(s: &BlastSettings) -> PathBuf {
    let p = s.blastn_path.trim().trim_matches('"');
    if p.is_empty() {
        return PathBuf::from("blastn");
    }
    let path = PathBuf::from(p);
    if path.is_dir() {
        let exe = if cfg!(windows) { "blastn.exe" } else { "blastn" };
        return path.join(exe);
    }
    path
}

/// The database's folder and name. BLAST+ reads spaces in `-db` as a list of
/// databases, so `blastn` runs inside the folder and gets only the name.
pub fn database(s: &BlastSettings) -> Result<(PathBuf, String)> {
    let raw = s.local_db.trim().trim_matches('"');
    if raw.is_empty() {
        bail!("choose a local BLAST database in Settings");
    }
    let mut path = PathBuf::from(raw);
    // A database file was picked (e.g. coi.nsq or coi.00.nsq): use its name.
    if path.extension().is_some_and(|e| DB_EXTS.contains(&e.to_string_lossy().to_ascii_lowercase().as_str())) {
        let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let stem = match stem.rsplit_once('.') {
            Some((base, vol)) if vol.len() == 2 && vol.chars().all(|c| c.is_ascii_digit()) => base.to_string(),
            _ => stem,
        };
        path.set_file_name(stem);
    }
    let dir = path.parent().filter(|p| !p.as_os_str().is_empty()).map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    if name.is_empty() {
        bail!("the local BLAST database has no name");
    }
    if name.contains(' ') {
        bail!("BLAST+ cannot read a database name with spaces (\"{name}\"); rename the database files");
    }
    let found = std::fs::read_dir(&dir)
        .with_context(|| format!("cannot open the database folder {}", dir.display()))?
        .filter_map(|e| e.ok())
        .any(|e| {
            let f = e.file_name().to_string_lossy().to_string();
            f.strip_prefix(&name).and_then(|rest| rest.strip_prefix('.')).is_some_and(|rest| {
                let ext = rest.rsplit('.').next().unwrap_or(rest).to_ascii_lowercase();
                DB_EXTS.contains(&ext.as_str())
            })
        });
    if !found {
        bail!("no BLAST database named \"{name}\" in {} (make one with makeblastdb -dbtype nucl)", dir.display());
    }
    Ok((dir, name))
}

pub fn args(s: &BlastSettings, db: &str, query: &Path, out: &Path) -> Vec<OsString> {
    let threads = std::thread::available_parallelism().map_or(2, |n| n.get()).to_string();
    let task = if s.megablast { "megablast" } else { "blastn" };
    let hits = s.max_hits.max(1).to_string();
    [
        "-task", task, "-db", db, "-outfmt", "15", "-max_target_seqs", &hits, "-num_threads", &threads,
    ]
    .iter()
    .map(OsString::from)
    .chain([OsString::from("-query"), query.as_os_str().to_owned(), OsString::from("-out"), out.as_os_str().to_owned()])
    .collect()
}

fn command(program: &Path) -> Command {
    let mut c = Command::new(program);
    c.stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}

/// `blastn -version` and the database check, for the Settings "Test" button.
pub fn check(s: &BlastSettings) -> Result<String> {
    let exe = blastn(s);
    let out = command(&exe)
        .arg("-version")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("cannot run {} (install BLAST+ or set its path)", exe.display()))?;
    let version = String::from_utf8_lossy(&out.stdout).lines().next().unwrap_or_default().trim().to_string();
    if !out.status.success() || !version.starts_with("blastn") {
        bail!("{} is not BLAST+ blastn", exe.display());
    }
    let (dir, name) = database(s)?;
    Ok(format!("{version}; database \"{name}\" in {}", dir.display()))
}

/// A running `blastn`, with its scratch folder.
pub struct Job {
    pub child: Child,
    dir: PathBuf,
}

impl Job {
    /// Start `blastn` on `fasta`. `tag` names the scratch folder.
    pub fn start(s: &BlastSettings, fasta: &str, tag: &str) -> Result<Self> {
        let (db_dir, db_name) = database(s)?;
        let dir = std::env::temp_dir().join(format!("coi-blast-{}-{tag}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let query = dir.join("query.fasta");
        std::fs::write(&query, fasta)?;
        let err = std::fs::File::create(dir.join("stderr.txt"))?;
        let exe = blastn(s);
        let child = command(&exe)
            .args(args(s, &db_name, &query, &dir.join("out.json")))
            .current_dir(&db_dir)
            // Taxonomy files (taxdb) next to the database give species names.
            .env("BLASTDB", &db_dir)
            .stdout(Stdio::null())
            .stderr(err)
            .spawn()
            .with_context(|| format!("cannot run {} (install BLAST+ or set its path in Settings)", exe.display()))?;
        Ok(Self { child, dir })
    }

    /// The results once `blastn` exited, or its error message.
    pub fn finish(&self, status: std::process::ExitStatus) -> Result<String> {
        if !status.success() {
            let err = std::fs::read_to_string(self.dir.join("stderr.txt")).unwrap_or_default();
            let msg = err.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" ");
            bail!("BLAST+ stopped with an error: {}", if msg.is_empty() { status.to_string() } else { msg });
        }
        std::fs::read_to_string(self.dir.join("out.json")).context("BLAST+ wrote no results")
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_database_by_name_or_file() {
        let dir = std::env::temp_dir().join(format!("coi-db-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("my refs")).unwrap();
        for f in ["coi.ndb", "coi.nsq", "big.00.nsq", "big.nal"] {
            std::fs::write(dir.join("my refs").join(f), b"").unwrap();
        }
        let mut s = BlastSettings::default();
        for (input, name) in [("coi", "coi"), ("coi.nsq", "coi"), ("big.00.nsq", "big"), ("big", "big")] {
            s.local_db = dir.join("my refs").join(input).to_string_lossy().into();
            let (d, n) = database(&s).unwrap();
            assert_eq!((d, n.as_str()), (dir.join("my refs"), name));
        }
        s.local_db = dir.join("my refs").join("other").to_string_lossy().into();
        assert!(database(&s).unwrap_err().to_string().contains("no BLAST database"));
        s.local_db = String::new();
        assert!(database(&s).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn passes_the_settings_to_blastn() {
        let s = BlastSettings { megablast: false, ..Default::default() };
        let a: Vec<String> = args(&s, "coi", Path::new("q.fa"), Path::new("o.json")).iter().map(|x| x.to_string_lossy().into()).collect();
        let at = |k: &str| a[a.iter().position(|x| x == k).unwrap() + 1].clone();
        assert_eq!((at("-task"), at("-db"), at("-outfmt"), at("-max_target_seqs")), ("blastn".into(), "coi".into(), "15".into(), "10".into()));
        assert_eq!((at("-query"), at("-out")), ("q.fa".into(), "o.json".into()));
    }
}
