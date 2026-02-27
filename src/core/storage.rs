use crate::core::deck::{Card, Deck, DeckSource, read_deck_from_file};
use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::OptionalExtension;
use rusqlite::{Connection, OpenFlags, params};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub struct DeckStatsSummary {
    pub total_cards: i64,
    pub new_count: i64,
    pub learning_count: i64,
    pub mature_count: i64,
    pub average_easiness: f64,
}

pub struct CardStatRow {
    #[allow(dead_code)]
    pub card_id: i64,
    pub term: String,
    pub definition: String,
    pub learning_score: i64,
    pub interval: i64,
    pub easiness: f64,
    pub next_due: i64,
}

pub type SessionDelta = (i64, i64, i64, Option<crate::core::learn::SM2Stats>);

// make learnings core constants for correct and incorrect answers
const CORRECT_ANSWER_SCORE: i64 = 3;
const INCORRECT_ANSWER_SCORE: i64 = 1;

/// Schema initialized by `init_db`
const SCHEMA: &str = r#"
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS decks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    description TEXT,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    source_path TEXT,
    source_hash TEXT
);

CREATE TABLE IF NOT EXISTS cards (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    deck_id INTEGER NOT NULL REFERENCES decks(id) ON DELETE CASCADE,
    term TEXT NOT NULL,
    definition TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);

CREATE INDEX IF NOT EXISTS idx_cards_deck_id ON cards(deck_id);

CREATE TABLE IF NOT EXISTS card_stats (
    card_id INTEGER PRIMARY KEY REFERENCES cards(id) ON DELETE CASCADE,
    learning_score INTEGER NOT NULL DEFAULT 0,
    correct_count INTEGER NOT NULL DEFAULT 0,
    incorrect_count INTEGER NOT NULL DEFAULT 0,
    last_answered_at INTEGER,
    interval INTEGER NOT NULL DEFAULT 0,
    repetitions INTEGER NOT NULL DEFAULT 0,
    easiness_factor REAL NOT NULL DEFAULT 2.5,
    next_due INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);

CREATE INDEX IF NOT EXISTS idx_card_stats_learning_score ON card_stats(learning_score);
CREATE INDEX IF NOT EXISTS idx_card_stats_next_due ON card_stats(next_due);

CREATE TABLE IF NOT EXISTS deck_stats (
    deck_id INTEGER PRIMARY KEY REFERENCES decks(id) ON DELETE CASCADE,
    questions_answered_total INTEGER NOT NULL DEFAULT 0,
    questions_correct_total INTEGER NOT NULL DEFAULT 0,
    last_studied_at INTEGER
);

CREATE TABLE IF NOT EXISTS card_confusions (
    card_id INTEGER NOT NULL REFERENCES cards(id) ON DELETE CASCADE,
    mistaken_card_id INTEGER NOT NULL REFERENCES cards(id) ON DELETE CASCADE,
    count INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (card_id, mistaken_card_id)
);

CREATE TABLE IF NOT EXISTS user_profile (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    currency INTEGER NOT NULL DEFAULT 0,
    streak INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);
"#;

/// wrapper around rusqlite connection exposing the repository API
pub struct Storage {
    pub conn: Connection,
}

fn now_secs() -> i64 {
    Utc::now().timestamp()
}

// Returns the path to the database file to use.
// Priority:
//  1) Environment variable QUIZZY_DB
//  2) OS-specific user data directory under "quizzy/quizzy.db"
pub fn db_path_from_env_or_default() -> PathBuf {
    if let Ok(p) = env::var("QUIZZY_DB") {
        return PathBuf::from(p);
    }

    let mut base = dirs_next::data_local_dir()
        .or_else(dirs_next::data_dir)
        .unwrap_or_else(|| {
            env::current_dir().expect("Unable to determine current directory for DB fallback")
        });

    base.push("quizzy");
    let _ = fs::create_dir_all(&base);
    base.push("quizzy.db");
    base
}

impl Storage {
    /// Return the `updated_at` timestamp from `user_profile` (if present)
    pub fn get_user_last_active(&self) -> Result<Option<i64>> {
        use rusqlite::OptionalExtension;
        let val: Option<i64> = self
            .conn
            .query_row(
                "SELECT updated_at FROM user_profile WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .optional()
            .context("Failed to query updated_at in user_profile.")?;
        Ok(val)
    }

    /// Find unsaved session files written by fallback logic.
    /// They live next to the DB file and match `quizzy_failed_session_*.log`.
    pub fn failed_session_files(&self) -> Result<Vec<std::path::PathBuf>> {
        let mut dir = db_path_from_env_or_default();
        // We want the directory containing the DB.
        if let Some(parent) = dir.parent() {
            dir = parent.to_path_buf();
        } else {
            dir = std::path::PathBuf::from(".");
        }

        let mut out = Vec::new();
        for entry in
            std::fs::read_dir(&dir).context("Failed to read DB directory for failed sessions.")?
        {
            let entry = entry.context("Failed to read directory entry.")?;
            let p = entry.path();
            if let Some(name) = p.file_name().and_then(|n| n.to_str())
                && name.starts_with("quizzy_failed_session_")
                && name.ends_with(".log")
            {
                out.push(p);
            }
        }
        Ok(out)
    }

    /// Parse a failed session file created by `write_failed_session_file`.
    /// Format expected: each line `card_id,corrects,incorrects,sm2_json`
    pub fn read_failed_session_file(&self, path: &std::path::Path) -> Result<Vec<SessionDelta>> {
        let s = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read failed session file {}.", path.display()))?;
        let mut out = Vec::new();
        for (line_number, line) in s.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() != 4 {
                // Compatibility check: old format has 3 parts
                if parts.len() == 3 {
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
                    out.push((a, b, c, None));
                    continue;
                }
                return Err(anyhow::anyhow!(
                    "Invalid format in {} at line {}. Expected 4 columns.",
                    path.display(),
                    line_number + 1
                ));
            }
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
            let d_str = parts[3].trim();
            let d: Option<crate::core::learn::SM2Stats> = if d_str == "NONE" {
                None
            } else {
                Some(serde_json::from_str(d_str).with_context(|| {
                    format!(
                        "Invalid SM2Stats JSON in {} line {}.",
                        path.display(),
                        line_number + 1
                    )
                })?)
            };
            out.push((a, b, c, d));
        }
        Ok(out)
    }

    /// Remove a failed session file after replay or if user discards it.
    pub fn remove_failed_session_file(&self, path: &Path) -> Result<()> {
        std::fs::remove_file(path)
            .with_context(|| format!("Failed to remove failed session file {}.", path.display()))?;
        Ok(())
    }

    /// Open or create the DB at the canonical path and initialize schema.
    pub fn open_default() -> Result<Self> {
        let path = db_path_from_env_or_default();

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create database parent directory {:?}.", parent)
            })?;
        }

        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE;
        let conn = Connection::open_with_flags(&path, flags)
            .with_context(|| format!("Failed to open sqlite database at {:?}.", path))?;

        conn.busy_timeout(std::time::Duration::from_secs(5))
            .context("Failed to set busy_timeout on sqlite connection.")?;

        // initialize schema and pragmas
        init_db(&conn).context("Failed to initialize database schema.")?;

        Ok(Self { conn })
    }

    /// List decks (id, name)
    pub fn list_decks(&self) -> Result<Vec<(i64, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name FROM decks ORDER BY name")
            .context("Failed to prepare list_decks.")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .context("Failed to query decks.")?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.context("Failed mapping deck row.")?);
        }
        Ok(out)
    }

    /// Create a deck and persist all cards in a single transaction.
    /// Returns the new deck id.
    pub fn create_deck_from_core(
        &mut self,
        deck: Deck,
        source_path: Option<&str>,
        source_hash: Option<&str>,
    ) -> Result<i64> {
        let now = now_secs();
        self.conn.execute(
            "INSERT INTO decks (name, description, created_at, updated_at, source_path, source_hash) VALUES (?1, ?2, ?3, ?3, ?4, ?5)",
            params![deck.name, None::<&str>, now, source_path, source_hash],
        ).context("Failed to insert deck row.")?;
        let deck_id = self.conn.last_insert_rowid();

        let tx = self
            .conn
            .transaction()
            .context("Failed to start transaction for deck insert.")?;
        for c in deck.cards {
            tx.execute(
                "INSERT INTO cards (deck_id, term, definition, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)",
                params![deck_id, c.term, c.definition, now],
            ).context("Failed to insert card.")?;
            let card_id = tx.last_insert_rowid();
            tx.execute(
                "INSERT INTO card_stats (card_id) VALUES (?1)",
                params![card_id],
            )
            .context("Failed to insert card_stats.")?;
        }
        tx.execute(
            "INSERT INTO deck_stats (deck_id) VALUES (?1)",
            params![deck_id],
        )
        .context("Failed to insert deck_stats.")?;
        tx.commit()
            .context("Failed to commit create_deck transaction.")?;

        Ok(deck_id)
    }

    /// Add a single card to a deck
    pub fn add_card_to_deck(&mut self, deck_id: i64, term: &str, definition: &str) -> Result<i64> {
        let now = now_secs();
        self.conn.execute(
            "INSERT INTO cards (deck_id, term, definition, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)",
            params![deck_id, term, definition, now],
        ).context("Failed to insert card.")?;
        let card_id = self.conn.last_insert_rowid();
        self.conn
            .execute(
                "INSERT INTO card_stats (card_id) VALUES (?1)",
                params![card_id],
            )
            .context("Failed to insert card_stats.")?;
        Ok(card_id)
    }

    /// Remove a card by id
    pub fn remove_card(&mut self, card_id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM cards WHERE id = ?1", params![card_id])
            .context("Failed to delete card.")?;
        Ok(())
    }

    /// Get a deck by name (returns deck with card ids populated)
    pub fn get_deck_by_name(&self, name: &str) -> Result<Deck> {
        let deck_id: i64 = self
            .conn
            .query_row("SELECT id FROM decks WHERE name = ?1", params![name], |r| {
                r.get(0)
            })
            .with_context(|| format!("Deck named '{}' not found.", name))?;
        self.get_deck_by_id(deck_id)
    }

    /// Get a deck by id (card ids are included)
    pub fn get_deck_by_id(&self, deck_id: i64) -> Result<Deck> {
        let name: String = self
            .conn
            .query_row(
                "SELECT name FROM decks WHERE id = ?1",
                params![deck_id],
                |r| r.get(0),
            )
            .context("Failed to query deck metadata.")?;

        let mut stmt = self
            .conn
            .prepare("SELECT id, term, definition FROM cards WHERE deck_id = ?1 ORDER BY id")
            .context("Failed to prepare select cards for deck.")?;
        let rows = stmt
            .query_map(params![deck_id], |r| {
                Ok(Card {
                    id: r.get(0)?,
                    term: r.get(1)?,
                    definition: r.get(2)?,
                })
            })
            .context("Failed to query_map cards.")?;

        let mut cards = Vec::new();
        for r in rows {
            cards.push(r.context("Failed mapping card row.")?);
        }

        Ok(Deck {
            name,
            cards,
            id: Some(deck_id),
        })
    }

    /// Delete a deck by id (maybe use this over name to prevent accidents?)
    pub fn delete_deck_by_id(&mut self, deck_id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM decks WHERE id = ?1", params![deck_id])
            .context("Failed to delete deck.")?;
        Ok(())
    }

    /// Delete a deck by name (so much more convenient)
    pub fn delete_deck_by_name(&mut self, name: &str) -> Result<()> {
        let deck_id: i64 = self
            .conn
            .query_row("SELECT id FROM decks WHERE name = ?1", params![name], |r| {
                r.get(0)
            })
            .with_context(|| format!("Deck '{}' not found.", name))?;
        self.delete_deck_by_id(deck_id)
    }

    /// Immediate update for a single answer (durable).
    /// correct: true => +3 learning_score and +1 correct_count; false => -1 learning_score and +1 incorrect_count
    pub fn _record_answer_immediate(&mut self, card_id: i64, correct: bool) -> Result<()> {
        let now = now_secs();
        let (score_delta, correct_delta, incorrect_delta) =
            if correct { (3, 1, 0) } else { (-1, 0, 1) };

        let tx = self
            .conn
            .transaction()
            .context("Failed to start transaction.")?;

        tx.execute(
            "UPDATE card_stats
             SET learning_score = learning_score + ?1,
                 correct_count = correct_count + ?2,
                 incorrect_count = incorrect_count + ?3,
                 last_answered_at = ?4
             WHERE card_id = ?5",
            params![score_delta, correct_delta, incorrect_delta, now, card_id],
        )
        .context("Failed to update card_stats.")?;

        let deck_id: i64 = tx
            .query_row(
                "SELECT deck_id FROM cards WHERE id = ?1",
                params![card_id],
                |r| r.get(0),
            )
            .context("Failed to lookup deck_id for card.")?;

        tx.execute(
            "UPDATE deck_stats
             SET questions_answered_total = questions_answered_total + 1,
                 questions_correct_total = questions_correct_total + ?1,
                 last_studied_at = ?2
             WHERE deck_id = ?3",
            params![if correct { 1 } else { 0 }, now, deck_id],
        )
        .context("Failed to update deck_stats.")?;

        tx.commit().context("Failed to commit transaction.")?;
        Ok(())
    }

    /// Batch commit at the end of a learning session.
    /// `updates` is a slice of tuples: (card_id, corrects_delta, incorrects_delta, Option<SM2Stats>)
    pub fn commit_session_batch(
        &mut self,
        updates: &[(i64, i64, i64, Option<crate::core::learn::SM2Stats>)],
    ) -> Result<()> {
        if updates.is_empty() {
            return Ok(());
        }

        let now = now_secs();
        let tx = self
            .conn
            .transaction()
            .context("failed to start transaction")?;

        let mut deck_deltas: HashMap<i64, (i64, i64)> = HashMap::new(); // deck_id -> (questions_total_delta, questions_correct_delta)

        for (card_id, corrects, incorrects, sm2) in updates {
            let score_delta = CORRECT_ANSWER_SCORE * corrects - INCORRECT_ANSWER_SCORE * incorrects;
            if let Some(s) = sm2 {
                let next_due = now + s.interval * 86400;
                tx.execute(
                    "UPDATE card_stats
                 SET learning_score = learning_score + ?1,
                     correct_count = correct_count + ?2,
                     incorrect_count = incorrect_count + ?3,
                     last_answered_at = ?4,
                     interval = ?5,
                     repetitions = ?6,
                     easiness_factor = ?7,
                     next_due = ?8
                 WHERE card_id = ?9",
                    params![
                        score_delta,
                        corrects,
                        incorrects,
                        now,
                        s.interval,
                        s.repetitions,
                        s.easiness_factor,
                        next_due,
                        card_id
                    ],
                )
                .with_context(|| format!("Failed to update card_stats for card_id {}.", card_id))?;
            } else {
                tx.execute(
                    "UPDATE card_stats
                 SET learning_score = learning_score + ?1,
                     correct_count = correct_count + ?2,
                     incorrect_count = incorrect_count + ?3,
                     last_answered_at = ?4
                 WHERE card_id = ?5",
                    params![score_delta, corrects, incorrects, now, card_id],
                )
                .with_context(|| format!("Failed to update card_stats for card_id {}.", card_id))?;
            }

            let deck_id: i64 = tx
                .query_row(
                    "SELECT deck_id FROM cards WHERE id = ?1",
                    params![card_id],
                    |r| r.get(0),
                )
                .with_context(|| format!("Failed to lookup deck_id for card_id {}.", card_id))?;

            let entry = deck_deltas.entry(deck_id).or_insert((0, 0));
            entry.0 += corrects + incorrects;
            entry.1 += *corrects;
        }

        for (deck_id, (q_delta, correct_delta)) in deck_deltas {
            tx.execute(
                "UPDATE deck_stats
                 SET questions_answered_total = questions_answered_total + ?1,
                     questions_correct_total = questions_correct_total + ?2,
                     last_studied_at = ?3
                 WHERE deck_id = ?4",
                params![q_delta, correct_delta, now, deck_id],
            )
            .with_context(|| format!("Failed to update deck_stats for deck_id {}.", deck_id))?;
        }

        tx.commit().context("Failed to commit batch transaction.")?;
        Ok(())
    }

    /// Update a confusion count for (card_id, mistaken_with) by adding delta.
    /// If new score <= 0, remove the confusion row.
    ///
    /// Behavior:
    ///  - If a row exists: new_count = old_count + delta.
    ///      - If new_count > 0 => UPDATE count = new_count
    ///      - If new_count <= 0 => DELETE row
    ///  - If no row exists and delta > 0 => INSERT new row with count = delta
    pub fn adjust_confusion(&mut self, card_id: i64, mistaken_with: i64, delta: i64) -> Result<()> {
        let tx = self
            .conn
            .transaction()
            .context("Failed to start transaction for adjust_confusion.")?;

        // Try to read existing count
        let existing: Option<i64> = tx
            .query_row(
                "SELECT count FROM card_confusions WHERE card_id = ?1 AND mistaken_card_id = ?2",
                params![card_id, mistaken_with],
                |r| r.get(0),
            )
            .optional()
            .context("Failed to query existing confusion.")?;

        match existing {
            Some(old) => {
                let new = old + delta;
                if new > 0 {
                    tx.execute(
                        "UPDATE card_confusions SET count = ?1 WHERE card_id = ?2 AND mistaken_card_id = ?3",
                        params![new, card_id, mistaken_with],
                    )
                    .with_context(|| format!("Failed to update confusion for card {} mistaken_with {}.", card_id, mistaken_with))?;
                } else {
                    tx.execute(
                        "DELETE FROM card_confusions WHERE card_id = ?1 AND mistaken_card_id = ?2",
                        params![card_id, mistaken_with],
                    )
                    .with_context(|| {
                        format!(
                            "Failed to delete confusion for card {} mistaken_with {}.",
                            card_id, mistaken_with
                        )
                    })?;
                }
            }
            None => {
                if delta > 0 {
                    // Insert a new row with count = delta
                    tx.execute(
                        "INSERT INTO card_confusions (card_id, mistaken_card_id, count) VALUES (?1, ?2, ?3)",
                        params![card_id, mistaken_with, delta],
                    )
                    .with_context(|| {
                        format!(
                            "Failed to insert confusion for card {} mistaken_with {}.",
                            card_id, mistaken_with
                        )
                    })?;
                }
            }
        }

        tx.commit()
            .context("Failed to commit adjust_confusion transaction.")?;
        Ok(())
    }

    /// Return recorded confusions for a card: Vec<(mistaken_card_id, count)> ordered by count desc.
    pub fn get_confusions(&self, card_id: i64) -> Result<Vec<(i64, i64)>> {
        let mut stmt = self.conn.prepare(
                "SELECT mistaken_card_id, count FROM card_confusions WHERE card_id = ?1 ORDER BY count DESC",
            ).context("Failed to prepare get_confusions.")?;
        let rows = stmt
            .query_map(params![card_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .context("Failed to query confusions.")?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.context("Failed to map confusion row.")?);
        }
        Ok(out)
    }

    /// Get current learning_score for a card (reads card_stats.learning_score).
    pub fn get_card_learning_score(&self, card_id: i64) -> Result<i64> {
        let score: i64 = self
            .conn
            .query_row(
                "SELECT learning_score FROM card_stats WHERE card_id = ?1",
                params![card_id],
                |r| r.get(0),
            )
            .with_context(|| format!("Failed to get learning_score for card {}.", card_id))?;
        Ok(score)
    }

    /// Get current SM-2 stats for a card.
    pub fn _get_card_sm2_stats(&self, card_id: i64) -> Result<crate::core::learn::SM2Stats> {
        use crate::core::learn::SM2Stats;
        self.conn
            .query_row(
                "SELECT interval, repetitions, easiness_factor FROM card_stats WHERE card_id = ?1",
                params![card_id],
                |r| {
                    Ok(SM2Stats {
                        interval: r.get(0)?,
                        repetitions: r.get(1)?,
                        easiness_factor: r.get(2)?,
                    })
                },
            )
            .with_context(|| format!("Failed to get SM2 stats for card {}.", card_id))
    }

    /// Get cards in the positive learning set for a deck (learning_score > 0)
    pub fn _get_positive_cards(&self, deck_id: i64) -> Result<Vec<Card>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT c.id, c.deck_id, c.term, c.definition
             FROM cards c
             JOIN card_stats s ON c.id = s.card_id
             WHERE c.deck_id = ?1 AND s.learning_score > 0
             ORDER BY s.learning_score DESC",
            )
            .context("Failed to prepare get_positive_cards statement.")?;
        let rows = stmt
            .query_map(params![deck_id], |r| {
                Ok(Card {
                    id: r.get(0)?,
                    term: r.get(2)?,
                    definition: r.get(3)?,
                })
            })
            .context("Failed to query_map positive cards.")?;

        let mut out = Vec::new();
        for r in rows {
            out.push(r.context("Failed to map positive card row.")?);
        }
        Ok(out)
    }

    /// Update persistent currency in user_profile (positive or negative delta)
    pub fn update_currency(&mut self, delta: i64) -> Result<()> {
        self.conn
            .execute(
                "UPDATE user_profile SET currency = currency + ?1, updated_at = ?2 WHERE id = 1",
                params![delta, now_secs()],
            )
            .context("Failed to update currency.")?;
        Ok(())
    }

    /// Read current currency
    pub fn get_currency(&self) -> Result<i64> {
        self.conn
            .query_row("SELECT currency FROM user_profile WHERE id = 1", [], |r| {
                r.get(0)
            })
            .context("Failed to query user currency.")
    }

    /// Update persistent gauntlet streak in user_profile (positive or negative delta)
    pub fn update_streak(&mut self, delta: i64) -> Result<()> {
        self.conn
            .execute(
                "UPDATE user_profile SET streak = streak + ?1, updated_at = ?2 WHERE id = 1",
                params![delta, now_secs()],
            )
            .context("Failed to update streak.")?;
        Ok(())
    }

    /// Read current streak
    pub fn get_streak(&self) -> Result<i64> {
        self.conn
            .query_row("SELECT streak FROM user_profile WHERE id = 1", [], |r| {
                r.get(0)
            })
            .context("Failed to query user streak.")
    }

    /// Return count of cards in a deck.
    pub fn get_deck_card_count(&self, deck_id: i64) -> Result<i64> {
        self.conn
            .query_row(
                "SELECT count(*) FROM cards WHERE deck_id = ?1",
                params![deck_id],
                |r| r.get(0),
            )
            .context("Failed to count cards in deck.")
    }

    /// Summarize stats for a deck: New (0 reps), Learning (1-6 interval), Mature (>=7 interval).
    pub fn get_deck_stats_summary(&self, deck_id: i64) -> Result<DeckStatsSummary> {
        self.conn
            .query_row(
                "SELECT
                    COUNT(*),
                    SUM(CASE WHEN s.repetitions = 0 THEN 1 ELSE 0 END),
                    SUM(CASE WHEN s.repetitions > 0 AND s.interval < 7 THEN 1 ELSE 0 END),
                    SUM(CASE WHEN s.interval >= 7 THEN 1 ELSE 0 END),
                    AVG(s.easiness_factor)
                 FROM cards c
                 JOIN card_stats s ON c.id = s.card_id
                 WHERE c.deck_id = ?1",
                params![deck_id],
                |r| {
                    Ok(DeckStatsSummary {
                        total_cards: r.get(0)?,
                        new_count: r.get::<_, Option<i64>>(1)?.unwrap_or(0),
                        learning_count: r.get::<_, Option<i64>>(2)?.unwrap_or(0),
                        mature_count: r.get::<_, Option<i64>>(3)?.unwrap_or(0),
                        average_easiness: r.get::<_, Option<f64>>(4)?.unwrap_or(2.5),
                    })
                },
            )
            .context("Failed to aggregate deck stats.")
    }

    /// Return paginated card stats for a deck.
    pub fn get_cards_paginated(
        &self,
        deck_id: i64,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<CardStatRow>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT c.id, c.term, c.definition, s.learning_score, s.interval, s.easiness_factor, s.next_due
                 FROM cards c
                 JOIN card_stats s ON c.id = s.card_id
                 WHERE c.deck_id = ?1
                 ORDER BY c.id
                 LIMIT ?2 OFFSET ?3",
            )
            .context("Failed to prepare get_cards_paginated statement.")?;

        let rows = stmt
            .query_map(params![deck_id, limit, offset], |r| {
                Ok(CardStatRow {
                    card_id: r.get(0)?,
                    term: r.get(1)?,
                    definition: r.get(2)?,
                    learning_score: r.get(3)?,
                    interval: r.get(4)?,
                    easiness: r.get(5)?,
                    next_due: r.get(6)?,
                })
            })
            .context("Failed to query paginated cards.")?;

        let mut out = Vec::new();
        for r in rows {
            out.push(r.context("Failed mapping card row.")?);
        }
        Ok(out)
    }

    /// Top N "leech" cards: those with the most incorrect answers for a deck.
    pub fn get_leech_cards(&self, deck_id: i64, limit: u32) -> Result<Vec<(String, i64)>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT c.term, s.incorrect_count
             FROM cards c
             JOIN card_stats s ON c.id = s.card_id
             WHERE c.deck_id = ?1 AND s.incorrect_count > 0
             ORDER BY s.incorrect_count DESC
             LIMIT ?2",
            )
            .context("Failed to prepare get_leech_cards statement.")?;

        let rows = stmt
            .query_map(params![deck_id, limit], |r| Ok((r.get(0)?, r.get(1)?)))
            .context("Failed to query leech cards.")?;

        let mut out = Vec::new();
        for r in rows {
            out.push(r.context("Failed mapping leech card row.")?);
        }
        Ok(out)
    }
}

/// Initialize the database connection: pragmas and schema
pub fn init_db(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "foreign_keys", "ON")
        .context("failed to enable foreign_keys")?;
    let _ = conn.pragma_update(None, "journal_mode", "WAL");

    conn.execute_batch(SCHEMA)
        .context("Failed to execute schema SQL")?;

    conn.execute(
        "INSERT OR IGNORE INTO user_profile (id, currency) VALUES (1, 0);",
        [],
    )
    .context("Failed to ensure user_profile row.")?;

    // migration for adding 'streak' column because I'm stupid
    let has_streak: Option<String> = conn
        .query_row(
            "SELECT name FROM pragma_table_info('user_profile') WHERE name = 'streak'",
            [],
            |r| r.get(0),
        )
        .optional()
        .context("Failed to check for streak column in user_profile.")?;

    if has_streak.is_none() {
        conn.execute(
            "ALTER TABLE user_profile ADD COLUMN streak INTEGER NOT NULL DEFAULT 0",
            [],
        )
        .context("Failed to add streak column to user_profile.")?;
    }

    // migration for SM-2 columns in card_stats
    let sm2_cols = ["interval", "repetitions", "easiness_factor", "next_due"];
    for col in sm2_cols {
        let has_col: Option<String> = conn
            .query_row(
                &format!(
                    "SELECT name FROM pragma_table_info('card_stats') WHERE name = '{}'",
                    col
                ),
                [],
                |r| r.get(0),
            )
            .optional()
            .context("Failed to check for column in card_stats.")?;

        if has_col.is_none() {
            let sql = match col {
                "interval" => {
                    "ALTER TABLE card_stats ADD COLUMN interval INTEGER NOT NULL DEFAULT 0"
                }
                "repetitions" => {
                    "ALTER TABLE card_stats ADD COLUMN repetitions INTEGER NOT NULL DEFAULT 0"
                }
                "easiness_factor" => {
                    "ALTER TABLE card_stats ADD COLUMN easiness_factor REAL NOT NULL DEFAULT 2.5"
                }
                "next_due" => {
                    "ALTER TABLE card_stats ADD COLUMN next_due INTEGER NOT NULL DEFAULT 0"
                }
                _ => continue,
            };
            conn.execute(sql, [])
                .with_context(|| format!("Failed to add {} column to card_stats.", col))?;
        }
    }

    Ok(())
}

pub fn get_deck(src: DeckSource, storage: &Storage) -> anyhow::Result<Deck> {
    match src {
        DeckSource::Named(n) => storage.get_deck_by_name(&n),
        DeckSource::File(p) => read_deck_from_file(p),
    }
}
