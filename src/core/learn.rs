use crate::core::deck::*;
use crate::core::storage::{FSRSStats, Storage, db_path_from_env_or_default};
use crate::core::string_distance::string_distance;
use anyhow::Context;
use core::f64;
use rand::Rng;
use rand::rngs::ThreadRng;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::cmp::min;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::core::fsrs::Rating;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum FSRSGrade {
    Again = 1,
    Hard = 2,
    Good = 3,
    Easy = 4,
}

impl FSRSGrade {
    pub fn score_delta(&self) -> i64 {
        match self {
            FSRSGrade::Again => -3,
            FSRSGrade::Hard => -1,
            FSRSGrade::Good => 1,
            FSRSGrade::Easy => 3,
        }
    }

    pub fn to_rating(&self) -> Rating {
        match self {
            FSRSGrade::Again => Rating::Again,
            FSRSGrade::Hard => Rating::Hard,
            FSRSGrade::Good => Rating::Good,
            FSRSGrade::Easy => Rating::Easy,
        }
    }

    pub fn from_rating(rating: Rating) -> Self {
        match rating {
            Rating::Again => FSRSGrade::Again,
            Rating::Hard => FSRSGrade::Hard,
            Rating::Good => FSRSGrade::Good,
            Rating::Easy => FSRSGrade::Easy,
        }
    }
}

/// Represents session performance deltas to be persisted across different study modes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionPayload {
    /// Test mode: (card_id, corrects, incorrects)
    Test { updates: Vec<(i64, i64, i64)> },
    /// Cram mode: (card_id, score_delta)
    Cram { updates: Vec<(i64, i64)> },
    /// Learn mode (FSRS): (card_id, fsrs_stats, corrects, incorrects, score_delta)
    Learn {
        updates: Vec<(i64, FSRSStats, i64, i64, i64)>,
    },
}

impl SessionPayload {
    pub fn is_empty(&self) -> bool {
        match self {
            SessionPayload::Test { updates } => updates.is_empty(),
            SessionPayload::Cram { updates } => updates.is_empty(),
            SessionPayload::Learn { updates } => updates.is_empty(),
        }
    }
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

    // use confusion-based candidates (if provided)
    let mut chosen: Vec<Card> = Vec::new();
    if let Some(confusion_vec) = confusions {
        // map confusion entries to cards
        let mut confusion_candidates: Vec<(i64, Card)> = Vec::new();
        for (mistaken_id, count) in confusion_vec.iter() {
            if let Some(card) = cards.iter().find(|oc| oc.id == Some(*mistaken_id)) {
                if c.same(card) {
                    continue; // important sanity check lol
                }
                // cap the confusion count to 20 to not over-value a single card
                confusion_candidates.push((min(*count, 20), card.clone()));
            }
        }
        let mut confusions_chosen = weighted_sample_no_replacement(confusion_candidates, 3, rng);
        // append any unique cards
        for chosen_card in confusions_chosen.drain(..) {
            if c.different(&chosen_card) && !chosen.contains(&chosen_card) {
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
                    other.definition.as_str()
                } else {
                    other.term.as_str()
                };
                let dist = string_distance(candidate_str, &expected);
                (dist, other.clone())
            })
            .collect();

        // sort ascending by distance (most similar first)
        candidates.sort_by_key(|(dist, _)| *dist);

        for (_, card) in candidates.into_iter().take(3 - chosen.len()) {
            if !chosen.contains(&card) && c.different(&card) {
                chosen.push(card); // only push cards with different terms AND definitions
            }
        }
    }

    // *sighs* if we still don't have 3 cards, put placeholders and admit defeat
    for i in 0..((3_usize).saturating_sub(chosen.len())) {
        let str = format!("[No option {}]", i);
        chosen.push(Card::new(str.as_str(), str.as_str()));
    }

    // add the correct card and shuffle
    chosen.push(c.clone());
    chosen.shuffle(rng);

    chosen
}

/// Try to commit session updates with retries and exponential backoff.
///
/// - `max_attempts`: total attempts (including first).
/// - On transient errors ("locked" or "busy"), retries with backoff.
/// - Returns Ok(()) if commit succeeds, or Err(anyhow::Error) on permanent failure.
pub fn commit_payload_with_retries(
    storage: &Storage,
    payload: &SessionPayload,
    max_attempts: usize,
) -> anyhow::Result<()> {
    if payload.is_empty() {
        return Ok(());
    }

    let mut attempt: usize = 0;
    let mut backoff_ms: u64 = 50;

    loop {
        attempt += 1;
        let res = match payload {
            SessionPayload::Test { updates } => storage.commit_test_session(updates),
            SessionPayload::Cram { updates } => storage.commit_cram_session(updates),
            SessionPayload::Learn { updates } => storage.commit_learn_session(updates),
        };

        match res {
            Ok(()) => return Ok(()),
            Err(e) => {
                let err_str = format!("{}", e);
                let is_transient = err_str.to_lowercase().contains("locked")
                    || err_str.to_lowercase().contains("busy");

                if attempt >= max_attempts || !is_transient {
                    return Err(e);
                }

                eprintln!(
                    "commit_payload_with_retries attempt {}/{} failed with transient error: {}. Retrying in {}ms...",
                    attempt, max_attempts, err_str, backoff_ms
                );

                sleep(Duration::from_millis(backoff_ms));
                backoff_ms = min(backoff_ms.saturating_mul(2), 2000);
            }
        }
    }
}

#[deprecated(note = "Use commit_payload_with_retries instead")]
pub fn commit_session_with_retries(
    storage: &mut Storage,
    updates: &[(i64, i64, i64)],
    max_attempts: usize,
) -> anyhow::Result<()> {
    let payload = SessionPayload::Test {
        updates: updates.to_vec(),
    };
    commit_payload_with_retries(storage, &payload, max_attempts)
}

/// Write failed session payload as JSON to a timestamped local file next to the DB.
pub fn write_failed_session_file(payload: &SessionPayload) -> anyhow::Result<PathBuf> {
    let mut path = db_path_from_env_or_default();
    let parent = path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let filename = format!("quizzy_failed_session_{}.log", ts);
    path = parent.join(filename);

    let json_str = serde_json::to_string_pretty(payload)
        .context("failed to serialize session payload to JSON")?;

    std::fs::write(&path, json_str).with_context(|| {
        format!(
            "failed to write fallback session file to {}",
            path.display()
        )
    })?;

    Ok(path)
}

/// Read a failed session file created by `write_failed_session_file`.
/// Supports new JSON format as well as legacy CSV format (`card_id,corrects,incorrects`).
pub fn read_failed_session_file(path: &Path) -> anyhow::Result<SessionPayload> {
    let s = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read failed session file {}.", path.display()))?;

    if let Ok(payload) = serde_json::from_str::<SessionPayload>(&s) {
        return Ok(payload);
    }

    let mut updates = Vec::new();
    for (line_number, line) in s.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() >= 3 {
            let a: i64 = parts[0].trim().parse().with_context(|| {
                format!(
                    "Invalid card_id in {} line {}.",
                    path.display(),
                    line_number + 1
                )
            })?;
            let b: i64 = parts[1].trim().parse().with_context(|| {
                format!(
                    "Invalid corrects in {} line {}.",
                    path.display(),
                    line_number + 1
                )
            })?;
            let c: i64 = parts[2].trim().parse().with_context(|| {
                format!(
                    "Invalid incorrects in {} line {}.",
                    path.display(),
                    line_number + 1
                )
            })?;
            updates.push((a, b, c));
        }
    }
    Ok(SessionPayload::Test { updates })
}
