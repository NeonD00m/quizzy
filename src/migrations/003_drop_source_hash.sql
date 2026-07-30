-- Migration 003: Rebuild decks table, dropping the source_hash column.
-- source_hash was written but never read anywhere in the codebase.
-- source_path is preserved because it is used for the export command fallback.
-- Safe to run on a fresh database (decks_new is created IF NOT EXISTS).

CREATE TABLE IF NOT EXISTS decks_new (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT    NOT NULL,
    description TEXT,
    created_at  INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    updated_at  INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    source_path TEXT
);

INSERT OR IGNORE INTO decks_new
    (id, name, description, created_at, updated_at, source_path)
SELECT id, name, description, created_at, updated_at, source_path
FROM decks;

DROP TABLE IF EXISTS decks;

ALTER TABLE decks_new RENAME TO decks;
