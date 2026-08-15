use anyhow::{Context, Result};
use rusqlite::Connection;
use std::collections::HashSet;

/// Represents a single versioned schema migration.
pub struct Migration {
    /// 1-based version number. Never change existing values — only append.
    pub version: u32,
    /// Human-readable name for logging.
    pub name: &'static str,
    /// SQL to execute. May be a single statement or a batch.
    pub sql: &'static str,
}

/// All migrations, in order. **Never edit existing entries — only append.**
///
/// Migration versioning rules:
///  - Versions are sequential starting at 1.
///  - Each migration is applied exactly once per database.
///  - The runner records the version in `schema_migrations` after applying.
///  - To add a new migration: append to this array with the next version number.
static MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "add_streak_to_user_profile",
        sql: "ALTER TABLE user_profile ADD COLUMN streak INTEGER NOT NULL DEFAULT 0",
    },
    Migration {
        version: 2,
        name: "sm2_to_fsrs_card_stats",
        sql: include_str!("../migrations/002_sm2_to_fsrs.sql"),
    },
    Migration {
        version: 3,
        name: "drop_source_hash",
        sql: include_str!("../migrations/003_drop_source_hash.sql"),
    },
    Migration {
        version: 4,
        name: "add_fsrs_lapses_and_state",
        sql: include_str!("../migrations/004_add_fsrs_lapses_and_state.sql"),
    },
];

/// Apply all pending migrations to the database.
///
/// Creates the `schema_migrations` tracking table if needed, reads which
/// versions have already been applied, and runs any that are missing in
/// version order.  Each migration executes inside a transaction; on success
/// the version number is recorded in `schema_migrations`.
///
/// Foreign keys are temporarily disabled around each migration to allow
/// table-rebuild operations (the SQLite recommended pattern for column
/// removal / renaming).
pub fn run_migrations(conn: &Connection) -> Result<()> {
    // Ensure the tracking table exists.
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version    INTEGER PRIMARY KEY,
            name       TEXT    NOT NULL,
            applied_at INTEGER NOT NULL DEFAULT (strftime('%s','now'))
        )",
    )
    .context("Failed to create schema_migrations table")?;

    // Read applied versions into a set for O(1) lookup.
    let applied: HashSet<u32> = {
        let mut stmt = conn
            .prepare("SELECT version FROM schema_migrations")
            .context("Failed to prepare schema_migrations query")?;
        stmt.query_map([], |row| row.get(0))
            .context("Failed to query schema_migrations")?
            .filter_map(|r| r.ok())
            .collect()
    };

    let max_version = MIGRATIONS.iter().map(|m| m.version).max().unwrap_or(0);

    if applied.len() == MIGRATIONS.len() {
        println!("[migrations] Schema up to date (version {}).", max_version);
        return Ok(());
    }

    for migration in MIGRATIONS {
        if applied.contains(&migration.version) {
            continue;
        }

        println!(
            "[migrations] Applying migration {}: {}...",
            migration.version, migration.name
        );

        // Disable foreign keys — required by SQLite for table-rebuild migrations.
        conn.pragma_update(None, "foreign_keys", "OFF")
            .context("Failed to disable foreign_keys for migration")?;

        let tx = conn
            .unchecked_transaction()
            .context("Failed to start migration transaction")?;

        let result = tx.execute_batch(migration.sql);

        // "duplicate column name" means the column already exists (e.g. migration 001
        // on a fresh DB where the base schema already includes `streak`).  Treat as
        // a no-op and continue so the version is still recorded.
        match result {
            Ok(()) => {}
            Err(ref e)
                if e.to_string()
                    .to_lowercase()
                    .contains("duplicate column name") =>
            {
                eprintln!(
                    "[migrations] Note: migration {} ({}) is a no-op — column already exists.",
                    migration.version, migration.name
                );
            }
            Err(e) => {
                // Re-enable foreign keys before propagating the error.
                let _ = conn.pragma_update(None, "foreign_keys", "ON");
                return Err(e).with_context(|| {
                    format!(
                        "Failed to execute migration {}: {}",
                        migration.version, migration.name
                    )
                });
            }
        }

        tx.execute(
            "INSERT INTO schema_migrations (version, name) VALUES (?1, ?2)",
            rusqlite::params![migration.version, migration.name],
        )
        .with_context(|| {
            format!(
                "Failed to record migration {} in schema_migrations",
                migration.version
            )
        })?;

        tx.commit()
            .with_context(|| format!("Failed to commit migration {}", migration.version))?;

        // Re-enable foreign keys.
        conn.pragma_update(None, "foreign_keys", "ON")
            .context("Failed to re-enable foreign_keys after migration")?;

        println!(
            "[migrations] Applied migration {}: {}.",
            migration.version, migration.name
        );
    }

    Ok(())
}
