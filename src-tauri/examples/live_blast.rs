//! Manual check against the real NCBI service: imports the sample zip, submits the
//! sequences that pass QC as one search, waits within NCBI's limits, saves the raw
//! JSON and prints the parsed top hits.
//! Usage: cargo run --example live_blast -- <zip> <out.json>

use coi_blast_lib::blast::{self, Client, SearchStatus};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let settings = coi_blast_lib::model::Settings::default();
    let imported = coi_blast_lib::importer::import_zip(args[1].as_ref(), &settings)?;
    let reads: Vec<_> = imported.reads.iter().filter(|r| r.derived.eligible).collect();
    let fasta = coi_blast_lib::fasta::multi(reads.iter().copied());
    println!("submitting {} sequences", reads.len());
    let client = Client::new()?;
    let (rid, rtoe) = client.put(&fasta, &settings.blast, "").await?;
    println!("RID {rid}, estimated {rtoe}s");
    tokio::time::sleep(blast::POLL_INTERVAL.max(std::time::Duration::from_secs(rtoe))).await;
    loop {
        match client.status(&rid).await? {
            SearchStatus::Waiting => {
                println!("waiting");
                tokio::time::sleep(blast::POLL_INTERVAL).await;
            }
            SearchStatus::Ready { hits } => {
                println!("ready, hits: {hits}");
                break;
            }
            other => anyhow::bail!("search ended: {other:?}"),
        }
    }
    let json = client.fetch_json(&rid).await?;
    std::fs::write(&args[2], &json)?;
    for q in blast::parse_results(&json)? {
        let top = q.hits.first();
        println!(
            "{:<45} len={} hits={} top={:?}",
            q.query_title.chars().take(45).collect::<String>(),
            q.query_len,
            q.hits.len(),
            top.map(|h| (&h.accession, &h.sciname, h.per_ident, h.query_cover, h.max_score))
        );
    }
    Ok(())
}
