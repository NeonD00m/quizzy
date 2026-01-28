use crate::core::deck::{Card, Deck, DeckSource, read_deck_from_file};
use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{Connection, OpenFlags, params};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

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
    last_answered_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_card_stats_learning_score ON card_stats(learning_score);

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
            .context("failed to query user_profile.updated_at")?;
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
            std::fs::read_dir(&dir).context("failed to read DB directory for failed sessions")?
        {
            let entry = entry.context("failed to read directory entry")?;
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
    /// Format expected: each line `card_id,corrects,incorrects`
    pub fn read_failed_session_file(&self, path: &std::path::Path) -> Result<Vec<(i64, i64, i64)>> {
        let s = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read failed session file {}", path.display()))?;
        let mut out = Vec::new();
        for (line_number, line) in s.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() != 3 {
                return Err(anyhow::anyhow!(
                    "invalid format in {} at line {}",
                    path.display(),
                    line_number + 1
                ));
            }
            let a: i64 = parts[0].trim().parse().with_context(|| {
                format!(
                    "invalid card_id in {} line {}",
                    path.display(),
                    line_number + 1
                )
            })?;
            let b: i64 = parts[1].trim().parse().with_context(|| {
                format!(
                    "invalid corrects in {} line {}",
                    path.display(),
                    line_number + 1
                )
            })?;
            let c: i64 = parts[2].trim().parse().with_context(|| {
                format!(
                    "invalid incorrects in {} line {}",
                    path.display(),
                    line_number + 1
                )
            })?;
            out.push((a, b, c));
        }
        Ok(out)
    }

    /// Remove a failed session file after replay or if user discards it.
    pub fn remove_failed_session_file(&self, path: &Path) -> Result<()> {
        std::fs::remove_file(path)
            .with_context(|| format!("failed to remove failed session file {}", path.display()))?;
        Ok(())
    }

    /// Open or create the DB at the canonical path and initialize schema.
    pub fn open_default() -> Result<Self> {
        let path = db_path_from_env_or_default();

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create database parent directory {:?}", parent)
            })?;
        }

        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE;
        let conn = Connection::open_with_flags(&path, flags)
            .with_context(|| format!("failed to open sqlite database at {:?}", path))?;

        conn.busy_timeout(std::time::Duration::from_secs(5))
            .context("failed to set busy_timeout on sqlite connection")?;

        // initialize schema and pragmas
        init_db(&conn).context("failed to initialize database schema")?;

        Ok(Self { conn })
    }

    /// List decks (id, name)
    pub fn list_decks(&self) -> Result<Vec<(i64, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name FROM decks ORDER BY name")
            .context("failed to prepare list_decks")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .context("failed to query decks")?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.context("failed mapping deck row")?);
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
        ).context("failed to insert deck row")?;
        let deck_id = self.conn.last_insert_rowid();

        let tx = self
            .conn
            .transaction()
            .context("failed to start transaction for deck insert")?;
        for c in deck.cards {
            tx.execute(
                "INSERT INTO cards (deck_id, term, definition, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)",
                params![deck_id, c.term, c.definition, now],
            ).context("failed to insert card")?;
            let card_id = tx.last_insert_rowid();
            tx.execute(
                "INSERT INTO card_stats (card_id) VALUES (?1)",
                params![card_id],
            )
            .context("failed to insert card_stats")?;
        }
        tx.execute(
            "INSERT INTO deck_stats (deck_id) VALUES (?1)",
            params![deck_id],
        )
        .context("failed to insert deck_stats")?;
        tx.commit()
            .context("failed to commit create_deck transaction")?;

        Ok(deck_id)
    }

    /// Add a single card to a deck
    pub fn add_card_to_deck(&mut self, deck_id: i64, term: &str, definition: &str) -> Result<i64> {
        let now = now_secs();
        self.conn.execute(
            "INSERT INTO cards (deck_id, term, definition, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)",
            params![deck_id, term, definition, now],
        ).context("failed to insert card")?;
        let card_id = self.conn.last_insert_rowid();
        self.conn
            .execute(
                "INSERT INTO card_stats (card_id) VALUES (?1)",
                params![card_id],
            )
            .context("failed to insert card_stats")?;
        Ok(card_id)
    }

    /// Remove a card by id
    pub fn remove_card(&mut self, card_id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM cards WHERE id = ?1", params![card_id])
            .context("failed to delete card")?;
        Ok(())
    }

    /// Get a deck by name (returns deck with card ids populated)
    pub fn get_deck_by_name(&self, name: &str) -> Result<Deck> {
        let deck_id: i64 = self
            .conn
            .query_row("SELECT id FROM decks WHERE name = ?1", params![name], |r| {
                r.get(0)
            })
            .with_context(|| format!("deck named '{}' not found", name))?;
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
            .context("failed to query deck metadata")?;

        let mut stmt = self
            .conn
            .prepare("SELECT id, term, definition FROM cards WHERE deck_id = ?1 ORDER BY id")
            .context("failed to prepare select cards for deck")?;
        let rows = stmt
            .query_map(params![deck_id], |r| {
                Ok(Card {
                    id: r.get(0)?,
                    term: r.get(1)?,
                    definition: r.get(2)?,
                })
            })
            .context("failed to query_map cards")?;

        let mut cards = Vec::new();
        for r in rows {
            cards.push(r.context("failed mapping card row")?);
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
            .context("failed to delete deck")?;
        Ok(())
    }

    /// Delete a deck by name (so much more convenient)
    pub fn delete_deck_by_name(&mut self, name: &str) -> Result<()> {
        let deck_id: i64 = self
            .conn
            .query_row("SELECT id FROM decks WHERE name = ?1", params![name], |r| {
                r.get(0)
            })
            .with_context(|| format!("deck '{}' not found", name))?;
        self.delete_deck_by_id(deck_id)
    }

    /// Immediate update for a single answer (durable).
    /// correct: true => +3 learning_score and +1 correct_count; false => -1 learning_score and +1 incorrect_count
    pub fn record_answer_immediate(&mut self, card_id: i64, correct: bool) -> Result<()> {
        let now = now_secs();
        let (score_delta, correct_delta, incorrect_delta) =
            if correct { (3, 1, 0) } else { (-1, 0, 1) };

        let tx = self
            .conn
            .transaction()
            .context("failed to start transaction")?;

        tx.execute(
            "UPDATE card_stats
             SET learning_score = learning_score + ?1,
                 correct_count = correct_count + ?2,
                 incorrect_count = incorrect_count + ?3,
                 last_answered_at = ?4
             WHERE card_id = ?5",
            params![score_delta, correct_delta, incorrect_delta, now, card_id],
        )
        .context("failed to update card_stats")?;

        let deck_id: i64 = tx
            .query_row(
                "SELECT deck_id FROM cards WHERE id = ?1",
                params![card_id],
                |r| r.get(0),
            )
            .context("failed to lookup deck_id for card")?;

        tx.execute(
            "UPDATE deck_stats
             SET questions_answered_total = questions_answered_total + 1,
                 questions_correct_total = questions_correct_total + ?1,
                 last_studied_at = ?2
             WHERE deck_id = ?3",
            params![if correct { 1 } else { 0 }, now, deck_id],
        )
        .context("failed to update deck_stats")?;

        tx.commit().context("failed to commit transaction")?;
        Ok(())
    }

    /// Batch commit at the end of a learning session.
    /// `updates` is a slice of tuples: (card_id, corrects_delta, incorrects_delta)
    pub fn commit_session_batch(&mut self, updates: &[(i64, i64, i64)]) -> Result<()> {
        if updates.is_empty() {
            return Ok(());
        }

        let now = now_secs();
        let tx = self
            .conn
            .transaction()
            .context("failed to start transaction")?;

        let mut deck_deltas: HashMap<i64, (i64, i64)> = HashMap::new(); // deck_id -> (questions_total_delta, questions_correct_delta)

        for (card_id, corrects, incorrects) in updates {
            let score_delta = CORRECT_ANSWER_SCORE * corrects - INCORRECT_ANSWER_SCORE * incorrects;
            tx.execute(
                "UPDATE card_stats
                 SET learning_score = learning_score + ?1,
                     correct_count = correct_count + ?2,
                     incorrect_count = incorrect_count + ?3,
                     last_answered_at = ?4
                 WHERE card_id = ?5",
                params![score_delta, corrects, incorrects, now, card_id],
            )
            .with_context(|| format!("failed to update card_stats for card_id {}", card_id))?;

            let deck_id: i64 = tx
                .query_row(
                    "SELECT deck_id FROM cards WHERE id = ?1",
                    params![card_id],
                    |r| r.get(0),
                )
                .with_context(|| format!("failed to lookup deck_id for card_id {}", card_id))?;

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
            .with_context(|| format!("failed to update deck_stats for deck_id {}", deck_id))?;
        }

        tx.commit().context("failed to commit batch transaction")?;
        Ok(())
    }

    /// Record that `mistaken_with` was chosen when asking about `card_id`.
    /// This increments the confusion count for (card_id,mistaken_with).
    pub fn record_confusion(&mut self, card_id: i64, mistaken_with: i64) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO card_confusions (card_id, mistaken_card_id, count)
                     VALUES (?1, ?2, 1)
                     ON CONFLICT(card_id, mistaken_card_id) DO UPDATE SET count = count + 1",
                params![card_id, mistaken_with],
            )
            .with_context(|| {
                format!(
                    "failed to insert/update confusion for card {} mistaken_with {}",
                    card_id, mistaken_with
                )
            })?;
        Ok(())
    }

    /// Mark that a previous confusion for (card_id, mistaken_with) has been corrected.
    /// If `new_score` > 0, set `count = new_score`. If `new_score` <= 0, remove the confusion row.
    pub fn correct_confusion(
        &mut self,
        card_id: i64,
        mistaken_with: i64,
        new_score: i64,
    ) -> Result<()> {
        if new_score > 0 {
            self.conn
                .execute(
                    "UPDATE card_confusions
                 SET count = ?1
                 WHERE card_id = ?2 AND mistaken_card_id = ?3",
                    params![new_score, card_id, mistaken_with],
                )
                .with_context(|| {
                    format!(
                        "failed to update confusion for card {} mistaken_with {}",
                        card_id, mistaken_with
                    )
                })?;
        } else {
            // remove the confusion entry entirely if the new score is non-positive
            self.conn
                .execute(
                    "DELETE FROM card_confusions WHERE card_id = ?1 AND mistaken_card_id = ?2",
                    params![card_id, mistaken_with],
                )
                .with_context(|| {
                    format!(
                        "failed to delete confusion for card {} mistaken_with {}",
                        card_id, mistaken_with
                    )
                })?;
        }
        Ok(())
    }

    /// Return recorded confusions for a card: Vec<(mistaken_card_id, count)> ordered by count desc.
    pub fn get_confusions(&self, card_id: i64) -> Result<Vec<(i64, i64)>> {
        let mut stmt = self.conn.prepare(
                "SELECT mistaken_card_id, count FROM card_confusions WHERE card_id = ?1 ORDER BY count DESC",
            ).context("failed to prepare get_confusions")?;
        let rows = stmt
            .query_map(params![card_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .context("failed to query confusions")?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.context("failed to map confusion row")?);
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
            .context(format!("failed to get learning_score for card {}", card_id))?;
        Ok(score)
    }

    /// Get cards in the positive learning set for a deck (learning_score > 0)
    pub fn get_positive_cards(&self, deck_id: i64) -> Result<Vec<Card>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT c.id, c.deck_id, c.term, c.definition
             FROM cards c
             JOIN card_stats s ON c.id = s.card_id
             WHERE c.deck_id = ?1 AND s.learning_score > 0
             ORDER BY s.learning_score DESC",
            )
            .context("failed to prepare get_positive_cards statement")?;
        let rows = stmt
            .query_map(params![deck_id], |r| {
                Ok(Card {
                    id: r.get(0)?,
                    term: r.get(2)?,
                    definition: r.get(3)?,
                })
            })
            .context("failed to query_map positive cards")?;

        let mut out = Vec::new();
        for r in rows {
            out.push(r.context("failed to map positive card row")?);
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
            .context("failed to update currency")?;
        Ok(())
    }

    /// Read current currency
    pub fn get_currency(&self) -> Result<i64> {
        self.conn
            .query_row("SELECT currency FROM user_profile WHERE id = 1", [], |r| {
                r.get(0)
            })
            .context("failed to query user currency")
    }
}

/// Initialize the database connection: pragmas and schema
pub fn init_db(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "foreign_keys", "ON")
        .context("failed to enable foreign_keys")?;
    let _ = conn.pragma_update(None, "journal_mode", "WAL");

    conn.execute_batch(SCHEMA)
        .context("failed to execute schema SQL")?;

    conn.execute(
        "INSERT OR IGNORE INTO user_profile (id, currency) VALUES (1, 0);",
        [],
    )
    .context("failed to ensure user_profile row")?;

    Ok(())
}

pub fn get_deck(src: DeckSource, storage: &Storage) -> anyhow::Result<Deck> {
    match src {
        DeckSource::Named(n) => storage.get_deck_by_name(&n),
        DeckSource::File(p) => read_deck_from_file(p),
    }
}
