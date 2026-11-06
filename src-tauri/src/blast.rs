//! NCBI BLAST URL API client (https://blast.ncbi.nlm.nih.gov/doc/blast-help/developerinfo.html)
//! and parser for its single-file JSON output (`FORMAT_TYPE=JSON2_S`).

use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::BlastSettings;

pub const BLAST_URL: &str = "https://blast.ncbi.nlm.nih.gov/Blast.cgi";
pub const TOOL: &str = "coi-barcode-blast";
/// NCBI: no more than one submission every 10 seconds.
pub const SUBMIT_INTERVAL: Duration = Duration::from_secs(10);
/// NCBI: poll a RID no more than once a minute.
pub const POLL_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq)]
pub enum SearchStatus {
    Waiting,
    Ready { hits: bool },
    Failed(String),
    /// NCBI no longer knows the RID (expired, or never existed).
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HitRow {
    pub rank: usize,
    pub accession: String,
    pub title: String,
    pub sciname: String,
    pub taxid: Option<u64>,
    pub max_score: f64,
    pub total_score: f64,
    pub query_cover: f64,
    pub evalue: f64,
    pub per_ident: f64,
    pub acc_len: usize,
    pub hsp_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    pub query_id: String,
    pub query_title: String,
    pub query_len: usize,
    pub program: String,
    pub database: String,
    pub message: Option<String>,
    pub hits: Vec<HitRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Hsp {
    pub bit_score: f64,
    pub score: f64,
    pub evalue: f64,
    pub identity: usize,
    pub align_len: usize,
    pub gaps: usize,
    pub query_from: usize,
    pub query_to: usize,
    pub hit_from: usize,
    pub hit_to: usize,
    pub query_strand: String,
    pub hit_strand: String,
    pub qseq: String,
    pub hseq: String,
    pub midline: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HitDetail {
    pub accession: String,
    pub title: String,
    pub sciname: String,
    pub length: usize,
    pub hsps: Vec<Hsp>,
}

/// Entrez query for the optional organism filter and exclusions, as the web form builds it.
pub fn entrez_query(s: &BlastSettings) -> Option<String> {
    let mut parts = Vec::new();
    let organism = s.organism.trim();
    if !organism.is_empty() {
        let term = organism.replace('"', "");
        if term.chars().all(|c| c.is_ascii_digit()) {
            parts.push(format!("txid{term}[Organism:exp]"));
        } else {
            parts.push(format!("\"{term}\"[Organism]"));
        }
    }
    if s.exclude_models {
        parts.push("NOT srcdb_refseq_model[PROP]".into());
    }
    if s.exclude_uncultured {
        parts.push("NOT (environmental samples[organism] OR metagenomes[orgn])".into());
    }
    if parts.is_empty() {
        return None;
    }
    let q = parts.join(" ");
    Some(if q.starts_with("NOT") { format!("all[filter] {q}") } else { q })
}

pub struct Client {
    http: reqwest::Client,
    base: String,
}

impl Client {
    /// NCBI's service, or the URL in `COI_BLAST_BASE_URL` (used to test against a mock server).
    pub fn new() -> Result<Self> {
        match std::env::var("COI_BLAST_BASE_URL") {
            Ok(url) if !url.is_empty() => Self::with_base(&url),
            _ => Self::with_base(BLAST_URL),
        }
    }

    pub fn with_base(base: &str) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("coi-barcode-blast/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(300))
            .connect_timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self { http, base: base.to_string() })
    }

    /// Submit a search. Returns the RID and NCBI's estimated seconds to completion.
    pub async fn put(&self, fasta: &str, s: &BlastSettings, email: &str) -> Result<(String, u64)> {
        let hits = s.max_hits.to_string();
        let mut form: Vec<(&str, &str)> = vec![
            ("CMD", "Put"),
            ("PROGRAM", &s.program),
            ("DATABASE", &s.database),
            ("QUERY", fasta),
            ("HITLIST_SIZE", &hits),
            ("DESCRIPTIONS", &hits),
            ("ALIGNMENTS", &hits),
            ("tool", TOOL),
        ];
        if s.megablast {
            form.push(("MEGABLAST", "on"));
        }
        let entrez = entrez_query(s);
        if let Some(q) = &entrez {
            form.push(("ENTREZ_QUERY", q));
        }
        if !email.trim().is_empty() {
            form.push(("email", email.trim()));
        }
        let resp = self.http.post(&self.base).form(&form).send().await.context("cannot reach NCBI")?;
        let code = resp.status();
        let body = resp.text().await?;
        if !code.is_success() {
            bail!("NCBI answered {code}: {}", error_text(&body).unwrap_or_default());
        }
        parse_put(&body)
    }

    pub async fn status(&self, rid: &str) -> Result<SearchStatus> {
        let resp = self
            .http
            .get(&self.base)
            .query(&[("CMD", "Get"), ("FORMAT_OBJECT", "SearchInfo"), ("RID", rid), ("tool", TOOL)])
            .send()
            .await
            .context("cannot reach NCBI")?;
        let code = resp.status();
        let body = resp.text().await?;
        if !code.is_success() {
            bail!("NCBI answered {code}");
        }
        Ok(parse_status(&body))
    }

    pub async fn fetch_json(&self, rid: &str) -> Result<String> {
        let resp = self
            .http
            .get(&self.base)
            .query(&[("CMD", "Get"), ("FORMAT_TYPE", "JSON2_S"), ("RID", rid), ("tool", TOOL)])
            .send()
            .await
            .context("cannot reach NCBI")?;
        let code = resp.status();
        let body = resp.text().await?;
        if !code.is_success() {
            bail!("NCBI answered {code}");
        }
        if !body.trim_start().starts_with('{') {
            if parse_status(&body) == SearchStatus::Unknown {
                bail!("RID_EXPIRED");
            }
            bail!("NCBI did not return results: {}", error_text(&body).unwrap_or_else(|| "unexpected answer".into()));
        }
        Ok(body)
    }
}

/// Value of `key` from any `QBlastInfoBegin ... QBlastInfoEnd` block.
fn qblast_value<'a>(body: &'a str, key: &str) -> Option<&'a str> {
    body.split("QBlastInfoBegin").skip(1).find_map(|block| {
        let block = block.split("QBlastInfoEnd").next().unwrap_or(block);
        block.lines().find_map(|line| {
            let (k, v) = line.split_once('=')?;
            (k.trim() == key).then(|| v.trim())
        })
    })
}

/// Short human-readable error from an NCBI HTML page.
fn error_text(body: &str) -> Option<String> {
    let markers = ["class=\"error\"", "Message ID#", "Error:"];
    let pos = markers.iter().find_map(|m| body.find(m))?;
    let snippet = &body[pos..body.len().min(pos + 400)];
    let mut text = String::new();
    let mut in_tag = false;
    for c in snippet.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => text.push(c),
            _ => {}
        }
    }
    let text = text.trim_start_matches("class=\"error\"").split_whitespace().collect::<Vec<_>>().join(" ");
    (!text.is_empty()).then_some(text)
}

pub fn parse_put(body: &str) -> Result<(String, u64)> {
    let rid = qblast_value(body, "RID").filter(|r| !r.is_empty());
    match rid {
        Some(rid) => {
            let rtoe = qblast_value(body, "RTOE").and_then(|v| v.parse().ok()).unwrap_or(30);
            Ok((rid.to_string(), rtoe))
        }
        None => Err(anyhow!(
            "NCBI did not accept the search: {}",
            error_text(body).unwrap_or_else(|| "no Request ID in the answer".into())
        )),
    }
}

pub fn parse_status(body: &str) -> SearchStatus {
    match qblast_value(body, "Status") {
        Some("WAITING") => SearchStatus::Waiting,
        Some("READY") => SearchStatus::Ready { hits: qblast_value(body, "ThereAreHits") == Some("yes") },
        Some("FAILED") => {
            SearchStatus::Failed(error_text(body).unwrap_or_else(|| "NCBI reports that the search failed".into()))
        }
        _ => SearchStatus::Unknown,
    }
}

fn reports(json: &Value) -> Vec<&Value> {
    match json.get("BlastOutput2") {
        Some(Value::Array(items)) => items.iter().filter_map(|i| i.get("report")).collect(),
        Some(obj @ Value::Object(_)) => obj.get("report").into_iter().collect(),
        _ => Vec::new(),
    }
}

fn f(v: &Value, k: &str) -> f64 {
    v.get(k).and_then(Value::as_f64).unwrap_or(0.0)
}

fn u(v: &Value, k: &str) -> usize {
    v.get(k).and_then(Value::as_u64).unwrap_or(0) as usize
}

fn s(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or_default().to_string()
}

/// Accession with version, e.g. "PX112589.1" from id "gb|PX112589.1|".
fn accession(desc: &Value) -> String {
    let id = s(desc, "id");
    id.split('|').find(|p| p.contains('.') && p.chars().next().is_some_and(|c| c.is_ascii_alphabetic()))
        .map(str::to_string)
        .unwrap_or_else(|| s(desc, "accession"))
}

fn hit_row(rank: usize, hit: &Value, query_len: usize) -> HitRow {
    let desc = hit.get("description").and_then(|d| d.get(0)).cloned().unwrap_or(Value::Null);
    let hsps: Vec<&Value> = hit.get("hsps").and_then(Value::as_array).map(|a| a.iter().collect()).unwrap_or_default();
    let best = hsps.iter().max_by(|a, b| f(a, "bit_score").total_cmp(&f(b, "bit_score")));
    // Query cover: share of the query covered by the union of all HSPs.
    let mut ranges: Vec<(usize, usize)> = hsps
        .iter()
        .map(|h| {
            let (a, b) = (u(h, "query_from"), u(h, "query_to"));
            (a.min(b), a.max(b))
        })
        .collect();
    ranges.sort();
    let mut covered = 0;
    let mut cur: Option<(usize, usize)> = None;
    for (a, b) in ranges {
        cur = match cur {
            Some((ca, cb)) if a <= cb + 1 => Some((ca, cb.max(b))),
            Some((ca, cb)) => {
                covered += cb - ca + 1;
                Some((a, b))
            }
            None => Some((a, b)),
        };
    }
    if let Some((ca, cb)) = cur {
        covered += cb - ca + 1;
    }
    HitRow {
        rank,
        accession: accession(&desc),
        title: s(&desc, "title"),
        sciname: s(&desc, "sciname"),
        taxid: desc.get("taxid").and_then(Value::as_u64),
        max_score: best.map_or(0.0, |h| f(h, "bit_score")),
        total_score: hsps.iter().map(|h| f(h, "bit_score")).sum(),
        query_cover: if query_len > 0 { (covered as f64 * 100.0 / query_len as f64).min(100.0) } else { 0.0 },
        evalue: hsps.iter().map(|h| f(h, "evalue")).fold(f64::INFINITY, f64::min).min(f64::MAX),
        per_ident: best.map_or(0.0, |h| {
            let len = u(h, "align_len");
            if len > 0 { u(h, "identity") as f64 * 100.0 / len as f64 } else { 0.0 }
        }),
        acc_len: u(hit, "len"),
        hsp_count: hsps.len(),
    }
}

/// Parse a JSON2_S response into one result per query, in submission order.
pub fn parse_results(json: &str) -> Result<Vec<QueryResult>> {
    let v: Value = serde_json::from_str(json).context("NCBI returned results that are not valid JSON")?;
    let reports = reports(&v);
    if reports.is_empty() {
        bail!("NCBI results contain no reports");
    }
    Ok(reports
        .into_iter()
        .map(|r| {
            let search = &r["results"]["search"];
            let query_len = u(search, "query_len");
            let hits = search
                .get("hits")
                .and_then(Value::as_array)
                .map(|hits| hits.iter().enumerate().map(|(i, h)| hit_row(i + 1, h, query_len)).collect())
                .unwrap_or_default();
            QueryResult {
                query_id: s(search, "query_id"),
                query_title: s(search, "query_title"),
                query_len,
                program: s(r, "program"),
                database: s(&r["search_target"], "db"),
                message: search.get("message").and_then(Value::as_str).map(str::to_string),
                hits,
            }
        })
        .collect())
}

/// Full alignment detail of one hit (1-based rank) of one query (0-based index).
pub fn hit_detail(json: &str, query_index: usize, rank: usize) -> Result<HitDetail> {
    let v: Value = serde_json::from_str(json)?;
    let report = *reports(&v).get(query_index).context("query not found in results")?;
    let hit = report["results"]["search"]["hits"].get(rank.wrapping_sub(1)).context("hit not found in results")?;
    let desc = hit.get("description").and_then(|d| d.get(0)).cloned().unwrap_or(Value::Null);
    let hsps = hit
        .get("hsps")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|h| Hsp {
                    bit_score: f(h, "bit_score"),
                    score: f(h, "score"),
                    evalue: f(h, "evalue"),
                    identity: u(h, "identity"),
                    align_len: u(h, "align_len"),
                    gaps: u(h, "gaps"),
                    query_from: u(h, "query_from"),
                    query_to: u(h, "query_to"),
                    hit_from: u(h, "hit_from"),
                    hit_to: u(h, "hit_to"),
                    query_strand: s(h, "query_strand"),
                    hit_strand: s(h, "hit_strand"),
                    qseq: s(h, "qseq"),
                    hseq: s(h, "hseq"),
                    midline: s(h, "midline"),
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(HitDetail {
        accession: accession(&desc),
        title: s(&desc, "title"),
        sciname: s(&desc, "sciname"),
        length: u(hit, "len"),
        hsps,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_put_answer() {
        let body = "<html><!--QBlastInfoBegin\n    RID = EVH1JEK3016\n    RTOE = 21\nQBlastInfoEnd\n-->";
        assert_eq!(parse_put(body).unwrap(), ("EVH1JEK3016".into(), 21));
        assert!(parse_put("<p class=\"error\">Query contains no data</p>").is_err());
    }

    #[test]
    fn parses_status() {
        let ready = "QBlastInfoBegin\n\tStatus=READY\nQBlastInfoEnd\nQBlastInfoBegin\n\tThereAreHits=yes\nQBlastInfoEnd";
        assert_eq!(parse_status(ready), SearchStatus::Ready { hits: true });
        assert_eq!(parse_status("QBlastInfoBegin\n\tStatus=WAITING\nQBlastInfoEnd"), SearchStatus::Waiting);
        assert_eq!(parse_status("QBlastInfoBegin\n\tStatus=UNKNOWN\nQBlastInfoEnd"), SearchStatus::Unknown);
    }

    #[test]
    fn builds_entrez_query() {
        let mut s = BlastSettings::default();
        assert_eq!(entrez_query(&s), None);
        s.organism = "Aedes".into();
        s.exclude_models = true;
        assert_eq!(entrez_query(&s).unwrap(), "\"Aedes\"[Organism] NOT srcdb_refseq_model[PROP]");
    }
}
