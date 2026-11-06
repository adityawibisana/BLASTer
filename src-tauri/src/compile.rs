//! "Compile results": closest species per sample, and species counts for the batch chart.

use std::collections::HashMap;

use crate::blast::{HitRow, QueryResult};
use crate::model::{Compiled, CompiledRow, Flag, Read, SpeciesCount};

/// Species name of a hit: NCBI's scientific name, or the first two words of the title.
pub fn species_of(hit: &HitRow) -> String {
    if !hit.sciname.trim().is_empty() {
        return hit.sciname.trim().to_string();
    }
    hit.title.split_whitespace().take(2).collect::<Vec<_>>().join(" ")
}

fn rank_key(h: &HitRow) -> (i64, i64, i64) {
    // Rounded as NCBI displays them, so equal-looking values tie.
    ((h.per_ident * 100.0).round() as i64, (h.query_cover * 100.0).round() as i64, (h.max_score * 10.0).round() as i64)
}

pub struct Closest<'a> {
    pub best: &'a HitRow,
    pub runner_up: Option<&'a HitRow>,
    pub counted: usize,
}

/// Highest % identity among hits covering at least `min_cover` % of the query.
/// Exact ties: higher query cover, then higher Max Score, then the species backed
/// by more hit records, then NCBI's order. Always a single answer.
pub fn closest(hits: &[HitRow], min_cover: f64) -> Option<Closest<'_>> {
    let mut counted: Vec<&HitRow> = hits.iter().filter(|h| h.query_cover + 1e-9 >= min_cover).collect();
    if counted.is_empty() {
        return None;
    }
    let mut records: HashMap<String, usize> = HashMap::new();
    for h in &counted {
        *records.entry(species_of(h)).or_default() += 1;
    }
    counted.sort_by(|a, b| {
        rank_key(b)
            .cmp(&rank_key(a))
            .then_with(|| records[&species_of(b)].cmp(&records[&species_of(a)]))
            .then_with(|| a.rank.cmp(&b.rank))
    });
    let best = counted[0];
    let species = species_of(best);
    let runner_up = counted.iter().copied().find(|h| species_of(h) != species);
    Some(Closest { best, runner_up, counted: counted.len() })
}

/// More than one species among counted hits within 1% identity of the best one.
pub fn mixed_species(hits: &[HitRow], min_cover: f64) -> bool {
    let Some(c) = closest(hits, min_cover) else { return false };
    let species = species_of(c.best);
    hits.iter()
        .filter(|h| h.query_cover + 1e-9 >= min_cover)
        .any(|h| h.per_ident >= c.best.per_ident - 1.0 && species_of(h) != species)
}

pub fn compile<'q>(
    reads: &[Read],
    result_for: impl Fn(&Read) -> Option<(&'q QueryResult, String)>,
    flags_for: impl Fn(&Read) -> Vec<Flag>,
    min_cover: f64,
) -> Compiled {
    let mut rows = Vec::with_capacity(reads.len());
    let mut based_on = Vec::with_capacity(reads.len());
    for read in reads {
        let flags = flags_for(read);
        let found = result_for(read);
        based_on.push((read.id.clone(), found.as_ref().map(|(_, key)| key.clone())));
        let mut row = CompiledRow {
            sample_id: read.id.clone(),
            species: None,
            identity: None,
            query_cover: None,
            accession: None,
            title: None,
            runner_up: None,
            runner_up_identity: None,
            hits_counted: 0,
            reason: None,
            flags,
        };
        match found {
            None => {
                row.reason = Some(if read.excluded {
                    "excluded".into()
                } else if !read.derived.eligible {
                    "failedQc".into()
                } else {
                    "notSearched".into()
                })
            }
            Some((result, _)) => match closest(&result.hits, min_cover) {
                None => row.reason = Some("noHits".into()),
                Some(c) => {
                    row.species = Some(species_of(c.best));
                    row.identity = Some(c.best.per_ident);
                    row.query_cover = Some(c.best.query_cover);
                    row.accession = Some(c.best.accession.clone());
                    row.title = Some(c.best.title.clone());
                    row.runner_up = c.runner_up.map(species_of);
                    row.runner_up_identity = c.runner_up.map(|h| h.per_ident);
                    row.hits_counted = c.counted;
                }
            },
        }
        rows.push(row);
    }
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for row in &rows {
        if let Some(s) = &row.species {
            *counts.entry(s.as_str()).or_default() += 1;
        }
    }
    let total: usize = counts.values().sum();
    let mut chart: Vec<SpeciesCount> = counts
        .into_iter()
        .map(|(species, count)| SpeciesCount {
            species: species.to_string(),
            count,
            pct: count as f64 * 100.0 / total.max(1) as f64,
        })
        .collect();
    chart.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.species.cmp(&b.species)));
    Compiled { created_at: chrono::Local::now().to_rfc3339(), min_cover, rows, chart, based_on }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(rank: usize, species: &str, ident: f64, cover: f64, score: f64) -> HitRow {
        HitRow {
            rank,
            accession: format!("AC{rank}"),
            title: format!("{species} isolate {rank}"),
            sciname: species.into(),
            taxid: None,
            max_score: score,
            total_score: score,
            query_cover: cover,
            evalue: 0.0,
            per_ident: ident,
            acc_len: 650,
            hsp_count: 1,
        }
    }

    #[test]
    fn short_hit_does_not_win() {
        let hits = vec![hit(1, "Aedes albopictus", 99.8, 100.0, 1100.0), hit(2, "Culex sp.", 100.0, 8.0, 90.0)];
        let c = closest(&hits, 80.0).unwrap();
        assert_eq!(c.best.sciname, "Aedes albopictus");
        assert_eq!(c.counted, 1);
    }

    #[test]
    fn highest_identity_beats_ncbi_order() {
        let hits = vec![hit(1, "A one", 99.1, 100.0, 1200.0), hit(2, "B two", 99.5, 95.0, 1100.0)];
        let c = closest(&hits, 80.0).unwrap();
        assert_eq!(c.best.sciname, "B two");
        assert_eq!(c.runner_up.unwrap().sciname, "A one");
    }

    #[test]
    fn exact_tie_goes_to_species_with_more_records() {
        let hits = vec![
            hit(1, "A one", 100.0, 100.0, 1000.0),
            hit(2, "B two", 100.0, 100.0, 1000.0),
            hit(3, "B two", 99.0, 100.0, 990.0),
        ];
        assert_eq!(closest(&hits, 80.0).unwrap().best.sciname, "B two");
    }

    #[test]
    fn mixed_species_within_one_percent() {
        let hits = vec![hit(1, "A one", 99.8, 100.0, 1000.0), hit(2, "B two", 99.0, 100.0, 990.0)];
        assert!(mixed_species(&hits, 80.0));
        let hits = vec![hit(1, "A one", 99.8, 100.0, 1000.0), hit(2, "B two", 97.0, 100.0, 990.0)];
        assert!(!mixed_species(&hits, 80.0));
    }
}
