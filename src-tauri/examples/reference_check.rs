//! Definition of done #2: the screenshot query is VN248-2 bases 43-680 (638 bp, the
//! Geneious trim). Submit exactly that and look for PX112589.1.
//! Usage: cargo run --example reference_check -- <zip>

use coi_blast_lib::blast::{self, Client, SearchStatus};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let imported = coi_blast_lib::importer::import_zip(args[1].as_ref(), &Default::default())?;
    let read = imported.reads.iter().find(|r| r.id == "VN248-2").expect("VN248-2");
    let query = &read.bases[42..680];
    assert!(query.starts_with("GTCGGAACTTCACTAAGAG"), "not the screenshot query");
    println!("query length {}", query.len());
    let settings = coi_blast_lib::model::Settings::default();
    let client = Client::new()?;
    let (rid, _) = client.put(&format!(">VN248-2_ref bases 43-680\n{query}\n"), &settings.blast, "").await?;
    println!("RID {rid}");
    loop {
        tokio::time::sleep(blast::POLL_INTERVAL).await;
        match client.status(&rid).await? {
            SearchStatus::Waiting => continue,
            SearchStatus::Ready { .. } => break,
            other => anyhow::bail!("search ended: {other:?}"),
        }
    }
    let json = client.fetch_json(&rid).await?;
    let results = blast::parse_results(&json)?;
    let q = &results[0];
    println!("query_len {} hits {}", q.query_len, q.hits.len());
    for h in q.hits.iter().take(3) {
        println!("  #{} {} {} ident {:.2} cover {:.0} max {:.0}", h.rank, h.accession, h.sciname, h.per_ident, h.query_cover, h.max_score);
    }
    match q.hits.iter().find(|h| h.accession == "PX112589.1") {
        Some(h) => println!(
            "PX112589.1 found: rank {} max score {:.0} total {:.0} cover {:.0}% evalue {} ident {:.2}% acc len {}",
            h.rank, h.max_score, h.total_score, h.query_cover, h.evalue, h.per_ident, h.acc_len
        ),
        None => println!("PX112589.1 NOT in the hit list"),
    }
    Ok(())
}
