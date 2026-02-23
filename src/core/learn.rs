use crate::core::deck::*;
use crate::core::storage::{Storage, db_path_from_env_or_default};
use crate::core::string_distance::string_distance;
use core::f64;
use rand::Rng;
use rand::rngs::ThreadRng;
use rand::seq::SliceRandom;
use std::cmp::min;
use std::fs::OpenOptions;
use std::io::Write as IoWrite;
use std::path::PathBuf;
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct SM2Stats {
    pub interval: i64,
    pub repetitions: i64,
    pub easiness_factor: f64,
}

impl Default for SM2Stats {
    fn default() -> Self {
        Self {
            interval: 0,
            repetitions: 0,
            easiness_factor: 2.5,
        }
    }
}

/// Calculate the next interval and stats for a card based on SM-2 algorithm.
/// `quality` is a value from 0 to 5.
pub fn calculate_sm2(stats: SM2Stats, quality: u8) -> (SM2Stats, i64) {
    let mut n = stats.repetitions;
    let mut ef = stats.easiness_factor;
    let mut i = stats.interval;

    if quality >= 3 {
        if n == 0 {
            i = 1;
        } else if n == 1 {
            i = 6;
        } else {
            i = (i as f64 * ef).round() as i64;
        }
        n += 1;
    } else {
        n = 0;
        i = 1;
    }

    ef = ef + (0.1 - (5.0 - quality as f64) * (0.08 + (5.0 - quality as f64) * 0.02));
    if ef < 1.3 {
        ef = 1.3;
    }

    (
        SM2Stats {
            interval: i,
            repetitions: n,
            easiness_factor: ef,
        },
        i,
    )
}

pub fn decide(condition1: bool, condition2: bool, rng: &mut ThreadRng, probability: f64) -> bool {
    if condition1 {
        true
    } else if condition2 {
        false
    } else {
        rng.gen_bool(probability)
    }
}

/// "learned" threshold that scales with deck size, likely temporary
pub fn learned_threshold(deck_size: usize) -> i64 {
    8 + (deck_size as f64 * 0.5_f64) as i64
}

/// Returns a vector including the original card and 3 others, randomly sorted
pub fn get_multiple_choice_for_card(
    c: &Card,
    cards: &[Card],
    rng: &mut ThreadRng,
    ask_term: bool,
    confusions: Option<&Vec<(i64, i64)>>,
) -> Vec<Card> {
    let expected = if ask_term {
        c.definition.clone()
    } else {
        c.term.clone()
    };

    // weighted sample without replacement from Vec<(weight, Card)>
    fn weighted_sample_no_replacement(
        mut items: Vec<(i64, Card)>,
        k: usize,
        rng: &mut ThreadRng,
    ) -> Vec<Card> {
        let mut out = Vec::new();
        if items.is_empty() || k == 0 {
            return out;
        }
        // make sure no negative weights
        for it in items.iter_mut() {
            if it.0 < 0 {
                it.0 = 0;
            }
        }

        while out.len() < k && !items.is_empty() {
            let total: i64 = items.iter().map(|(w, _)| *w).sum();
            if total <= 0 {
                break;
            }
            let pick = rng.gen_range(0..total);
            let mut idx = 0usize;
            let mut acc = 0i64;
            for (i, (w, _)) in items.iter().enumerate() {
                acc += *w;
                if pick < acc {
                    idx = i;
                    break;
                }
            }
            let chosen = items.remove(idx).1;
            out.push(chosen);
        }
        out
    }

    // use confusion-based candiates (if provided)
    let mut chosen: Vec<Card> = Vec::new();
    if let Some(confusion_vec) = confusions {
        // map confusion entries to cards
        let mut confusion_candidates: Vec<(i64, Card)> = Vec::new();
        for (mistaken_id, count) in confusion_vec.iter() {
            if let Some(card) = cards.iter().find(|oc| oc.id == Some(*mistaken_id)) {
                if card == c {
                    continue; // important sanity check lol
                }
                // cap the confusion count to 20 to not over-value a single card
                confusion_candidates.push((min(*count, 20), card.clone()));
            }
        }
        let mut confusions_chosen = weighted_sample_no_replacement(confusion_candidates, 3, rng);
        // append any unique cards
        for chosen_card in confusions_chosen.drain(..) {
            if chosen_card != *c && !chosen.contains(&chosen_card) {
                chosen.push(chosen_card);
            }
        }
    }
    // if not enough confusions, use string distance
    if chosen.len() < 3 {
        let mut candidates: Vec<(u8, Card)> = cards
            .iter()
            .filter(|other| *other != c)
            .map(|other| {
                let candidate_str = if ask_term {
                    other.definition.clone()
                } else {
                    other.term.clone()
                };
                let dist = string_distance(candidate_str, expected.clone());
                (dist, other.clone())
            })
            .collect();

        // sort ascending by distance (most similar first)
        candidates.sort_by_key(|(dist, _)| *dist);

        for (_distance, card) in candidates.into_iter().take(3 - chosen.len()) {
            if !chosen.contains(&card) {
                chosen.push(card);
            }
        }
    }

    // *sighs* if we still don't have 3 cards, put placeholders
    for i in 0..((3_usize).saturating_sub(chosen.len())) {
        let str = format!("[No option {}]", i);
        chosen.push(Card::new(str.as_str(), str.as_str()));
    }

    // add the correct card and shuffle
    chosen.push(c.clone());
    chosen.shuffle(rng);

    chosen
}

/// Try to commit session updates with retries and backoff.
///
/// - `max_attempts`: how many total attempts (including first attempt).
/// - On transient errors (contains "locked" or "busy") we retry; otherwise we fail fast.
/// - Returns Ok(()) if commit succeeds, or Err(anyhow::Error) on permanent failure.
pub fn commit_session_with_retries(
    storage: &mut Storage,
    updates: &[(i64, i64, i64, Option<SM2Stats>)],
    max_attempts: usize,
) -> anyhow::Result<()> {
    if updates.is_empty() {
        return Ok(());
    }

    let mut attempt: usize = 0;
    let mut backoff_ms: u64 = 50;

    loop {
        attempt += 1;
        match storage.commit_session_batch(updates) {
            Ok(()) => return Ok(()),
            Err(e) => {
                // Inspect error string for likely transient causes (e.g. SQLITE_BUSY / "database is locked").
                // We downcast/inspect generically via the error string because commit_session_batch returns anyhow::Error.
                let err_str = format!("{}", e);
                let is_transient = err_str.to_lowercase().contains("locked")
                    || err_str.to_lowercase().contains("busy");

                if attempt >= max_attempts || !is_transient {
                    // Give up and propagate the original error.
                    return Err(e);
                }

                eprintln!(
                    "commit_session_batch attempt {}/{} failed with transient error: {}. Retrying in {}ms...",
                    attempt, max_attempts, err_str, backoff_ms
                );

                sleep(Duration::from_millis(backoff_ms));
                // exponential backoff, cap at 2000ms
                backoff_ms = min(backoff_ms.saturating_mul(2), 2000);
            }
        }
    }
}

/// Write failed session deltas to a timestamped local file next to the DB
/// failed session file format: each line  = "card_id,corrects,incorrects,sm2_json\n"
pub fn write_failed_session_file(updates: &[(i64, i64, i64, Option<SM2Stats>)]) -> anyhow::Result<PathBuf> {
    // Use storage's db path helper to find the DB directory (stores next to DB).
    let mut path = db_path_from_env_or_default();
    let parent = path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    // timestamp for filename
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let filename = format!("quizzy_failed_session_{}.log", ts);
    path = parent.join(filename);

    let mut f = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)
        .map_err(|e| anyhow::anyhow!("failed to create fallback session file: {}", e))?;

    for (card_id, corrects, incorrects, sm2) in updates {
        let sm2_str = if let Some(s) = sm2 {
            serde_json::to_string(s).unwrap_or_default()
        } else {
            "NONE".to_string()
        };
        writeln!(f, "{},{},{},{}", card_id, corrects, incorrects, sm2_str)
            .map_err(|e| anyhow::anyhow!("failed to write to fallback session file: {}", e))?;
    }

    Ok(path)
}
