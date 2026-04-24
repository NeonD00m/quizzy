use clap::{Parser, Subcommand};
use std::path::PathBuf;
mod core;
mod ui;
use crate::core::deck::{Deck, DeckSource, resolve_deck_source, write_deck_to_file};
use crate::core::import::import_from_quizlet;
use crate::core::learn::commit_session_with_retries;
use crate::core::storage::{Storage, get_deck};
use crate::core::string_distance::string_distance;
use crate::ui::cards::cards_mode;
use crate::ui::gamble::gauntlet_mode;
use crate::ui::learn::learn_mode;
use crate::ui::stats::stats_mode;
use chrono::Utc;
use std::io::{Write, stdin, stdout};

#[derive(Parser)]
#[command(name = "quizzy")]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Compares two strings and outputs a distance metric (for testing or fun).
    Compare { s1: String, s2: String },
    /// Creates a new deck with a given name, optionally importing from a file or another deck.
    New {
        name: String,
        /// Source to import from (e.g. new_cards.csv or "Spanish Phrases")
        source: Option<String>,
    },
    /// Imports a deck from a Quizlet URL or JSON file from the API.
    ///
    /// Imports a deck from a Quizlet URL or JSON file from the API. If a name is provided, it will be used for the deck; otherwise, you will be prompted to provide one.
    Import {
        name: Option<String>,
        // using url requires browser available, json can be used directly
        url_or_json: Option<String>,
    },
    /// Writes a deck (by name or file path) to a file in the current directory.
    ///
    /// Writes a deck (by name or file path) to a file in the current directory. The file type is determined by the extension you provide (e.g. csv, tsv, json). If the file already exists, it will be overwritten.
    Export {
        name: String,
        /// Destination file path (e.g. deck.csv, output.json)
        file_path: PathBuf,
    },
    /// Adds a new card to a saved deck.
    Add {
        deck: String,
        term: String,
        definition: String,
    },
    /// Adds terms and definitions from a file or another deck to a saved deck.
    Append {
        deck: String,
        /// Source to import from (e.g. new_cards.csv or "Spanish Phrases")
        source: String,
    },
    /// Removes a card from a saved deck by term.
    Remove { deck: String, term: String },
    /// Clears all cards from a saved deck, but keeps the deck itself.
    Clear { deck: String },
    /// Renames a saved deck.
    Rename { deck: String, new_name: String },
    /// Lists saved decks, or cards in a deck if a deck name is provided.
    ///
    /// Lists saved decks, or cards in a deck if a deck name is provided. Use -v/--verbose for card counts and creation dates when listing decks.
    List {
        deck: Option<String>,

        /// If provided, only lists cards or decks containing the pattern in their name (case-insensitive)
        search: Option<String>,

        /// List decks with more details (e.g. card count, last studied)
        #[arg(short, long)]
        verbose: bool,
    },
    /// Starts a learning session with a deck, asking questions in various formats.
    ///
    /// Starts a learning session with a deck, asking questions in various formats. By default, it will ask a mix of term and definition questions, prioritizing written questions over multiple choice. Use the flags to customize the question types and quantity. Performance stats will be saved after the session unless --nostats is used.
    Learn {
        deck: String,

        /// Don't save performance stats
        #[arg(short, long)]
        nostats: bool,

        /// Ask about terms only (priority)
        #[arg(short, long)]
        terms: bool,

        /// Ask about definitions only
        #[arg(short, long)]
        definitions: bool,

        /// Ask written questions only (priority)
        #[arg(short, long, default_value_t = false)]
        written: bool,

        /// Ask multiple choice questions only
        #[arg(short, long, default_value_t = false)]
        multiple_choice: bool,

        /// Set the amount of questions
        #[arg(short, long, default_value_t = 20)]
        questions: u8,
    },
    /// Review cards in a deck without quizzing, optionally shuffling the order.
    Cards {
        deck: String,

        /// Shuffle cards before studying
        #[arg(short, long)]
        shuffle: bool,
    },
    /// A more intense learning mode that will have you on your toes!
    Gauntlet { deck: String },
    /// Currently an alias for Gauntlet mode, but may soon have a separate style of game.
    Gamble { deck: String },
    /// Permanently deletes a deck from the database by name. Use with caution!
    Delete { deck: String },
    /// Shows performance statistics for a deck, or overall if no deck is specified. Stats are paginated with --size and --page.
    Stats {
        deck: Option<String>,

        /// Page size
        #[arg(short, long, default_value_t = 10)]
        size: u32,

        /// Page size
        #[arg(short, long, default_value_t = 0)]
        page: u32,
    },
}

fn startup(storage: &mut Storage) -> anyhow::Result<()> {
    // 1) Welcome back if user inactive for a while (7 days)
    if let Ok(Some(last_studied)) = storage.get_user_last_studied() {
        let now = Utc::now().timestamp();
        let secs_since = now - last_studied;
        let seven_days = 7 * 24 * 60 * 60;
        if secs_since >= seven_days {
            println!(
                "Welcome back! It's been {} days since you last studied with Quizzy.",
                secs_since / 86400
            );
        }
    }

    // 2) Look for unsaved session files
    match storage.failed_session_files() {
        Ok(files) if !files.is_empty() => {
            println!("Unsaved session(s) found!");
            for (i, p) in files.iter().enumerate() {
                println!("  [{}] {}", i + 1, p.display());
            }
            print!("Would you like me to try saving them now? (y/N): ");
            stdout().flush()?;
            let mut choice = String::new();
            stdin().read_line(&mut choice)?;
            let choice = choice.trim().to_lowercase();
            if choice == "y" || choice == "yes" {
                for p in files {
                    println!("Attempting to save {}", p.display());
                    match storage.read_failed_session_file(&p) {
                        Ok(updates) => {
                            // commit_session_with_retries is in ui::learn and should be public
                            match commit_session_with_retries(storage, &updates, 3) {
                                Ok(()) => {
                                    println!(
                                        "Saved session {} successfully; removing file.",
                                        p.display()
                                    );
                                    if let Err(e) = storage.remove_failed_session_file(&p) {
                                        eprintln!(
                                            "Warning: failed to remove {}: {}",
                                            p.display(),
                                            e
                                        );
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Failed to save session {}: {}", p.display(), e);
                                    eprintln!("File has been preserved; you can retry later.");
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to parse session file {}: {}", p.display(), e);
                            eprintln!("Skipping this file. You can inspect or delete it manually.");
                        }
                    }
                }
            } else {
                println!(
                    "Okay — unsaved sessions will remain in the DB directory. You can replay them later."
                );
            }
        }
        Ok(_) => { /* no files found */ }
        Err(e) => {
            eprintln!("Warning: failed to enumerate unsaved session files: {}", e);
        }
    };
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let mut storage = Storage::open_default()?;
    startup(&mut storage)?;
    match cli.command {
        Command::Compare { s1, s2 } => {
            println!("String Distance: {}", string_distance(&s1, &s2));
            Ok(())
        }
        Command::New { name, source } => ui::general::new(&mut storage, name, source),
        Command::Import { name, url_or_json } => {
            import_from_quizlet(name, url_or_json, &mut storage)
        }
        Command::Export { name, file_path } => {
            let deck = get_deck(resolve_deck_source(name.as_str()), &storage)?;
            println!("Exporting deck '{}' to {}...", name, file_path.display());
            write_deck_to_file(&deck, file_path)?;
            println!("Successfully exported deck.");
            Ok(())
        }
        Command::Add {
            deck,
            term,
            definition,
        } => ui::general::add(&mut storage, deck, term, definition),
        Command::Append { deck, source } => ui::general::append(&mut storage, deck, source),
        Command::Remove { deck, term } => ui::general::remove(&mut storage, deck, term),
        Command::Clear { deck } => ui::general::clear(&mut storage, deck),
        Command::Rename { deck, new_name } => ui::general::rename(&mut storage, deck, new_name),
        Command::List {
            deck,
            search,
            verbose,
        } => ui::general::list(&mut storage, deck, search, verbose),
        Command::Learn {
            deck,
            nostats,
            terms,
            definitions,
            written,
            multiple_choice,
            questions,
        } => {
            let deck = get_deck(resolve_deck_source(deck.as_str()), &storage)?;
            storage.update_user_last_active()?;
            if let Some(id) = deck.id {
                storage.update_deck_last_studied(id)?;
            }
            learn_mode(
                deck,
                nostats,
                terms,
                definitions,
                written,
                multiple_choice,
                questions,
                &mut storage,
            )
        }
        Command::Cards { deck, shuffle } => {
            let deck = get_deck(resolve_deck_source(deck.as_str()), &storage)?;
            storage.update_user_last_active()?;
            if let Some(id) = deck.id {
                storage.update_deck_last_studied(id)?;
            }
            cards_mode(deck, shuffle)
        }
        Command::Gamble { deck } => {
            let deck = get_deck(resolve_deck_source(deck.as_str()), &storage)?;
            storage.update_user_last_active()?;
            if let Some(id) = deck.id {
                storage.update_deck_last_studied(id)?;
            }
            gauntlet_mode(deck, &mut storage)
        }
        Command::Gauntlet { deck } => {
            let deck = get_deck(resolve_deck_source(deck.as_str()), &storage)?;
            storage.update_user_last_active()?;
            if let Some(id) = deck.id {
                storage.update_deck_last_studied(id)?;
            }
            gauntlet_mode(deck, &mut storage)
        }
        Command::Delete { deck } => match resolve_deck_source(deck.as_str()) {
            DeckSource::Named(name) => ui::general::delete(&mut storage, name),
            DeckSource::File(_) => {
                println!(
                    "Path specified; not deleting files. Use the deck name of a saved deck to delete from DB."
                );
                Ok(())
            }
        },
        Command::Stats { deck, size, page } => {
            let deck_option: Option<Deck> = if let Some(name) = deck {
                get_deck(resolve_deck_source(name.as_str()), &storage).ok()
            } else {
                None
            };
            stats_mode(deck_option, size, page, &mut storage)
        }
    }
}
