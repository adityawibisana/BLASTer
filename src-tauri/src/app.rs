//! Tauri commands, the background BLAST runner and the auto-saver.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Notify;

use crate::analysis;
use crate::blast::{self, Client, SearchStatus};
use crate::model::*;
use crate::primers::{self, Dir};
use crate::store::{self, ProjectHandle};
use crate::views;

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

pub struct AppState {
    pub project: Arc<Mutex<Option<ProjectHandle>>>,
    pub running: AtomicBool,
    pub cancel: AtomicBool,
    pub save_signal: Notify,
    pub config_dir: PathBuf,
    pub documents_dir: PathBuf,
}

impl AppState {
    fn lock(&self) -> std::sync::MutexGuard<'_, Option<ProjectHandle>> {
        self.project.lock().unwrap_or_else(|p| p.into_inner())
    }
}

/// Run `f` on the open project, then schedule a save.
fn mutate<T>(state: &AppState, f: impl FnOnce(&mut ProjectHandle) -> anyhow::Result<T>) -> CmdResult<T> {
    let out = {
        let mut guard = state.lock();
        let h = guard.as_mut().ok_or("no project is open")?;
        f(h).map_err(err)?
    };
    state.save_signal.notify_one();
    Ok(out)
}

fn read_only<T>(state: &AppState, f: impl FnOnce(&ProjectHandle) -> anyhow::Result<T>) -> CmdResult<T> {
    let guard = state.lock();
    let h = guard.as_ref().ok_or("no project is open")?;
    f(h).map_err(err)
}

fn changed(app: &AppHandle, reason: &str) {
    let _ = app.emit("project-changed", reason);
}

// ---------- saving ----------

/// Save the open project now. Pending archive entries are cleared only once they are on disk.
pub fn save_now(state: &AppState) -> anyhow::Result<()> {
    let snapshot = {
        let guard = state.lock();
        guard.as_ref().map(|h| (h.path.clone(), h.project.clone(), h.pending.clone()))
    };
    let Some((path, project, pending)) = snapshot else { return Ok(()) };
    store::write(Some(&path), &path, &project, &pending)?;
    let saved: Vec<String> = pending.into_iter().map(|(n, _)| n).collect();
    let mut guard = state.lock();
    if let Some(h) = guard.as_mut().filter(|h| h.path == path) {
        h.pending.retain(|(n, _)| !saved.contains(n));
    }
    Ok(())
}

pub async fn saver_loop(app: AppHandle) {
    let state = app.state::<AppState>();
    loop {
        state.save_signal.notified().await;
        tokio::time::sleep(Duration::from_millis(600)).await;
        let app2 = app.clone();
        let result = tokio::task::spawn_blocking(move || save_now(&app2.state::<AppState>())).await;
        match result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                let _ = app.emit("save-error", e.to_string());
            }
            Err(e) => {
                let _ = app.emit("save-error", e.to_string());
            }
        }
    }
}

// ---------- defaults and recent projects ----------

fn defaults_path(state: &AppState) -> PathBuf {
    state.config_dir.join("defaults.json")
}

fn load_defaults(state: &AppState) -> Settings {
    std::fs::read_to_string(defaults_path(state)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentProject {
    pub path: String,
    pub name: String,
    pub opened_at: String,
    #[serde(default)]
    pub exists: bool,
}

fn recent_path(state: &AppState) -> PathBuf {
    state.config_dir.join("recent.json")
}

fn load_recent(state: &AppState) -> Vec<RecentProject> {
    std::fs::read_to_string(recent_path(state)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn remember(state: &AppState, path: &Path, name: &str) {
    let mut list = load_recent(state);
    let p = path.display().to_string();
    list.retain(|r| !r.path.eq_ignore_ascii_case(&p));
    list.insert(0, RecentProject { path: p, name: name.into(), opened_at: chrono::Local::now().to_rfc3339(), exists: true });
    list.truncate(15);
    let _ = std::fs::create_dir_all(&state.config_dir);
    let _ = std::fs::write(recent_path(state), serde_json::to_string_pretty(&list).unwrap_or_default());
}

#[tauri::command]
pub fn get_defaults(state: State<'_, AppState>) -> Settings {
    load_defaults(&state)
}

#[tauri::command]
pub fn save_defaults(state: State<'_, AppState>, settings: Settings) -> CmdResult<()> {
    std::fs::create_dir_all(&state.config_dir).map_err(err)?;
    std::fs::write(defaults_path(&state), serde_json::to_string_pretty(&settings).map_err(err)?).map_err(err)
}

#[tauri::command]
pub fn recent_projects(state: State<'_, AppState>) -> Vec<RecentProject> {
    load_recent(&state).into_iter().map(|mut r| {
        r.exists = Path::new(&r.path).exists();
        r
    }).collect()
}

#[tauri::command]
pub fn forget_recent(state: State<'_, AppState>, path: String) {
    let mut list = load_recent(&state);
    list.retain(|r| r.path != path);
    let _ = std::fs::write(recent_path(&state), serde_json::to_string_pretty(&list).unwrap_or_default());
}

#[tauri::command]
pub fn primer_library() -> Vec<primers::PrimerSet> {
    primers::builtin_library()
}

// ---------- projects ----------

fn view_json(state: &AppState) -> CmdResult<Value> {
    let running = state.running.load(Ordering::SeqCst);
    read_only(state, |h| Ok(serde_json::to_value(views::project_view(h, running))?))
}

#[tauri::command]
pub fn get_project(state: State<'_, AppState>) -> CmdResult<Option<Value>> {
    if state.lock().is_none() {
        return Ok(None);
    }
    view_json(&state).map(Some)
}

fn has_unfinished(h: &ProjectHandle) -> bool {
    h.project.blast.groups.iter().any(|g| matches!(g.status, GroupStatus::Pending | GroupStatus::Submitted))
}

fn install(app: &AppHandle, handle: ProjectHandle) -> CmdResult<Value> {
    let state = app.state::<AppState>();
    if state.running.load(Ordering::SeqCst) {
        return Err("a BLAST search is still running for the open project; cancel it first".into());
    }
    let _ = save_now(&state);
    remember(&state, &handle.path, &handle.project.name);
    let resume = has_unfinished(&handle);
    *state.lock() = Some(handle);
    if resume {
        spawn_runner(app.clone());
    }
    view_json(&state)
}

#[tauri::command]
pub async fn create_project(app: AppHandle, zip_path: String) -> CmdResult<Value> {
    let state = app.state::<AppState>();
    let settings = load_defaults(&state);
    let docs = state.documents_dir.join("COI Barcode BLAST");
    let zip = PathBuf::from(&zip_path);
    let handle = tokio::task::spawn_blocking(move || -> anyhow::Result<ProjectHandle> {
        let imported = crate::importer::import_zip(&zip, &settings)?;
        let name = zip.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Batch".into());
        std::fs::create_dir_all(&docs)?;
        let path = store::unique_path(&docs, &name);
        let mut project = Project {
            format: PROJECT_FORMAT,
            name,
            created_at: chrono::Local::now().to_rfc3339(),
            source_zip: zip.display().to_string(),
            settings,
            reads: imported.reads,
            skipped: imported.skipped,
            blast: BlastState::default(),
            compiled: None,
            pair_suggestions: Vec::new(),
            rejected_pairs: Vec::new(),
        };
        crate::pairs::suggest(&mut project);
        store::create(&path, project, imported.files)
    })
    .await
    .map_err(err)?
    .map_err(err)?;
    install(&app, handle)
}

#[tauri::command]
pub async fn open_project(app: AppHandle, path: String) -> CmdResult<Value> {
    let p = PathBuf::from(path);
    let handle = tokio::task::spawn_blocking(move || store::open(&p)).await.map_err(err)?.map_err(err)?;
    install(&app, handle)
}

#[tauri::command]
pub async fn close_project(app: AppHandle) -> CmdResult<()> {
    let state = app.state::<AppState>();
    if state.running.load(Ordering::SeqCst) {
        return Err("a BLAST search is running; cancel it first (it can be resumed later)".into());
    }
    let app2 = app.clone();
    tokio::task::spawn_blocking(move || save_now(&app2.state::<AppState>())).await.map_err(err)?.map_err(err)?;
    *state.lock() = None;
    Ok(())
}

#[tauri::command]
pub async fn save_project_as(app: AppHandle, path: String) -> CmdResult<Value> {
    let state = app.state::<AppState>();
    let dest = PathBuf::from(&path);
    let snapshot = read_only(&state, |h| Ok((h.path.clone(), h.project.clone(), h.pending.clone())))?;
    let dest2 = dest.clone();
    tokio::task::spawn_blocking(move || store::write(Some(&snapshot.0), &dest2, &snapshot.1, &snapshot.2))
        .await
        .map_err(err)?
        .map_err(err)?;
    {
        let mut guard = state.lock();
        if let Some(h) = guard.as_mut() {
            h.path = dest.clone();
            h.pending.clear();
            remember(&state, &dest, &h.project.name);
        }
    }
    view_json(&state)
}

// ---------- reads ----------

fn find_read<'a>(h: &'a mut ProjectHandle, id: &str) -> anyhow::Result<&'a mut Read> {
    h.project.reads.iter_mut().find(|r| r.id == id).ok_or_else(|| anyhow::anyhow!("sample {id} not found"))
}

#[tauri::command]
pub async fn get_trace(state: State<'_, AppState>, id: String) -> CmdResult<views::TraceView> {
    let path = read_only(&state, |h| Ok(h.path.clone()))?;
    let bytes = store::read_entry(&path, &store::ab1_entry(&id)).map_err(err)?;
    let t = crate::abif::parse(&bytes).map_err(err)?;
    Ok(views::TraceView { bases: String::from_utf8_lossy(&t.bases).into_owned(), quals: t.quals, peaks: t.peaks, traces: t.traces })
}

#[tauri::command]
pub fn preview_trim(state: State<'_, AppState>, id: String, start: usize, end: usize) -> CmdResult<Derived> {
    read_only(&state, |h| {
        let read = h.project.reads.iter().find(|r| r.id == id).ok_or_else(|| anyhow::anyhow!("sample not found"))?;
        Ok(analysis::preview(read, &h.project.settings, Range { start: start.min(end), end: end.max(start) }))
    })
}

#[tauri::command]
pub fn set_trim(app: AppHandle, state: State<'_, AppState>, id: String, start: usize, end: usize) -> CmdResult<()> {
    mutate(&state, |h| {
        let r = find_read(h, &id)?;
        let range = Range { start: start.min(end).min(r.raw_length), end: end.max(start).min(r.raw_length) };
        r.manual_trim = (range != r.auto_trim).then_some(range);
        crate::pairs::refresh_read(&mut h.project, &id);
        Ok(())
    })?;
    changed(&app, "read");
    Ok(())
}

#[tauri::command]
pub fn reset_read(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<()> {
    mutate(&state, |h| {
        let r = find_read(h, &id)?;
        r.manual_trim = None;
        if r.pair.is_none() && r.paired_with.is_none() {
            r.direction_manual = None;
        }
        crate::pairs::refresh_read(&mut h.project, &id);
        crate::pairs::suggest(&mut h.project);
        Ok(())
    })?;
    changed(&app, "read");
    Ok(())
}

#[tauri::command]
pub fn set_direction(app: AppHandle, state: State<'_, AppState>, id: String, direction: Option<Dir>) -> CmdResult<()> {
    mutate(&state, |h| {
        let r = find_read(h, &id)?;
        if r.pair.is_some() || r.paired_with.is_some() {
            anyhow::bail!("{id} is in a pair; use Swap F/R or Unpair to change its direction");
        }
        r.direction_manual = direction.filter(|d| Some(*d) != r.direction_auto);
        crate::pairs::refresh_read(&mut h.project, &id);
        crate::pairs::suggest(&mut h.project);
        Ok(())
    })?;
    changed(&app, "read");
    Ok(())
}

#[tauri::command]
pub fn set_inclusion(
    app: AppHandle,
    state: State<'_, AppState>,
    ids: Vec<String>,
    include_failed: Option<bool>,
    excluded: Option<bool>,
) -> CmdResult<()> {
    mutate(&state, |h| {
        for id in &ids {
            let r = find_read(h, id)?;
            if let Some(v) = include_failed {
                r.include_failed = v;
            }
            if let Some(v) = excluded {
                r.excluded = v;
            }
            crate::pairs::refresh_read(&mut h.project, id);
        }
        Ok(())
    })?;
    changed(&app, "read");
    Ok(())
}

#[tauri::command]
pub async fn update_settings(app: AppHandle, settings: Settings) -> CmdResult<Value> {
    let state = app.state::<AppState>();
    mutate(&state, |h| {
        let old = std::mem::replace(&mut h.project.settings, settings.clone());
        let mut analysis_old = old.clone();
        let mut analysis_new = settings.clone();
        // Only these fields change the analysis; the rest just needs a refresh.
        for s in [&mut analysis_old, &mut analysis_new] {
            s.email.clear();
            s.language.clear();
            s.blast = BlastSettings::default();
            s.compile_min_cover = 0.0;
            s.low_identity = 0.0;
            s.fr_pattern.clear();
        }
        let sets = analysis::primer_sets(&settings);
        for r in &mut h.project.reads {
            if analysis_old != analysis_new {
                analysis::analyze(r, &settings, &sets);
            } else {
                analysis::refresh(r, &settings);
            }
        }
        crate::pairs::refresh_all_pairs(&mut h.project);
        crate::pairs::suggest(&mut h.project);
        Ok(())
    })?;
    changed(&app, "settings");
    view_json(&state)
}

// ---------- forward/reverse pairs ----------

#[tauri::command]
pub fn confirm_pair(app: AppHandle, state: State<'_, AppState>, forward_id: String, reverse_id: String) -> CmdResult<()> {
    mutate(&state, |h| crate::pairs::confirm(&mut h.project, &forward_id, &reverse_id))?;
    changed(&app, "pairs");
    Ok(())
}

#[tauri::command]
pub fn reject_pair(app: AppHandle, state: State<'_, AppState>, forward_id: String, reverse_id: String) -> CmdResult<()> {
    mutate(&state, |h| {
        crate::pairs::reject(&mut h.project, &forward_id, &reverse_id);
        Ok(())
    })?;
    changed(&app, "pairs");
    Ok(())
}

#[tauri::command]
pub fn unpair(app: AppHandle, state: State<'_, AppState>, forward_id: String) -> CmdResult<()> {
    mutate(&state, |h| {
        crate::pairs::unpair(&mut h.project, &forward_id);
        Ok(())
    })?;
    changed(&app, "pairs");
    Ok(())
}

#[tauri::command]
pub fn swap_pair(app: AppHandle, state: State<'_, AppState>, forward_id: String) -> CmdResult<()> {
    mutate(&state, |h| crate::pairs::swap(&mut h.project, &forward_id))?;
    changed(&app, "pairs");
    Ok(())
}

#[tauri::command]
pub fn set_pair_use_only(app: AppHandle, state: State<'_, AppState>, forward_id: String, use_only: Option<Dir>) -> CmdResult<()> {
    mutate(&state, |h| {
        crate::pairs::set_use_only(&mut h.project, &forward_id, use_only);
        Ok(())
    })?;
    changed(&app, "pairs");
    Ok(())
}

// ---------- results ----------

#[tauri::command]
pub fn get_sample_result(state: State<'_, AppState>, id: String) -> CmdResult<Option<Value>> {
    read_only(&state, |h| {
        let read = h.project.reads.iter().find(|r| r.id == id).ok_or_else(|| anyhow::anyhow!("sample not found"))?;
        Ok(views::sample_result(h, read).map(serde_json::to_value).transpose()?)
    })
}

#[tauri::command]
pub async fn get_hit_detail(state: State<'_, AppState>, id: String, rank: usize) -> CmdResult<blast::HitDetail> {
    let (path, entry, pending, index) = read_only(&state, |h| {
        let read = h.project.reads.iter().find(|r| r.id == id).ok_or_else(|| anyhow::anyhow!("sample not found"))?;
        let rref = read.result.as_ref().ok_or_else(|| anyhow::anyhow!("sample has no results"))?;
        let name = store::raw_entry(&rref.group_id);
        let pending = h.pending.iter().find(|(n, _)| *n == name).map(|(_, b)| b.clone());
        Ok((h.path.clone(), name, pending, rref.index))
    })?;
    tokio::task::spawn_blocking(move || -> anyhow::Result<blast::HitDetail> {
        let bytes = match pending {
            Some(b) => b,
            None => store::read_entry(&path, &entry)?,
        };
        blast::hit_detail(std::str::from_utf8(&bytes)?, index, rank)
    })
    .await
    .map_err(err)?
    .map_err(err)
}

#[tauri::command]
pub fn compile_results(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Value> {
    mutate(&state, |h| {
        let compiled = {
            let hr: &ProjectHandle = h;
            crate::compile::compile(
                &hr.project.reads,
                |r| views::result_for_compile(hr, r),
                |r| views::sample_flags(hr, r),
                hr.project.settings.compile_min_cover,
            )
        };
        h.project.compiled = Some(compiled);
        Ok(())
    })?;
    changed(&app, "compiled");
    view_json(&state)
}

// ---------- export ----------

#[tauri::command]
pub async fn export_excel(app: AppHandle, path: String) -> CmdResult<()> {
    let state = app.state::<AppState>();
    let _guard_check = read_only(&state, |_| Ok(()))?;
    let app2 = app.clone();
    tokio::task::spawn_blocking(move || {
        let state = app2.state::<AppState>();
        let guard = state.lock();
        let h = guard.as_ref().ok_or_else(|| anyhow::anyhow!("no project is open"))?;
        crate::export::excel(h, Path::new(&path))
    })
    .await
    .map_err(err)?
    .map_err(err)
}

#[tauri::command]
pub async fn export_pdf(app: AppHandle, path: String, full: bool) -> CmdResult<usize> {
    let app2 = app.clone();
    tokio::task::spawn_blocking(move || {
        let state = app2.state::<AppState>();
        // Build the markup under the lock, typeset outside it.
        let markup = {
            let guard = state.lock();
            let h = guard.as_ref().ok_or_else(|| anyhow::anyhow!("no project is open"))?;
            crate::pdf::report_markup(h, &crate::pdf::Options { full })?
        };
        let bytes = crate::pdf::render(markup)?;
        std::fs::write(&path, &bytes)?;
        Ok::<usize, anyhow::Error>(bytes.len())
    })
    .await
    .map_err(err)?
    .map_err(err)
}

#[tauri::command]
pub fn export_fasta(state: State<'_, AppState>, path: String, only_eligible: bool) -> CmdResult<usize> {
    read_only(&state, |h| crate::export::fasta(h, Path::new(&path), only_eligible))
}

#[tauri::command]
pub fn export_raw(state: State<'_, AppState>, path: String) -> CmdResult<usize> {
    read_only(&state, |h| crate::export::raw_results(h, Path::new(&path)))
}

// ---------- BLAST ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueResult {
    pub queued_samples: usize,
    pub groups: usize,
}

/// Queue samples for BLAST. `only_changed`: samples with no current result (never
/// searched, outdated, or whose search failed or expired).
#[tauri::command]
pub fn start_blast(app: AppHandle, state: State<'_, AppState>, only_changed: bool) -> CmdResult<QueueResult> {
    let result = mutate(&state, |h| {
        let s = &h.project.settings;
        if s.email.trim().is_empty() {
            anyhow::bail!("NO_EMAIL");
        }
        let queued = views::queued_ids(&h.project).into_iter().map(String::from).collect::<std::collections::HashSet<_>>();
        let selected: Vec<&Read> = h
            .project
            .reads
            .iter()
            .filter(|r| r.derived.eligible && !queued.contains(&r.id))
            .filter(|r| !only_changed || h.result_for(r).is_none() || h.outdated(r))
            .collect();
        let size = s.blast.group_size.clamp(1, 100);
        let mut next = h.project.blast.groups.len() + 1;
        let mut new_groups = Vec::new();
        for chunk in selected.chunks(size) {
            new_groups.push(BlastGroup {
                id: format!("g{next:04}"),
                sample_ids: chunk.iter().map(|r| r.id.clone()).collect(),
                fingerprints: chunk.iter().map(|r| r.derived.fingerprint.clone()).collect(),
                fasta: crate::fasta::multi(&h.project.reads, chunk.iter().copied()),
                status: GroupStatus::Pending,
                rid: None,
                rtoe_secs: None,
                submitted_at: None,
                finished_at: None,
                attempts: 0,
                error: None,
            });
            next += 1;
        }
        let out = QueueResult { queued_samples: selected.len(), groups: new_groups.len() };
        if !new_groups.is_empty() {
            h.project.blast.settings_used = Some(s.blast.clone());
            h.project.blast.groups.extend(new_groups);
        }
        Ok(out)
    })?;
    if result.groups > 0 || has_unfinished_now(&state) {
        spawn_runner(app.clone());
    }
    changed(&app, "blast");
    Ok(result)
}

fn has_unfinished_now(state: &AppState) -> bool {
    state.lock().as_ref().is_some_and(has_unfinished)
}

#[tauri::command]
pub fn cancel_blast(app: AppHandle, state: State<'_, AppState>) {
    state.cancel.store(true, Ordering::SeqCst);
    changed(&app, "blast");
}

pub fn spawn_runner(app: AppHandle) {
    let state = app.state::<AppState>();
    if state.running.swap(true, Ordering::SeqCst) {
        return;
    }
    state.cancel.store(false, Ordering::SeqCst);
    tauri::async_runtime::spawn(async move {
        run_queue(&app).await;
        let state = app.state::<AppState>();
        state.running.store(false, Ordering::SeqCst);
        state.save_signal.notify_one();
        changed(&app, "blast");
    });
}

fn update_group(app: &AppHandle, id: &str, f: impl FnOnce(&mut BlastGroup)) {
    let state = app.state::<AppState>();
    {
        let mut guard = state.lock();
        if let Some(g) = guard.as_mut().and_then(|h| h.project.blast.groups.iter_mut().find(|g| g.id == id)) {
            f(g);
        }
    }
    state.save_signal.notify_one();
    changed(app, "blast");
}

/// Sleep in one-second steps so a cancel takes effect quickly. Returns false if cancelled.
async fn sleep_unless_cancelled(app: &AppHandle, d: Duration) -> bool {
    let state = app.state::<AppState>();
    let end = Instant::now() + d;
    while Instant::now() < end {
        if state.cancel.load(Ordering::SeqCst) {
            return false;
        }
        tokio::time::sleep(Duration::from_secs(1).min(end - Instant::now())).await;
    }
    !state.cancel.load(Ordering::SeqCst)
}

/// Retry a network call with increasing delays (30 s, 60 s, 120 s, 240 s).
async fn with_retries<T, F, Fut>(app: &AppHandle, group: &str, mut call: F) -> Result<Option<T>, String>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<T>>,
{
    let mut delay = Duration::from_secs(30);
    for attempt in 0..5 {
        match call().await {
            Ok(v) => return Ok(Some(v)),
            Err(e) => {
                let msg = e.to_string();
                if msg == "RID_EXPIRED" || attempt == 4 {
                    return Err(msg);
                }
                update_group(app, group, |g| g.error = Some(format!("{msg} (retrying in {} s)", delay.as_secs())));
                if !sleep_unless_cancelled(app, delay).await {
                    return Ok(None);
                }
                delay *= 2;
            }
        }
    }
    unreachable!()
}

async fn run_queue(app: &AppHandle) {
    let state = app.state::<AppState>();
    let client = match Client::new() {
        Ok(c) => c,
        Err(e) => {
            let _ = app.emit("save-error", e.to_string());
            return;
        }
    };
    let mut last_submit: Option<Instant> = None;
    loop {
        if state.cancel.load(Ordering::SeqCst) {
            let mut guard = state.lock();
            if let Some(h) = guard.as_mut() {
                for g in &mut h.project.blast.groups {
                    if g.status == GroupStatus::Pending {
                        g.status = GroupStatus::Cancelled;
                    }
                }
            }
            break;
        }
        let next = {
            let guard = state.lock();
            guard.as_ref().and_then(|h| {
                let g = h
                    .project
                    .blast
                    .groups
                    .iter()
                    .find(|g| g.status == GroupStatus::Submitted)
                    .or_else(|| h.project.blast.groups.iter().find(|g| g.status == GroupStatus::Pending))?;
                let blast = h.project.blast.settings_used.clone().unwrap_or_else(|| h.project.settings.blast.clone());
                Some((g.clone(), blast, h.project.settings.email.clone()))
            })
        };
        let Some((group, blast_settings, email)) = next else { break };
        let gid = group.id.clone();

        // Submit (or resume a group that already has a RID).
        let (rid, first_wait) = match (&group.status, &group.rid) {
            (GroupStatus::Submitted, Some(rid)) => {
                let since = group
                    .submitted_at
                    .as_deref()
                    .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
                    .map(|t| (chrono::Local::now().fixed_offset() - t).num_seconds().max(0) as u64)
                    .unwrap_or(0);
                (rid.clone(), Duration::from_secs(60u64.saturating_sub(since).max(5)))
            }
            _ => {
                if let Some(t) = last_submit {
                    let wait = blast::SUBMIT_INTERVAL.saturating_sub(t.elapsed());
                    if !sleep_unless_cancelled(app, wait).await {
                        continue;
                    }
                }
                let fasta = group.fasta.clone();
                let put = with_retries(app, &gid, || client.put(&fasta, &blast_settings, &email)).await;
                last_submit = Some(Instant::now());
                match put {
                    Ok(Some((rid, rtoe))) => {
                        let r = rid.clone();
                        update_group(app, &gid, |g| {
                            g.status = GroupStatus::Submitted;
                            g.rid = Some(r);
                            g.rtoe_secs = Some(rtoe);
                            g.submitted_at = Some(chrono::Local::now().to_rfc3339());
                            g.attempts += 1;
                            g.error = None;
                        });
                        (rid, blast::POLL_INTERVAL.max(Duration::from_secs(rtoe)))
                    }
                    Ok(None) => continue,
                    Err(msg) => {
                        update_group(app, &gid, |g| {
                            g.status = GroupStatus::Failed;
                            g.error = Some(msg);
                        });
                        continue;
                    }
                }
            }
        };

        // Poll no more than once a minute.
        if !sleep_unless_cancelled(app, first_wait).await {
            continue;
        }
        let outcome = loop {
            match with_retries(app, &gid, || client.status(&rid)).await {
                Ok(Some(SearchStatus::Waiting)) => {
                    if !sleep_unless_cancelled(app, blast::POLL_INTERVAL).await {
                        break None;
                    }
                }
                Ok(Some(SearchStatus::Ready { .. })) => break Some(Ok(())),
                Ok(Some(SearchStatus::Failed(msg))) => break Some(Err((GroupStatus::Failed, msg))),
                Ok(Some(SearchStatus::Unknown)) => {
                    break Some(Err((GroupStatus::Expired, "NCBI no longer has this search (expired)".into())));
                }
                Ok(None) => break None,
                Err(msg) => break Some(Err((GroupStatus::Failed, msg))),
            }
        };
        match outcome {
            None => continue, // cancelled; the group stays Submitted and can resume later
            Some(Err((status, msg))) => {
                update_group(app, &gid, |g| {
                    g.status = status;
                    g.error = Some(msg);
                });
                continue;
            }
            Some(Ok(())) => {}
        }

        let fetched = with_retries(app, &gid, || client.fetch_json(&rid)).await;
        match fetched {
            Ok(Some(json)) => {
                if let Err(e) = store_results(app, &group, json) {
                    update_group(app, &gid, |g| {
                        g.status = GroupStatus::Failed;
                        g.error = Some(e.to_string());
                    });
                }
            }
            Ok(None) => continue,
            Err(msg) => {
                let status = if msg == "RID_EXPIRED" { GroupStatus::Expired } else { GroupStatus::Failed };
                update_group(app, &gid, |g| {
                    g.status = status;
                    g.error = Some(if msg == "RID_EXPIRED" { "NCBI no longer has this search (expired)".into() } else { msg });
                });
            }
        }
    }
}

/// Parse a group's results, attach them to its samples and queue both entries for saving.
fn store_results(app: &AppHandle, group: &BlastGroup, json: String) -> anyhow::Result<()> {
    let parsed = blast::parse_results(&json)?;
    let state = app.state::<AppState>();
    {
        let mut guard = state.lock();
        let h = guard.as_mut().ok_or_else(|| anyhow::anyhow!("project was closed"))?;
        store::attach_results(h, &group.id, parsed, json);
    }
    state.save_signal.notify_one();
    changed(app, "blast");
    Ok(())
}
