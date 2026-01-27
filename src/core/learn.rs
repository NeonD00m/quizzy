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
    cards: &Vec<Card>,
    rng: &mut ThreadRng,
    ask_term: bool,
) -> Vec<Card> {
    let expected = if ask_term {
        c.definition.clone()
    } else {
        c.term.clone()
    };

    // build a list of candidate cards (exclude the card itself)
    let mut candidates: Vec<(u8, Card)> = cards
        .iter()
        .filter(|other| other.term != c.term && other.definition != c.definition)
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

    // TODO: do non-deterministicly weighted by similarity
    let mut choices: Vec<Card> = candidates
        .into_iter()
        .take(3)
        .map(|(_, card)| card)
        .collect();

    // if fewer than 3 similar choices found, fill randomly
    if choices.len() < 3 {
        let mut additional: Vec<Card> = cards
            .iter()
            .filter(|other| other.term != c.term || other.definition != c.definition)
            .filter(|other| {
                !choices
                    .iter()
                    .any(|ch| ch.term == other.term && ch.definition == other.definition)
            })
            .cloned()
            .collect();
        additional.shuffle(rng);
        for card in additional.into_iter().take(3 - choices.len()) {
            choices.push(card);
        }
    }

    // add the correct card and shuffle
    choices.push(c.clone());
    choices.shuffle(rng);

    choices
}

/// Try to commit session updates with retries and backoff.
///
/// - `max_attempts`: how many total attempts (including first attempt).
/// - On transient errors (contains "locked" or "busy") we retry; otherwise we fail fast.
/// - Returns Ok(()) if commit succeeds, or Err(anyhow::Error) on permanent failure.
pub fn commit_session_with_retries(
    storage: &mut Storage,
    updates: &[(i64, i64, i64)],
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
/// failed session file format: each line  = "card_id,corrects,incorrects\n"
pub fn write_failed_session_file(updates: &[(i64, i64, i64)]) -> anyhow::Result<PathBuf> {
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

    for (card_id, corrects, incorrects) in updates {
        writeln!(f, "{},{},{}", card_id, corrects, incorrects)
            .map_err(|e| anyhow::anyhow!("failed to write to fallback session file: {}", e))?;
    }

    Ok(path)
}
