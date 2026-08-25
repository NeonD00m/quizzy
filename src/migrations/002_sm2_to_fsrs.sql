-- Migration 002: Rebuild card_stats table, replacing SM-2 columns with FSRS columns.
-- Preserves: card_id, learning_score, correct_count, incorrect_count.
-- Drops:     interval, repetitions, easiness_factor (SM-2).
-- Adds:      stability, difficulty, repetition_count, last_review, next_due (FSRS).
-- Safe to run on a fresh database (card_stats_new is created IF NOT EXISTS).

CREATE TABLE IF NOT EXISTS card_stats_new (
    card_id          INTEGER PRIMARY KEY REFERENCES cards(id) ON DELETE CASCADE,
    learning_score   INTEGER NOT NULL DEFAULT 0,
    correct_count    INTEGER NOT NULL DEFAULT 0,
    incorrect_count  INTEGER NOT NULL DEFAULT 0,
    stability        REAL    NOT NULL DEFAULT 0.0,
    difficulty       REAL    NOT NULL DEFAULT 0.0,
    repetition_count INTEGER NOT NULL DEFAULT 0,
    last_review      INTEGER NOT NULL DEFAULT 0,
    next_due         INTEGER NOT NULL DEFAULT 0
);

INSERT OR IGNORE INTO card_stats_new
    (card_id, learning_score, correct_count, incorrect_count)
SELECT card_id,
       COALESCE(learning_score, 0),
       COALESCE(correct_count, 0),
       COALESCE(incorrect_count, 0)
FROM card_stats;

DROP TABLE IF EXISTS card_stats;

ALTER TABLE card_stats_new RENAME TO card_stats;

CREATE INDEX IF NOT EXISTS idx_card_stats_learning_score ON card_stats(learning_score);
CREATE INDEX IF NOT EXISTS idx_card_stats_next_due       ON card_stats(next_due);
