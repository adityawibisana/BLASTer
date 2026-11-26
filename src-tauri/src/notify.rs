//! Optional Telegram messages about the BLAST searches: when a group fails for good
//! (all tries used), when it times out, and when the whole batch has finished.
//!
//! The bot token is an app-wide setting kept in the app's config folder, never in a
//! project file. Messages carry only the project name and counts, no sample names
//! or sequences.

use std::path::Path;
use std::time::Duration;

use anyhow::{bail, Context};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::{BlastGroup, GroupStatus};

pub const FILE_NAME: &str = "notify.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct NotifySettings {
    pub enabled: bool,
    pub bot_token: String,
    pub chat_id: String,
}

impl NotifySettings {
    pub fn is_ready(&self) -> bool {
        self.enabled && !self.bot_token.trim().is_empty() && !self.chat_id.trim().is_empty()
    }
}

pub fn load(config_dir: &Path) -> NotifySettings {
    std::fs::read_to_string(config_dir.join(FILE_NAME)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save(config_dir: &Path, s: &NotifySettings) -> anyhow::Result<()> {
    std::fs::create_dir_all(config_dir)?;
    let clean = NotifySettings { enabled: s.enabled, bot_token: s.bot_token.trim().into(), chat_id: s.chat_id.trim().into() };
    std::fs::write(config_dir.join(FILE_NAME), serde_json::to_string_pretty(&clean)?)?;
    Ok(())
}

/// Why a group stopped, for its message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    Error,
    TimedOut,
}

/// The message for a group that has just failed, or None while it will be tried again.
pub fn group_message(project_name: &str, g: &BlastGroup, why: Stop) -> Option<String> {
    if !matches!(g.status, GroupStatus::Failed | GroupStatus::Expired) {
        return None; // back in the queue for another try
    }
    let n = g.sample_ids.len();
    let samples = format!("{n} sample{}", if n == 1 { "" } else { "s" });
    let tries = format!("{} tr{}", g.failures, if g.failures == 1 { "y" } else { "ies" });
    Some(match why {
        Stop::TimedOut => format!("⏱ {project_name}: group {} ({samples}) timed out after {tries}. Send it again from Processes.", g.id),
        Stop::Error => {
            let err = g.error.as_deref().unwrap_or("did not finish");
            format!("❌ {project_name}: group {} ({samples}) failed after {tries}: {}", g.id, short(err, 200))
        }
    })
}

/// The message once nothing is left to search, or None while groups are still waiting.
pub fn finished_message(project_name: &str, groups: &[BlastGroup]) -> Option<String> {
    if groups.is_empty() || groups.iter().any(|g| matches!(g.status, GroupStatus::Pending | GroupStatus::Submitted)) {
        return None;
    }
    let done = groups.iter().filter(|g| g.status == GroupStatus::Ready).count();
    let not_done = groups.len() - done;
    let samples = |pick: &dyn Fn(&BlastGroup) -> bool| {
        groups.iter().filter(|g| pick(g)).flat_map(|g| g.sample_ids.iter()).collect::<std::collections::HashSet<_>>().len()
    };
    let total = samples(&|_| true);
    let with_results = samples(&|g| g.status == GroupStatus::Ready);
    let icon = if not_done == 0 { "✅" } else { "⚠️" };
    let mut text = format!("{icon} {}: all searches finished. {done} of {} groups done", project_name, groups.len());
    if not_done > 0 {
        text += &format!(", {not_done} did not finish");
    }
    text += &format!(". {with_results} of {total} samples have results.");
    Some(text)
}

fn short(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.into();
    }
    format!("{}…", s.chars().take(max).collect::<String>())
}

fn client() -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder().timeout(Duration::from_secs(15)).build()?)
}

/// The Telegram error without the request URL, which holds the token.
fn clean(e: reqwest::Error) -> anyhow::Error {
    anyhow::anyhow!("cannot reach Telegram: {}", e.without_url())
}

fn url(token: &str, method: &str) -> String {
    format!("https://api.telegram.org/bot{}/{method}", token.trim())
}

async fn call(token: &str, method: &str, body: Value) -> anyhow::Result<Value> {
    if token.trim().is_empty() {
        bail!("enter the bot token first");
    }
    let resp = client()?.post(url(token, method)).json(&body).send().await.map_err(clean)?;
    let v: Value = resp.json().await.map_err(clean)?;
    if v.get("ok").and_then(Value::as_bool) != Some(true) {
        let desc = v.get("description").and_then(Value::as_str).unwrap_or("unknown error");
        bail!("Telegram: {desc}");
    }
    Ok(v.get("result").cloned().unwrap_or(Value::Null))
}

pub async fn send(token: &str, chat_id: &str, text: &str) -> anyhow::Result<()> {
    if chat_id.trim().is_empty() {
        bail!("find the chat first: send /start to the bot, then click Find chat");
    }
    call(token, "sendMessage", serde_json::json!({ "chat_id": chat_id.trim(), "text": text })).await?;
    Ok(())
}

/// Send `text` with the saved settings, in the background. Errors are only logged:
/// a message that cannot be sent never stops a search.
pub fn send_in_background(config_dir: &Path, text: String) {
    let s = load(config_dir);
    if !s.is_ready() {
        return;
    }
    tauri::async_runtime::spawn(async move {
        if let Err(e) = send(&s.bot_token, &s.chat_id, &text).await {
            eprintln!("telegram notification not sent: {e:#}");
        }
    });
}

/// The chat that most recently wrote to the bot (the user sends /start first).
pub async fn find_chat(token: &str) -> anyhow::Result<(String, String)> {
    let updates = call(token, "getUpdates", serde_json::json!({ "limit": 100 })).await?;
    latest_chat(&updates).context("no message found: open the bot in Telegram, send /start, then try again")
}

/// The newest chat in a getUpdates result, as (id, name).
pub fn latest_chat(updates: &Value) -> Option<(String, String)> {
    updates.as_array()?.iter().rev().find_map(|u| {
        let chat = ["message", "my_chat_member", "channel_post", "edited_message"].iter().find_map(|k| u.get(k)?.get("chat"))?;
        let id = chat.get("id")?;
        let id = id.as_i64().map(|n| n.to_string()).or_else(|| id.as_str().map(String::from))?;
        let name = ["title", "username", "first_name"].iter().find_map(|k| chat.get(k)?.as_str()).unwrap_or("").to_string();
        Some((id, name))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::MAX_TRIES;

    fn group(id: &str, status: GroupStatus, samples: usize) -> BlastGroup {
        serde_json::from_value(serde_json::json!({
            "id": id, "sampleIds": (0..samples).map(|i| format!("{id}-{i}")).collect::<Vec<_>>(),
            "fingerprints": [], "fasta": "", "status": status, "rid": null, "rtoeSecs": null,
            "submittedAt": null, "finishedAt": null, "attempts": 0, "error": null,
        }))
        .unwrap()
    }

    #[test]
    fn no_message_while_a_group_is_retried() {
        let mut g = group("3", GroupStatus::Submitted, 20);
        g.rid = Some("R".into());
        g.fail(GroupStatus::Failed, "boom".into(), true);
        assert_eq!(g.status, GroupStatus::Pending);
        assert!(group_message("P", &g, Stop::Error).is_none());
        for _ in 1..MAX_TRIES {
            g.status = GroupStatus::Submitted;
            g.fail(GroupStatus::Failed, "boom".into(), true);
        }
        assert_eq!(g.status, GroupStatus::Failed);
        let m = group_message("P", &g, Stop::Error).unwrap();
        assert!(m.contains("group 3 (20 samples) failed after 3 tries: boom"), "{m}");
    }

    #[test]
    fn timeout_message_has_no_samples() {
        let mut g = group("2", GroupStatus::Failed, 1);
        g.failures = 3;
        g.sample_ids = vec!["SECRET-SAMPLE".into()];
        let m = group_message("P", &g, Stop::TimedOut).unwrap();
        assert!(m.contains("timed out") && m.contains("1 sample)") && !m.contains("SECRET"), "{m}");
    }

    #[test]
    fn finished_only_when_nothing_is_left() {
        assert!(finished_message("Batch", &[]).is_none());
        let mut gs = vec![group("1", GroupStatus::Ready, 2), group("2", GroupStatus::Pending, 2)];
        assert!(finished_message("Batch", &gs).is_none());
        gs[1].status = GroupStatus::Failed;
        let m = finished_message("Batch", &gs).unwrap();
        assert!(m.contains("Batch: all searches finished. 1 of 2 groups done, 1 did not finish. 2 of 4 samples"), "{m}");
        gs[1].status = GroupStatus::Ready;
        assert!(finished_message("Batch", &gs).unwrap().starts_with("✅"));
    }

    #[test]
    fn newest_chat_is_found() {
        let v = serde_json::json!([
            { "message": { "chat": { "id": 1, "first_name": "Old" } } },
            { "message": { "chat": { "id": -100, "title": "Lab" } } },
        ]);
        assert_eq!(latest_chat(&v), Some(("-100".into(), "Lab".into())));
        assert_eq!(latest_chat(&serde_json::json!([])), None);
    }

    #[test]
    fn settings_round_trip_and_default() {
        let dir = std::env::temp_dir().join(format!("notify-test-{}", std::process::id()));
        assert_eq!(load(&dir), NotifySettings::default());
        let s = NotifySettings { enabled: true, bot_token: " 1:abc ".into(), chat_id: "42".into() };
        save(&dir, &s).unwrap();
        assert_eq!(load(&dir).bot_token, "1:abc");
        assert!(load(&dir).is_ready());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
