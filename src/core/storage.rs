use crate::core::deck::{Card, Deck, DeckSource, read_deck_from_file};
use crate::core::migrations::run_migrations;
use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::OptionalExtension;
use rusqlite::{Connection, OpenFlags, params};
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

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct FSRSStats {
    pub stability: f64,
    pub difficulty: f64,
    pub repetition_count: i64,
    pub last_review: i64,
    pub next_due: i64,
    pub lapses: i64,
    pub state: u8,
}

impl Default for FSRSStats {
    fn default() -> Self {
        Self {
            stability: 0.0,
            difficulty: 0.0,
            repetition_count: 0,
            last_review: 0,
            next_due: 0,
            lapses: 0,
            state: 0,
        }
    }
}

pub struct CardStatRow {
    pub card_id: i64,
    pub term: String,
    pub definition: String,
    pub learning_score: i64,
    pub fsrs: Option<FSRSStats>,
    #[allow(dead_code)]
    pub correct_count: i64,
    #[allow(dead_code)]
    pub incorrect_count: i64,
}

pub struct DeckListItem {
    pub id: i64,
    pub name: String,
    pub created_at: i64,
    pub card_count: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct DeckDashboardItem {
    #[allow(dead_code)]
    pub id: i64,
    pub name: String,
    pub total_cards: i64,
    pub due_cards: i64,
    pub new_cards: i64,
    #[allow(dead_code)]
    pub last_studied_at: Option<i64>,
    pub next_due_at: Option<i64>,
}

// ============================= Schema =============================

/// Base schema applied on first open. Subsequent structural changes are
/// handled by versioned migrations in `core::migrations`.
const SCHEMA: &str = r#"
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS decks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    description TEXT,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    source_path TEXT
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
    stability REAL NOT NULL DEFAULT 0.0,
    difficulty REAL NOT NULL DEFAULT 0.0,
    repetition_count INTEGER NOT NULL DEFAULT 0,
    last_review INTEGER NOT NULL DEFAULT 0,
    next_due INTEGER NOT NULL DEFAULT 0,
    lapses INTEGER NOT NULL DEFAULT 0,
    state INTEGER NOT NULL DEFAULT 0
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
    streak INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);
"#;

// ===================== Storage Struct & Lifecycle =====================

/// Wrapper around a rusqlite connection exposing the repository API.
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
    #[allow(dead_code)]
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

    /// Return the maximum `last_studied_at` from `deck_stats` (if present)
    pub fn get_user_last_studied(&self) -> Result<Option<i64>> {
        use rusqlite::OptionalExtension;
        let val: Option<i64> = self
            .conn
            .query_row("SELECT MAX(last_studied_at) FROM deck_stats", [], |r| {
                r.get(0)
            })
            .optional()
            .context("Failed to query MAX(last_studied_at) in deck_stats.")?;
        // If MAX returns NULL, optional() might still return Some(None) depending on rusqlite behavior
        // Actually MAX() on empty set or all NULLs returns NULL in SQLite.
        Ok(val)
    }

    /// Update the `updated_at` timestamp in `user_profile` to now.
    pub fn update_user_last_active(&mut self) -> Result<()> {
        self.conn
            .execute(
                "UPDATE user_profile SET updated_at = ?1 WHERE id = 1",
                params![now_secs()],
            )
            .context("Failed to update user_profile updated_at.")?;
        Ok(())
    }

    /// Update the `last_studied_at` timestamp for a specific deck in `deck_stats` to now.
    pub fn update_deck_last_studied(&mut self, deck_id: i64) -> Result<()> {
        self.conn
            .execute(
                "UPDATE deck_stats SET last_studied_at = ?1 WHERE deck_id = ?2",
                params![now_secs(), deck_id],
            )
            .context("Failed to update deck_stats last_studied_at.")?;
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

    // ========================== Deck CRUD ==========================

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

    /// List decks with metadata (id, name, created_at, card_count)
    pub fn list_decks_detailed(&self) -> Result<Vec<DeckListItem>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT d.id, d.name, d.created_at, COUNT(c.id), d.updated_at
                 FROM decks d
                 LEFT JOIN cards c ON d.id = c.deck_id
                 GROUP BY d.id
                 ORDER BY d.name",
            )
            .context("Failed to prepare list_decks_detailed.")?;
        let rows = stmt
            .query_map([], |r| {
                Ok(DeckListItem {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    created_at: r.get(2)?,
                    card_count: r.get(3)?,
                    updated_at: r.get(4)?,
                })
            })
            .context("Failed to query detailed decks.")?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.context("Failed mapping detailed deck row.")?);
        }
        Ok(out)
    }

    /// Create a deck and persist all cards in a single transaction.
    /// Returns the new deck id.
    pub fn create_deck_from_core(
        &mut self,
        deck: Deck,
        source_path: Option<&str>,
    ) -> Result<(i64, String)> {
        let now = now_secs();
        let tx = self
            .conn
            .transaction()
            .context("Failed to start transaction for deck creation.")?;
        tx.execute(
            "INSERT INTO decks (name, description, created_at, updated_at, source_path) VALUES (?1, ?2, ?3, ?3, ?4)",
            params![deck.name, None::<&str>, now, source_path],
        ).context("Failed to insert deck row.")?;
        let deck_id = tx.last_insert_rowid();
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

        Ok((deck_id, deck.name))
    }

    /// Add a single card to a deck
    pub fn add_card_to_deck(&mut self, deck_id: i64, term: &str, definition: &str) -> Result<()> {
        let now = now_secs();
        let tx = self
            .conn
            .transaction()
            .context("Failed to start transaction for single card insert.")?;
        tx.execute(
            "INSERT INTO cards (deck_id, term, definition, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)",
            params![deck_id, term, definition, now],
        ).context("Failed to insert card.")?;
        let card_id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO card_stats (card_id) VALUES (?1)",
            params![card_id],
        )
        .context("Failed to insert card_stats.")?;
        tx.execute(
            "UPDATE decks SET updated_at = ?1 WHERE id = ?2",
            params![now, deck_id],
        )
        .context("Failed to update deck updated_at.")?;
        tx.commit()
            .context("Failed to commit single card insert transaction.")?;
        Ok(())
    }

    /// Add multiple cards to a deck in a single transaction
    pub fn add_cards_to_deck_batch(
        &mut self,
        deck_id: i64,
        cards: Vec<Card>,
        clear: bool,
    ) -> Result<()> {
        let now = now_secs();
        let tx = self
            .conn
            .transaction()
            .context("Failed to start transaction for batch card insert.")?;
        if clear {
            tx.execute("DELETE FROM cards WHERE deck_id = ?1", params![deck_id])
                .context("Failed to clear cards from deck.")?;
            tx
                .execute(
                    "UPDATE deck_stats SET questions_answered_total = 0, questions_correct_total = 0, last_studied_at = NULL WHERE deck_id = ?1",
                    params![deck_id],
                )
                .context("Failed to reset deck stats.")?;
        }
        for c in cards {
            tx.execute(
                "INSERT INTO cards (deck_id, term, definition, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)",
                params![deck_id, c.term, c.definition, now],
            ).context("Failed to insert card in batch.")?;
            let card_id = tx.last_insert_rowid();
            tx.execute(
                "INSERT INTO card_stats (card_id) VALUES (?1)",
                params![card_id],
            )
            .context("Failed to insert card_stats in batch.")?;
        }
        tx.execute(
            "UPDATE decks SET updated_at = ?1 WHERE id = ?2",
            params![now, deck_id],
        )
        .context("Failed to update deck updated_at.")?;
        tx.commit()
            .context("Failed to commit batch card insert transaction.")?;
        Ok(())
    }

    /// Remove a card by id
    pub fn remove_card(&mut self, card_id: i64) -> Result<()> {
        let tx = self
            .conn
            .transaction()
            .context("Failed to start transaction for card remove.")?;
        let deck_id: i64 = tx
            .query_row(
                "SELECT deck_id FROM cards WHERE id = ?1",
                params![card_id],
                |r| r.get(0),
            )
            .context("Failed to lookup deck_id for card.")?;
        tx.execute(
            "UPDATE decks SET updated_at = ?1 WHERE id = ?2",
            params![now_secs(), deck_id],
        )
        .context("Failed to update deck updated_at.")?;
        tx.execute("DELETE FROM cards WHERE id = ?1", params![card_id])
            .context("Failed to delete card.")?;
        tx.commit()
            .context("Failed to commit card remove transaction.")?;
        Ok(())
    }

    /// Remove all cards from a deck
    pub fn clear_deck(&mut self, deck_id: i64) -> Result<()> {
        let tx = self
            .conn
            .transaction()
            .context("Failed to start transaction for clearing deck.")?;
        tx.execute("DELETE FROM cards WHERE deck_id = ?1", params![deck_id])
            .context("Failed to clear cards from deck.")?;
        // Also clear deck stats
        tx
            .execute(
                "UPDATE deck_stats SET questions_answered_total = 0, questions_correct_total = 0, last_studied_at = NULL WHERE deck_id = ?1",
                params![deck_id],
            )
            .context("Failed to reset deck stats.")?;
        tx.execute(
            "UPDATE decks SET updated_at = ?1 WHERE id = ?2",
            params![now_secs(), deck_id],
        )
        .context("Failed to update deck updated_at.")?;
        tx.commit()
            .context("Failed to commit card remove transaction.")?;
        Ok(())
    }

    /// Rename a deck
    pub fn rename_deck(&mut self, deck_id: i64, new_name: &str) -> Result<()> {
        let now = now_secs();
        self.conn
            .execute(
                "UPDATE decks SET name = ?1, updated_at = ?2 WHERE id = ?3",
                params![new_name, now, deck_id],
            )
            .context("Failed to rename deck.")?;
        Ok(())
    }

    /// Return the `source_path` recorded for a deck (if any).
    pub fn get_deck_source_path(&self, deck_id: i64) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT source_path FROM decks WHERE id = ?1",
                params![deck_id],
                |r| r.get(0),
            )
            .optional()
            .context("Failed to query source_path for deck.")
    }

    /// Find a deck by name if it exists (returns deck with card ids populated).
    pub fn find_deck_by_name(&self, name: &str) -> Result<Option<Deck>> {
        let deck_id: Option<i64> = self
            .conn
            .query_row("SELECT id FROM decks WHERE name = ?1", params![name], |r| {
                r.get(0)
            })
            .optional()
            .context("Failed to query deck by name.")?;
        match deck_id {
            Some(id) => self.get_deck_by_id(id).map(Some),
            None => Ok(None),
        }
    }

    /// Get a deck by name (returns deck with card ids populated)
    pub fn get_deck_by_name(&self, name: &str) -> Result<Deck> {
        self.find_deck_by_name(name)?
            .ok_or_else(|| anyhow::anyhow!("Deck named '{}' not found.", name))
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

    /// Delete a deck by id and its associated stats
    pub fn delete_deck_by_id(&mut self, deck_id: i64) -> Result<()> {
        let tx = self
            .conn
            .transaction()
            .context("Failed to start transaction for deleting deck.")?;
        tx.execute(
            "DELETE FROM deck_stats WHERE deck_id = ?1",
            params![deck_id],
        )
        .context("Failed to delete deck_stats.")?;
        tx.execute(
            "DELETE FROM card_stats WHERE card_id IN (SELECT id FROM cards WHERE deck_id = ?1)",
            params![deck_id],
        )
        .context("Failed to delete card_stats.")?;
        tx.execute("DELETE FROM decks WHERE id = ?1", params![deck_id])
            .context("Failed to delete deck.")?;
        tx.commit()
            .context("Failed to commit transaction for deleting deck.")?;
        Ok(())
    }

    // ========================= Card CRUD ==========================

    // (add_card_to_deck, add_cards_to_deck_batch, remove_card, clear_deck,
    //  update_card are above in the file — already grouped below Deck CRUD)

    // ======================== Session Commits ======================

    /// Test mode commit: Updates all-time counts and learning_score (+2 / -1).
    pub fn commit_test_session(
        &self,
        updates: &[(i64, i64, i64)], // (card_id, corrects, incorrects)
    ) -> anyhow::Result<()> {
        let tx = self.conn.unchecked_transaction()?;

        for &(card_id, corrects, incorrects) in updates {
            // Calculate learning_score delta (+2 for correct, -1 for incorrect)
            let score_delta = (corrects * 2) - incorrects;

            tx.execute(
                "INSERT INTO card_stats (card_id, learning_score, correct_count, incorrect_count)
                    VALUES (?1, ?2, ?3, ?4)
                    ON CONFLICT(card_id) DO UPDATE SET
                    learning_score = learning_score + ?2,
                    correct_count = correct_count + ?3,
                    incorrect_count = incorrect_count + ?4",
                params![card_id, score_delta, corrects, incorrects],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    /// Cram mode commit: Updates learning_score with lighter weights (+1 / -1).
    pub fn commit_cram_session(
        &self,
        updates: &[(i64, i64)], // (card_id, score_delta) where score_delta = (corrects * 1) - (incorrects * 1)
    ) -> anyhow::Result<()> {
        let tx = self.conn.unchecked_transaction()?;

        for &(card_id, score_delta) in updates {
            tx.execute(
                "INSERT INTO card_stats (card_id, learning_score)
                    VALUES (?1, ?2)
                    ON CONFLICT(card_id) DO UPDATE SET
                    learning_score = learning_score + ?2",
                params![card_id, score_delta],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    /// Learn mode commit: Updates FSRS metrics, last_review/next_due, all-time stats, and learning_score.
    pub fn commit_learn_session(
        &self,
        updates: &[(i64, FSRSStats, i64, i64, i64)], // (card_id, fsrs_stats, corrects, incorrects, score_delta)
    ) -> anyhow::Result<()> {
        let tx = self.conn.unchecked_transaction()?;

        for (card_id, fsrs, corrects, incorrects, score_delta) in updates {
            tx.execute(
                "INSERT INTO card_stats (
                    card_id, learning_score, stability, difficulty,
                    repetition_count, last_review, next_due, lapses, state, correct_count, incorrect_count
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                ON CONFLICT(card_id) DO UPDATE SET
                    learning_score = learning_score + ?2,
                    stability = ?3,
                    difficulty = ?4,
                    repetition_count = ?5,
                    last_review = ?6,
                    next_due = ?7,
                    lapses = ?8,
                    state = ?9,
                    correct_count = correct_count + ?10,
                    incorrect_count = incorrect_count + ?11",
                params![
                    card_id,
                    score_delta,
                    fsrs.stability,
                    fsrs.difficulty,
                    fsrs.repetition_count,
                    fsrs.last_review,
                    fsrs.next_due,
                    fsrs.lapses,
                    fsrs.state,
                    corrects,
                    incorrects
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
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

    // ========================== Stats Reads =========================

    /// Cram mode: Fetch cards for a deck ordered by lowest learning_score first.
    pub fn get_weakest_cards(
        &self,
        deck_id: i64,
        limit: usize,
    ) -> anyhow::Result<Vec<(Card, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.id, c.term, c.definition, COALESCE(s.learning_score, 0) as score
                FROM cards c
                LEFT JOIN card_stats s ON c.id = s.card_id
                WHERE c.deck_id = ?1
                ORDER BY score ASC
                LIMIT ?2",
        )?;

        let cards = stmt
            .query_map(params![deck_id, limit as i64], |row| {
                let card = Card {
                    id: Some(row.get(0)?),
                    term: row.get(1)?,
                    definition: row.get(2)?,
                };
                let score: i64 = row.get(3)?;
                Ok((card, score))
            })?
            .filter_map(Result::ok)
            .collect();

        Ok(cards)
    }

    /// Learn mode (FSRS): Fetch cards for a deck alongside their current FSRSStats.
    /// Orders by cards that are due first (next_due <= now or last_review == 0), then by oldest next_due.
    pub fn get_cards_with_fsrs_for_deck(
        &self,
        deck_id: i64,
    ) -> anyhow::Result<Vec<(Card, FSRSStats)>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.id, c.term, c.definition,
                    COALESCE(s.stability, 0.0),
                    COALESCE(s.difficulty, 0.0),
                    COALESCE(s.repetition_count, 0),
                    COALESCE(s.last_review, 0),
                    COALESCE(s.next_due, 0),
                    COALESCE(s.lapses, 0),
                    COALESCE(s.state, 0)
             FROM cards c
             LEFT JOIN card_stats s ON c.id = s.card_id
             WHERE c.deck_id = ?1
             ORDER BY
                CASE WHEN COALESCE(s.last_review, 0) == 0 THEN 0
                     WHEN COALESCE(s.next_due, 0) <= strftime('%s','now') THEN 1
                     ELSE 2 END ASC,
                COALESCE(s.next_due, 0) ASC,
                c.id ASC",
        )?;

        let rows = stmt.query_map(params![deck_id], |row| {
            let card = Card {
                id: Some(row.get(0)?),
                term: row.get(1)?,
                definition: row.get(2)?,
            };
            let fsrs = FSRSStats {
                stability: row.get(3)?,
                difficulty: row.get(4)?,
                repetition_count: row.get(5)?,
                last_review: row.get(6)?,
                next_due: row.get(7)?,
                lapses: row.get(8)?,
                state: row.get(9)?,
            };
            Ok((card, fsrs))
        })?;

        let mut out = Vec::new();
        for r in rows {
            out.push(r.context("Failed mapping card FSRS row.")?);
        }
        Ok(out)
    }

    /// Dashboard query: Returns summary stats for all saved decks including total cards,
    /// due cards, new cards, last studied timestamp, and earliest next due timestamp.
    pub fn get_deck_dashboard_items(&self) -> anyhow::Result<Vec<DeckDashboardItem>> {
        let now = now_secs();
        let mut stmt = self.conn.prepare(
            "SELECT d.id, d.name,
                    COUNT(c.id) AS total_cards,
                    SUM(CASE WHEN c.id IS NOT NULL AND (s.last_review IS NULL OR s.last_review = 0 OR s.next_due <= ?1) THEN 1 ELSE 0 END) AS due_cards,
                    SUM(CASE WHEN c.id IS NOT NULL AND (s.repetition_count IS NULL OR s.repetition_count = 0) THEN 1 ELSE 0 END) AS new_cards,
                    ds.last_studied_at,
                    MIN(CASE WHEN c.id IS NOT NULL AND s.last_review > 0 AND s.next_due > ?1 THEN s.next_due ELSE NULL END) AS next_due_at
             FROM decks d
             LEFT JOIN cards c ON d.id = c.deck_id
             LEFT JOIN card_stats s ON c.id = s.card_id
             LEFT JOIN deck_stats ds ON d.id = ds.deck_id
             GROUP BY d.id
             ORDER BY due_cards DESC, d.name ASC",
        )?;

        let rows = stmt.query_map(params![now], |row| {
            Ok(DeckDashboardItem {
                id: row.get(0)?,
                name: row.get(1)?,
                total_cards: row.get(2)?,
                due_cards: row.get(3)?,
                new_cards: row.get(4)?,
                last_studied_at: row.get(5)?,
                next_due_at: row.get(6)?,
            })
        })?;

        let mut out = Vec::new();
        for r in rows {
            out.push(r.context("Failed mapping deck dashboard item.")?);
        }
        Ok(out)
    }

    // ========================== Confusions ==========================

    /// Update a confusion count for (card_id, mistaken_with) by adding delta.
    /// If new score <= 0, remove the confusion row.
    ///
    /// Behavior:
    ///  - If a row exists: new_count = old_count + delta.
    ///      - If new_count > 0 => UPDATE count = new_count
    ///      - If new_count <= 0 => DELETE row
    ///  - If no row exists and delta > 0 => INSERT new row with count = delta
    pub fn adjust_confusion(&mut self, id_a: i64, id_b: i64, delta: i64) -> Result<()> {
        // enforce ordering so we have only have one undirected edge
        let (card_id, mistaken_with) = if id_a < id_b {
            (id_a, id_b)
        } else {
            (id_b, id_a)
        };

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

    /// Fetches bi-directional confusions for a given card for faster accurate distractors.
    pub fn get_bidirectional_confusions(&self, card_id: i64) -> anyhow::Result<Vec<(i64, i64)>> {
        Ok(self
            .conn
            .prepare(
                "SELECT mistaken_card_id, count
                FROM card_confusions
                WHERE card_id = ?1
                UNION ALL
                SELECT card_id, count
                FROM card_confusions
                WHERE mistaken_card_id = ?1",
            )?
            .query_map([card_id], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(Result::ok)
            .collect())
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

    // ======================== User Profile ========================

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

    // ======================== Stats Reads (cont) =================

    /// Summarize stats for a deck under FSRS: New (0 reps), Learning (reps > 0, stability < 7d), Mature (stability >= 7d).
    pub fn get_deck_stats_summary(&self, deck_id: i64) -> Result<DeckStatsSummary> {
        self.conn
            .query_row(
                "SELECT
                    COUNT(*),
                    SUM(CASE WHEN s.repetition_count = 0 THEN 1 ELSE 0 END),
                    SUM(CASE WHEN s.repetition_count > 0 AND s.stability < 7.0 THEN 1 ELSE 0 END),
                    SUM(CASE WHEN s.repetition_count > 0 AND s.stability >= 7.0 THEN 1 ELSE 0 END),
                    AVG(s.stability)
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
                        average_easiness: r.get::<_, Option<f64>>(4)?.unwrap_or(0.0),
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
                "SELECT c.id, c.term, c.definition, s.learning_score, s.stability, s.difficulty, s.repetition_count, s.last_review, s.next_due, s.lapses, s.state, s.correct_count, s.incorrect_count
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
                    fsrs: Some(FSRSStats {
                        stability: r.get(4)?,
                        difficulty: r.get(5)?,
                        repetition_count: r.get(6)?,
                        last_review: r.get(7)?,
                        next_due: r.get(8)?,
                        lapses: r.get(9)?,
                        state: r.get(10)?,
                    }),
                    correct_count: r.get(11)?,
                    incorrect_count: r.get(12)?,
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

    /// Update a card's term and/or definition
    pub fn update_card(
        &mut self,
        card_id: i64,
        term: Option<&str>,
        definition: Option<&str>,
    ) -> Result<()> {
        let now = now_secs();
        match (term, definition) {
            (Some(t), Some(d)) => {
                self.conn.execute(
                    "UPDATE cards SET term = ?1, definition = ?2, updated_at = ?3 WHERE id = ?4",
                    params![t, d, now, card_id],
                )?;
            }
            (Some(t), None) => {
                self.conn.execute(
                    "UPDATE cards SET term = ?1, updated_at = ?2 WHERE id = ?3",
                    params![t, now, card_id],
                )?;
            }
            (None, Some(d)) => {
                self.conn.execute(
                    "UPDATE cards SET definition = ?1, updated_at = ?2 WHERE id = ?3",
                    params![d, now, card_id],
                )?;
            }
            (None, None) => {}
        }
        Ok(())
    }
    // ==================== Failed Session Files ====================

    /// Find unsaved session files written by fallback logic.
    /// They live next to the DB file and match `quizzy_failed_session_*.log`.
    pub fn failed_session_files(&self) -> Result<Vec<std::path::PathBuf>> {
        let mut dir = db_path_from_env_or_default();
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

    /// Remove a failed session file after replay or if user discards it.
    pub fn remove_failed_session_file(&self, path: &Path) -> Result<()> {
        std::fs::remove_file(path)
            .with_context(|| format!("Failed to remove failed session file {}.", path.display()))?;
        Ok(())
    }
}

// ========================= Free Functions ==========================

/// Initialize the database connection: apply base schema, then run migrations.
pub fn init_db(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "foreign_keys", "ON")
        .context("Failed to enable foreign_keys")?;
    let _ = conn.pragma_update(None, "journal_mode", "WAL");

    // Apply the base schema (CREATE TABLE IF NOT EXISTS — safe on existing DBs).
    conn.execute_batch(SCHEMA)
        .context("Failed to execute schema SQL")?;

    // Ensure the user_profile singleton row exists.
    conn.execute(
        "INSERT OR IGNORE INTO user_profile (id, currency) VALUES (1, 0);",
        [],
    )
    .context("Failed to ensure user_profile row.")?;

    // Apply any pending versioned migrations.
    run_migrations(conn).context("Failed to run schema migrations.")?;

    Ok(())
}

pub fn get_deck(src: DeckSource, storage: &Storage) -> anyhow::Result<Deck> {
    match src {
        DeckSource::Named(n) => {
            // if n can be parsed from string into a number, then get deck by id, else get deck by name
            if let Ok(deck_id) = n.parse::<i64>() {
                storage
                    .get_deck_by_id(deck_id)
                    .context("Failed to get deck by id.")
            } else {
                storage
                    .get_deck_by_name(&n)
                    .context("Failed to get deck by name.")
            }
        }
        DeckSource::File(p) => read_deck_from_file(&p),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn test_init_db_and_fsrs_session_commit() {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();

        let storage = Storage { conn };

        let deck_id: i64 = storage
            .conn
            .query_row(
                "INSERT INTO decks (name) VALUES ('Test Deck') RETURNING id",
                [],
                |r| r.get(0),
            )
            .unwrap();

        let card_id: i64 = storage
            .conn
            .query_row(
                "INSERT INTO cards (deck_id, term, definition) VALUES (?1, 'Hola', 'Hello') RETURNING id",
                params![deck_id],
                |r| r.get(0),
            )
            .unwrap();

        let fsrs = FSRSStats {
            stability: 2.5,
            difficulty: 4.1,
            repetition_count: 1,
            last_review: 100000,
            next_due: 186400,
            lapses: 0,
            state: 2,
        };

        storage
            .commit_learn_session(&[(card_id, fsrs, 1, 0, 1)])
            .unwrap();

        let cards_with_fsrs = storage.get_cards_with_fsrs_for_deck(deck_id).unwrap();
        assert_eq!(cards_with_fsrs.len(), 1);
        let (c, read_fsrs) = &cards_with_fsrs[0];
        assert_eq!(c.term, "Hola");
        assert_eq!(read_fsrs.stability, 2.5);
        assert_eq!(read_fsrs.difficulty, 4.1);
        assert_eq!(read_fsrs.repetition_count, 1);
        assert_eq!(read_fsrs.lapses, 0);
        assert_eq!(read_fsrs.state, 2);

        let dashboard = storage.get_deck_dashboard_items().unwrap();
        assert_eq!(dashboard.len(), 1);
        assert_eq!(dashboard[0].name, "Test Deck");
        assert_eq!(dashboard[0].total_cards, 1);

        let stats_summary = storage.get_deck_stats_summary(deck_id).unwrap();
        assert_eq!(stats_summary.total_cards, 1);
        assert_eq!(stats_summary.new_count, 0);
        assert_eq!(stats_summary.learning_count, 1);
        assert_eq!(stats_summary.mature_count, 0);
        assert!((stats_summary.average_easiness - 2.5).abs() < 1e-6);
    }

    #[test]
    fn test_find_deck_by_name_and_weakest_cards_and_dashboard_zero_due() {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();

        let storage = Storage { conn };

        // Test find_deck_by_name returns None when deck does not exist
        assert!(storage.find_deck_by_name("NonExistent").unwrap().is_none());
        assert!(storage.get_deck_by_name("NonExistent").is_err());

        // Create deck
        let deck_id: i64 = storage
            .conn
            .query_row(
                "INSERT INTO decks (name) VALUES ('French') RETURNING id",
                [],
                |r| r.get(0),
            )
            .unwrap();

        let card_id: i64 = storage
            .conn
            .query_row(
                "INSERT INTO cards (deck_id, term, definition) VALUES (?1, 'Bonjour', 'Hello') RETURNING id",
                params![deck_id],
                |r| r.get(0),
            )
            .unwrap();

        // Test find_deck_by_name returns Some(Deck)
        let found = storage
            .find_deck_by_name("French")
            .unwrap()
            .expect("Deck should be found");
        assert_eq!(found.id, Some(deck_id));
        assert_eq!(found.name, "French");
        assert_eq!(found.cards.len(), 1);

        // Test get_weakest_cards when card_stats has no row for card (COALESCE integer)
        let weakest = storage.get_weakest_cards(deck_id, 10).unwrap();
        assert_eq!(weakest.len(), 1);
        assert_eq!(weakest[0].0.term, "Bonjour");
        assert_eq!(weakest[0].1, 0); // score should be 0 (integer)

        // Commit review with next_due far in the future
        let future_due = now_secs() + 100000;
        let fsrs = FSRSStats {
            stability: 5.0,
            difficulty: 3.0,
            repetition_count: 2,
            last_review: now_secs(),
            next_due: future_due,
            lapses: 0,
            state: 2,
        };
        storage
            .commit_learn_session(&[(card_id, fsrs, 1, 0, 1)])
            .unwrap();

        // Dashboard should return this deck even when due_cards == 0
        let dashboard = storage.get_deck_dashboard_items().unwrap();
        assert_eq!(dashboard.len(), 1);
        assert_eq!(dashboard[0].name, "French");
        assert_eq!(dashboard[0].total_cards, 1);
        assert_eq!(dashboard[0].due_cards, 0);
        assert_eq!(dashboard[0].new_cards, 0);
        assert_eq!(dashboard[0].next_due_at, Some(future_due));
    }
}
