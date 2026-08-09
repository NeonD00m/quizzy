-- Migration 004: Add lapses and state columns to card_stats table for FSRS state machine tracking.
-- lapses: Count of "Again" ratings (defaults to 0).
-- state: Card memory state (0=New, 1=Learning, 2=Review, 3=Relearning, defaults to 0).

ALTER TABLE card_stats ADD COLUMN lapses INTEGER NOT NULL DEFAULT 0;
ALTER TABLE card_stats ADD COLUMN state INTEGER NOT NULL DEFAULT 0;
