//! BLAST results files the user brings in, and attaching them to the project.
//!
//! Accepted: what the NCBI website's *Download All* offers (single-file JSON, XML,
//! single-file XML2, or the multi-file JSON zip) and the single-file JSON that BLAST+
//! writes (`-outfmt 15`). Each is turned into NCBI's single-file JSON (`JSON2_S`),
//! the form the app stores per group and reads hit tables and alignments from.

use std::collections::HashSet;
use std::io::Read as _;

use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::{Value, json};

use crate::model::{BlastGroup, GroupSource, GroupStatus};
use crate::store::{self, ProjectHandle};

/// Reports of a results file, each a JSON2 `report` object, in file order.
pub fn reports(bytes: &[u8]) -> Result<Vec<Value>> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    if bytes.starts_with(b"PK") {
        return zip_reports(bytes);
    }
    let text = std::str::from_utf8(bytes).context("the file is not a text BLAST results file")?;
    let trimmed = text.trim_start();
    let out = if trimmed.starts_with('{') {
        json_reports(&serde_json::from_str(trimmed).context("the file is not valid JSON")?)?
    } else if trimmed.starts_with('<') {
        xml_reports(trimmed)?
    } else {
        bail!("this is not a BLAST results file (expected JSON or XML)");
    };
    if out.is_empty() {
        bail!("the file holds no BLAST results");
    }
    Ok(out)
}

/// The JSON2_S document for `reports`, as stored in `blast/<group>.raw.json`.
pub fn document(reports: &[Value]) -> String {
    let items: Vec<Value> = reports.iter().map(|r| json!({ "report": r })).collect();
    serde_json::to_string(&json!({ "BlastOutput2": items })).unwrap_or_default()
}

fn json_reports(v: &Value) -> Result<Vec<Value>> {
    match v.get("BlastOutput2") {
        Some(Value::Array(items)) => Ok(items.iter().filter_map(|i| i.get("report").cloned()).collect()),
        Some(obj @ Value::Object(_)) => Ok(obj.get("report").cloned().into_iter().collect()),
        _ if v.get("BlastJSON").is_some() => bail!(
            "this is the index file of a multi-file JSON download; import the whole zip, or download \"Single-file JSON\" instead"
        ),
        _ => bail!("this JSON file does not hold BLAST results"),
    }
}

/// NCBI's multi-file JSON download: one JSON2 file per query, plus an index file.
fn zip_reports(bytes: &[u8]) -> Result<Vec<Value>> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).context("the zip file cannot be read")?;
    let mut out = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        if !entry.name()?.to_ascii_lowercase().ends_with(".json") {
            continue;
        }
        let mut text = String::new();
        if entry.read_to_string(&mut text).is_err() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) {
            if v.get("BlastOutput2").is_some() {
                out.extend(json_reports(&v)?);
            }
        }
    }
    if out.is_empty() {
        bail!("the zip file holds no BLAST JSON results");
    }
    Ok(out)
}

fn xml_reports(text: &str) -> Result<Vec<Value>> {
    let mut out = Vec::new();
    // Some tools write one XML document per query, one after another.
    let docs: Vec<&str> = if text.matches("<?xml").count() > 1 {
        text.split("<?xml").filter(|d| !d.trim().is_empty()).map(|d| d.trim()).collect()
    } else {
        vec![text]
    };
    for d in docs {
        let owned;
        let src = if d.starts_with("version") {
            owned = format!("<?xml {d}");
            owned.as_str()
        } else {
            d
        };
        let opts = roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() };
        let doc = roxmltree::Document::parse_with_options(src, opts).context("the XML file cannot be read")?;
        match doc.root_element().tag_name().name() {
            "BlastOutput" => out.extend(legacy_reports(doc.root_element())),
            "BlastXML2" => out.extend(xml2_reports(doc.root_element())),
            other => bail!("this XML file does not hold BLAST results (<{other}>)"),
        }
    }
    Ok(out)
}

type Node<'a, 'i> = roxmltree::Node<'a, 'i>;

fn child<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == name)
}

fn children<'a, 'i>(n: Node<'a, 'i>, name: &'static str) -> impl Iterator<Item = Node<'a, 'i>> {
    n.children().filter(move |c| c.is_element() && c.tag_name().name() == name)
}

/// Text at a path of child names, e.g. `["search-target", "Target", "db"]`.
fn text<'a>(n: Node<'a, '_>, path: &[&str]) -> &'a str {
    let mut cur = Some(n);
    for p in path {
        cur = cur.and_then(|c| child(c, p));
    }
    cur.and_then(|c| c.text()).map(str::trim).unwrap_or_default()
}

fn int(s: &str) -> Value {
    s.parse::<i64>().map(Value::from).unwrap_or(Value::from(0))
}

fn float(s: &str) -> Value {
    s.parse::<f64>().ok().and_then(|f| serde_json::Number::from_f64(f).map(Value::Number)).unwrap_or(Value::from(0))
}

/// Legacy BLAST XML (`<BlastOutput>`): one `Iteration` per query.
fn legacy_reports(root: Node) -> Vec<Value> {
    let program = text(root, &["BlastOutput_program"]);
    let db = text(root, &["BlastOutput_db"]);
    let Some(iterations) = child(root, "BlastOutput_iterations") else { return Vec::new() };
    children(iterations, "Iteration")
        .map(|it| {
            let hits: Vec<Value> = child(it, "Iteration_hits")
                .map(|hs| {
                    children(hs, "Hit")
                        .map(|h| {
                            // Hit_def lists every identical sequence, joined by " >".
                            let def = text(h, &["Hit_def"]);
                            let title = def.split(" >").next().unwrap_or(def);
                            let hsps: Vec<Value> = child(h, "Hit_hsps")
                                .map(|x| {
                                    children(x, "Hsp")
                                        .map(|p| {
                                            let strand = |k: &str| if text(p, &[k]).starts_with('-') { "Minus" } else { "Plus" };
                                            json!({
                                                "num": int(text(p, &["Hsp_num"])),
                                                "bit_score": float(text(p, &["Hsp_bit-score"])),
                                                "score": float(text(p, &["Hsp_score"])),
                                                "evalue": float(text(p, &["Hsp_evalue"])),
                                                "identity": int(text(p, &["Hsp_identity"])),
                                                "query_from": int(text(p, &["Hsp_query-from"])),
                                                "query_to": int(text(p, &["Hsp_query-to"])),
                                                "query_strand": strand("Hsp_query-frame"),
                                                "hit_from": int(text(p, &["Hsp_hit-from"])),
                                                "hit_to": int(text(p, &["Hsp_hit-to"])),
                                                "hit_strand": strand("Hsp_hit-frame"),
                                                "align_len": int(text(p, &["Hsp_align-len"])),
                                                "gaps": int(text(p, &["Hsp_gaps"])),
                                                "qseq": text(p, &["Hsp_qseq"]),
                                                "hseq": text(p, &["Hsp_hseq"]),
                                                "midline": text(p, &["Hsp_midline"]),
                                            })
                                        })
                                        .collect()
                                })
                                .unwrap_or_default();
                            json!({
                                "num": int(text(h, &["Hit_num"])),
                                "description": [{
                                    "id": text(h, &["Hit_id"]),
                                    "accession": text(h, &["Hit_accession"]),
                                    "title": title,
                                }],
                                "len": int(text(h, &["Hit_len"])),
                                "hsps": hsps,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            let mut search = json!({
                "query_id": text(it, &["Iteration_query-ID"]),
                "query_title": text(it, &["Iteration_query-def"]),
                "query_len": int(text(it, &["Iteration_query-len"])),
                "hits": hits,
            });
            let message = text(it, &["Iteration_message"]);
            if !message.is_empty() {
                search["message"] = message.into();
            }
            json!({ "program": program, "search_target": { "db": db }, "results": { "search": search } })
        })
        .collect()
}

/// BLAST XML2 (`<BlastXML2>`): the same structure as JSON2, with wrapper elements
/// (`Report`, `Search`, `Hit`, ...) and dashes in the names.
fn xml2_reports(root: Node) -> Vec<Value> {
    root.descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "Report")
        .map(|r| {
            let search = child(r, "results").and_then(|x| child(x, "Results")).and_then(|x| child(x, "search")).and_then(|x| child(x, "Search"));
            let Some(search) = search else {
                return json!({ "program": text(r, &["program"]), "results": { "search": {} } });
            };
            let hits: Vec<Value> = child(search, "hits")
                .map(|hs| {
                    children(hs, "Hit")
                        .map(|h| {
                            let description: Vec<Value> = child(h, "description")
                                .map(|d| {
                                    children(d, "HitDescr")
                                        .map(|x| {
                                            let mut o = json!({
                                                "id": text(x, &["id"]),
                                                "accession": text(x, &["accession"]),
                                                "title": text(x, &["title"]),
                                                "sciname": text(x, &["sciname"]),
                                            });
                                            if let Ok(t) = text(x, &["taxid"]).parse::<u64>() {
                                                o["taxid"] = t.into();
                                            }
                                            o
                                        })
                                        .collect()
                                })
                                .unwrap_or_default();
                            let hsps: Vec<Value> = child(h, "hsps")
                                .map(|x| {
                                    children(x, "Hsp")
                                        .map(|p| {
                                            json!({
                                                "num": int(text(p, &["num"])),
                                                "bit_score": float(text(p, &["bit-score"])),
                                                "score": float(text(p, &["score"])),
                                                "evalue": float(text(p, &["evalue"])),
                                                "identity": int(text(p, &["identity"])),
                                                "query_from": int(text(p, &["query-from"])),
                                                "query_to": int(text(p, &["query-to"])),
                                                "query_strand": text(p, &["query-strand"]),
                                                "hit_from": int(text(p, &["hit-from"])),
                                                "hit_to": int(text(p, &["hit-to"])),
                                                "hit_strand": text(p, &["hit-strand"]),
                                                "align_len": int(text(p, &["align-len"])),
                                                "gaps": int(text(p, &["gaps"])),
                                                "qseq": text(p, &["qseq"]),
                                                "hseq": text(p, &["hseq"]),
                                                "midline": text(p, &["midline"]),
                                            })
                                        })
                                        .collect()
                                })
                                .unwrap_or_default();
                            json!({ "num": int(text(h, &["num"])), "description": description, "len": int(text(h, &["len"])), "hsps": hsps })
                        })
                        .collect()
                })
                .unwrap_or_default();
            let mut s = json!({
                "query_id": text(search, &["query-id"]),
                "query_title": text(search, &["query-title"]),
                "query_len": int(text(search, &["query-len"])),
                "hits": hits,
            });
            let message = text(search, &["message"]);
            if !message.is_empty() {
                s["message"] = message.into();
            }
            json!({
                "program": text(r, &["program"]),
                "search_target": { "db": text(r, &["search-target", "Target", "db"]) },
                "results": { "search": s },
            })
        })
        .collect()
}

// ---------- attaching to the project ----------

#[derive(Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    /// Samples that got results from the file.
    pub samples: usize,
    /// Groups that now hold them.
    pub groups: Vec<String>,
    /// Query names in the file that match no sample of the project.
    pub unknown: Vec<String>,
    /// Samples whose sequence is not the one searched (changed since, or no longer eligible).
    pub changed: Vec<String>,
    /// Samples that already had current results; left as they are.
    pub already: Vec<String>,
    /// Eligible samples that still have no current results.
    pub still_missing: usize,
}

/// The sample ID of a report: the first word of the query title (the FASTA header).
fn sample_id(report: &Value) -> String {
    let search = &report["results"]["search"];
    let title = search["query_title"].as_str().unwrap_or_default();
    title.split_whitespace().next().unwrap_or_else(|| search["query_id"].as_str().unwrap_or_default()).to_string()
}

/// A group ID not used yet: one past the highest `gNNNN`.
pub fn next_group_id(groups: &[BlastGroup]) -> String {
    let max = groups.iter().filter_map(|g| g.id.strip_prefix('g')?.parse::<usize>().ok()).max().unwrap_or(0);
    format!("g{:04}", max.max(groups.len()) + 1)
}

/// Attach the results in `reports` to the project's samples.
///
/// Queries are matched to samples by ID. A group still waiting or not finished whose
/// samples are all in the file takes them; the rest go into one new group. Imported
/// samples leave unsent groups, so they are not searched again.
pub fn import_into(h: &mut ProjectHandle, reports: Vec<Value>) -> Result<ImportSummary> {
    let mut sum = ImportSummary::default();
    let mut found: Vec<(String, Value)> = Vec::new();
    let mut seen = HashSet::new();
    for r in reports {
        let id = sample_id(&r);
        if !seen.insert(id.clone()) {
            continue;
        }
        let Some(read) = h.project.reads.iter().find(|x| x.id == id) else {
            sum.unknown.push(id);
            continue;
        };
        let qlen = r["results"]["search"]["query_len"].as_u64().unwrap_or(0) as usize;
        if !read.derived.eligible || qlen != read.derived.final_seq.len() {
            sum.changed.push(id);
            continue;
        }
        if h.result_for(read).is_some() && !h.outdated(read) {
            sum.already.push(id);
            continue;
        }
        found.push((id, r));
    }
    if found.is_empty() && sum.unknown.len() + sum.changed.len() + sum.already.len() == 0 {
        bail!("the file holds no BLAST results");
    }

    let mut left: HashSet<String> = found.iter().map(|(id, _)| id.clone()).collect();
    let mut targets: Vec<String> = Vec::new();
    for g in &h.project.blast.groups {
        if g.status != GroupStatus::Ready && !g.sample_ids.is_empty() && g.sample_ids.iter().all(|s| left.contains(s)) {
            for s in &g.sample_ids {
                left.remove(s);
            }
            targets.push(g.id.clone());
        }
    }
    // Imported samples leave groups that have not been sent, or did not finish.
    let reads = h.project.reads.clone();
    for g in &mut h.project.blast.groups {
        let open = matches!(g.status, GroupStatus::Pending | GroupStatus::Failed | GroupStatus::Expired | GroupStatus::Cancelled);
        if !open || targets.contains(&g.id) || !g.sample_ids.iter().any(|s| left.contains(s)) {
            continue;
        }
        let keep: Vec<usize> = (0..g.sample_ids.len()).filter(|&i| !left.contains(&g.sample_ids[i])).collect();
        g.fingerprints = keep.iter().filter_map(|&i| g.fingerprints.get(i).cloned()).collect();
        g.sample_ids = keep.iter().map(|&i| g.sample_ids[i].clone()).collect();
        let kept: Vec<&crate::model::Read> = g.sample_ids.iter().filter_map(|id| reads.iter().find(|r| &r.id == id)).collect();
        g.fasta = crate::fasta::multi(&reads, kept);
    }
    h.project.blast.groups.retain(|g| !g.sample_ids.is_empty() || g.status == GroupStatus::Ready);
    if !left.is_empty() {
        let ids: Vec<String> = found.iter().map(|(id, _)| id.clone()).filter(|id| left.contains(id)).collect();
        let chosen: Vec<&crate::model::Read> = ids.iter().filter_map(|id| reads.iter().find(|r| &r.id == id)).collect();
        let id = next_group_id(&h.project.blast.groups);
        h.project.blast.groups.push(BlastGroup {
            id: id.clone(),
            fingerprints: chosen.iter().map(|r| r.derived.fingerprint.clone()).collect(),
            fasta: crate::fasta::multi(&reads, chosen.iter().copied()),
            sample_ids: ids,
            status: GroupStatus::Pending,
            rid: None,
            rtoe_secs: None,
            submitted_at: None,
            finished_at: None,
            attempts: 0,
            error: None,
            last_checked_at: None,
            last_check: None,
            source: GroupSource::File,
        });
        targets.push(id);
    }

    for gid in &targets {
        let Some(g) = h.project.blast.groups.iter_mut().find(|g| &g.id == gid) else { continue };
        // The file was searched with the sequences as they are now.
        g.fingerprints = g
            .sample_ids
            .iter()
            .map(|s| reads.iter().find(|r| &r.id == s).map(|r| r.derived.fingerprint.clone()).unwrap_or_default())
            .collect();
        g.source = GroupSource::File;
        let in_order: Vec<Value> =
            g.sample_ids.iter().filter_map(|s| found.iter().find(|(id, _)| id == s).map(|(_, r)| r.clone())).collect();
        let raw = document(&in_order);
        let parsed = crate::blast::parse_results(&raw)?;
        sum.samples += parsed.len();
        store::attach_results(h, gid, parsed, raw);
    }
    sum.groups = targets;
    sum.still_missing = h
        .project
        .reads
        .iter()
        .filter(|r| r.derived.eligible && (h.result_for(r).is_none() || h.outdated(r)))
        .count();
    Ok(sum)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEGACY: &str = r#"<?xml version="1.0"?>
<!DOCTYPE BlastOutput PUBLIC "-//NCBI//NCBI BlastOutput/EN" "http://www.ncbi.nlm.nih.gov/dtd/NCBI_BlastOutput.dtd">
<BlastOutput>
  <BlastOutput_program>blastn</BlastOutput_program>
  <BlastOutput_db>core_nt</BlastOutput_db>
  <BlastOutput_iterations>
    <Iteration>
      <Iteration_query-ID>Query_1</Iteration_query-ID>
      <Iteration_query-def>s1 bases 1-10 dir:F</Iteration_query-def>
      <Iteration_query-len>10</Iteration_query-len>
      <Iteration_hits>
        <Hit>
          <Hit_num>1</Hit_num>
          <Hit_id>gi|1|gb|AB123.1|</Hit_id>
          <Hit_def>Aedes aegypti COI &gt;gi|2|gb|AB124.1| Aedes aegypti COI</Hit_def>
          <Hit_accession>AB123</Hit_accession>
          <Hit_len>658</Hit_len>
          <Hit_hsps><Hsp>
            <Hsp_num>1</Hsp_num><Hsp_bit-score>18.5</Hsp_bit-score><Hsp_score>10</Hsp_score><Hsp_evalue>1.5e-3</Hsp_evalue>
            <Hsp_query-from>1</Hsp_query-from><Hsp_query-to>10</Hsp_query-to><Hsp_hit-from>30</Hsp_hit-from><Hsp_hit-to>21</Hsp_hit-to>
            <Hsp_query-frame>1</Hsp_query-frame><Hsp_hit-frame>-1</Hsp_hit-frame>
            <Hsp_identity>9</Hsp_identity><Hsp_gaps>0</Hsp_gaps><Hsp_align-len>10</Hsp_align-len>
            <Hsp_qseq>ACGTACGTAC</Hsp_qseq><Hsp_hseq>ACGTACGTAA</Hsp_hseq><Hsp_midline>||||||||| </Hsp_midline>
          </Hsp></Hit_hsps>
        </Hit>
      </Iteration_hits>
    </Iteration>
    <Iteration>
      <Iteration_query-ID>Query_2</Iteration_query-ID>
      <Iteration_query-def>s2</Iteration_query-def>
      <Iteration_query-len>12</Iteration_query-len>
      <Iteration_hits></Iteration_hits>
      <Iteration_message>No hits found</Iteration_message>
    </Iteration>
  </BlastOutput_iterations>
</BlastOutput>"#;

    #[test]
    fn reads_legacy_xml() {
        let r = reports(LEGACY.as_bytes()).unwrap();
        assert_eq!(r.len(), 2);
        let parsed = crate::blast::parse_results(&document(&r)).unwrap();
        assert_eq!(parsed[0].query_title, "s1 bases 1-10 dir:F");
        assert_eq!(parsed[0].database, "core_nt");
        let h = &parsed[0].hits[0];
        assert_eq!((h.accession.as_str(), h.title.as_str(), h.acc_len), ("AB123.1", "Aedes aegypti COI", 658));
        assert_eq!((h.per_ident, h.query_cover, h.evalue), (90.0, 100.0, 1.5e-3));
        assert!(parsed[1].hits.is_empty());
        assert_eq!(parsed[1].message.as_deref(), Some("No hits found"));
        let d = crate::blast::hit_detail(&document(&r), 0, 1).unwrap();
        assert_eq!((d.hsps[0].hit_strand.as_str(), d.hsps[0].hit_from), ("Minus", 30));
    }

    #[test]
    fn reads_json_and_rejects_other_files() {
        let one = r#"{"BlastOutput2":{"report":{"program":"blastn","results":{"search":{"query_title":"s1","query_len":5,"hits":[]}}}}}"#;
        assert_eq!(reports(one.as_bytes()).unwrap().len(), 1);
        assert!(reports(b"{\"BlastJSON\":[{\"File\":\"X_1.json\"}]}").unwrap_err().to_string().contains("zip"));
        assert!(reports(b">s1\nACGT\n").is_err());
        assert!(reports(b"<html></html>").is_err());
    }

    #[test]
    fn reads_multi_file_json_zip() {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut buf);
            let o = zip::write::SimpleFileOptions::default();
            use std::io::Write as _;
            w.start_file("X.json", o).unwrap();
            w.write_all(br#"{"BlastJSON":[{"File":"X_1.json"},{"File":"X_2.json"}]}"#).unwrap();
            for q in ["s1", "s2"] {
                w.start_file(format!("X_{q}.json"), o).unwrap();
                write!(w, r#"{{"BlastOutput2":{{"report":{{"results":{{"search":{{"query_title":"{q}","hits":[]}}}}}}}}}}"#).unwrap();
            }
            w.finish().unwrap();
        }
        let r = reports(buf.get_ref()).unwrap();
        assert_eq!(r.iter().map(sample_id).collect::<Vec<_>>(), ["s1", "s2"]);
    }

    #[test]
    fn next_id_skips_used_ids() {
        let g = |id: &str| BlastGroup {
            id: id.into(),
            sample_ids: vec!["a".into()],
            fingerprints: vec!["f".into()],
            fasta: String::new(),
            status: GroupStatus::Ready,
            rid: None,
            rtoe_secs: None,
            submitted_at: None,
            finished_at: None,
            attempts: 0,
            error: None,
            last_checked_at: None,
            last_check: None,
            source: GroupSource::Ncbi,
        };
        let groups = vec![g("g0002"), g("g0007")];
        assert_eq!(next_group_id(&groups), "g0008");
        assert_eq!(next_group_id(&[]), "g0001");
    }
}
