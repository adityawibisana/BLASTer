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

/// A name that does not identify a species: "Culex sp. RP-2019-12", "Culicidae sp.",
/// "Diptera sp. BOLD:AAA4210", "Aedes cf. albopictus", "uncultured Culex", ...
pub fn is_placeholder(species: &str) -> bool {
    let words: Vec<&str> = species.split_whitespace().collect();
    const OPEN: &[&str] = &["sp.", "sp", "spp.", "spp", "cf.", "cf", "aff.", "aff", "nr.", "nr", "gen."];
    words.len() < 2
        || words.iter().any(|w| OPEN.contains(&w.to_ascii_lowercase().as_str()))
        || matches!(words[0].to_ascii_lowercase().as_str(), "uncultured" | "unidentified" | "unclassified")
        || species.contains("BOLD:")
        || species.to_ascii_lowercase().contains("environmental sample")
}

fn rank_key(h: &HitRow) -> (i64, i64, bool, i64) {
    // Rounded as NCBI displays them, so equal-looking values tie. At equal identity
    // and cover, a named species comes before a placeholder name.
    (
        (h.per_ident * 100.0).round() as i64,
        (h.query_cover * 100.0).round() as i64,
        !is_placeholder(&species_of(h)),
        (h.max_score * 10.0).round() as i64,
    )
}

pub struct Closest<'a> {
    pub best: &'a HitRow,
    pub runner_up: Option<&'a HitRow>,
    pub counted: usize,
}

/// Highest % identity among hits covering at least `min_cover` % of the query.
/// Exact ties: higher query cover, then a named species over a placeholder name,
/// then higher Max Score, then the species backed by more hit records, then NCBI's
/// order. Always a single answer. The runner-up is the closest other named species
/// (any other species when there is none).
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
    let other = |h: &&HitRow| species_of(h) != species;
    let runner_up = counted
        .iter()
        .copied()
        .find(|h| other(h) && !is_placeholder(&species_of(h)))
        .or_else(|| counted.iter().copied().find(other));
    Some(Closest { best, runner_up, counted: counted.len() })
}

/// Another named species among counted hits within 1% identity of the best one.
/// Placeholder names ("Culicidae sp.", "Aedes sp.", BOLD BINs) do not count.
pub fn mixed_species(hits: &[HitRow], min_cover: f64) -> bool {
    let Some(c) = closest(hits, min_cover) else { return false };
    let species = species_of(c.best);
    hits.iter().filter(|h| h.query_cover + 1e-9 >= min_cover).any(|h| {
        let s = species_of(h);
        h.per_ident >= c.best.per_ident - 1.0 && s != species && !is_placeholder(&s)
    })
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
    fn named_species_beats_placeholder_at_equal_identity() {
        // VN236-1: one "Culex sp. RP-2019-12" record with a slightly higher score
        // against 68 Culex quinquefasciatus records, all at 100%.
        let hits = vec![
            hit(1, "Culex sp. RP-2019-12", 100.0, 100.0, 1150.0),
            hit(2, "Culex quinquefasciatus", 100.0, 100.0, 1140.0),
            hit(3, "Culex quinquefasciatus", 100.0, 100.0, 1140.0),
            hit(4, "Culicidae sp.", 100.0, 100.0, 1150.0),
            hit(5, "Culex pipiens", 100.0, 100.0, 1140.0),
        ];
        let c = closest(&hits, 80.0).unwrap();
        assert_eq!(c.best.sciname, "Culex quinquefasciatus");
        assert_eq!(c.runner_up.unwrap().sciname, "Culex pipiens");
        assert!(mixed_species(&hits, 80.0));
        // A placeholder that is strictly closer still wins: it may be an undescribed species.
        let hits = vec![hit(1, "Culex sp. RP-2019-12", 100.0, 100.0, 1150.0), hit(2, "Culex pipiens", 98.0, 100.0, 1100.0)];
        assert_eq!(closest(&hits, 80.0).unwrap().best.sciname, "Culex sp. RP-2019-12");
    }

    #[test]
    fn placeholders_do_not_make_mixed_species() {
        let hits = vec![
            hit(1, "Armigeres subalbatus", 100.0, 100.0, 1100.0),
            hit(2, "Culicidae sp.", 100.0, 100.0, 1100.0),
            hit(3, "Diptera sp. BOLD:AAA4210", 99.8, 100.0, 1090.0),
        ];
        assert!(!mixed_species(&hits, 80.0));
        assert!(is_placeholder("Culex pipiens complex sp. CROBB649"));
        assert!(is_placeholder("uncultured Culex"));
        assert!(is_placeholder("Aedes cf. albopictus"));
        assert!(!is_placeholder("Culex pipiens molestus"));
        assert!(!is_placeholder("Aedes albopictus"));
    }

    #[test]
    fn mixed_species_within_one_percent() {
        let hits = vec![hit(1, "A one", 99.8, 100.0, 1000.0), hit(2, "B two", 99.0, 100.0, 990.0)];
        assert!(mixed_species(&hits, 80.0));
        let hits = vec![hit(1, "A one", 99.8, 100.0, 1000.0), hit(2, "B two", 97.0, 100.0, 990.0)];
        assert!(!mixed_species(&hits, 80.0));
    }
}
