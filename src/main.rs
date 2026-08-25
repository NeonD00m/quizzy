use clap::{Parser, Subcommand};
use std::path::PathBuf;
mod core;
mod mcp;
mod ui;
use crate::core::deck::{Deck, DeckSource, resolve_deck_source};
use crate::core::learn::{commit_payload_with_retries, read_failed_session_file};
use crate::core::storage::{Storage, get_deck};
use crate::core::string_distance::string_distance;
use crate::ui::cards::{cards_mode, cram_mode};
use crate::ui::gamble::gauntlet_mode;
use crate::ui::import::import_from_quizlet;
use crate::ui::learn::{learn_dashboard, learn_mode, test_mode};
use crate::ui::stats::stats_mode;
use chrono::Utc;
use std::io::{Write, stdin, stdout};

#[derive(Parser)]
#[command(name = "quizzy", version)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Compares two strings and outputs a distance metric (for testing).
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
        /// Using a url requires browser available, json can be used directly
        url_or_json: Option<String>,
    },
    /// Imports decks from all files in a given directory.
    ImportAll {
        /// Directory to import deck files from
        dir: PathBuf,
        /// Overwrite existing decks with the same name
        #[arg(long)]
        overwrite: bool,
    },
    /// Writes a deck (by name, deck id, or file path) to a file in the current directory.
    ///
    /// Writes a deck (by name, deck id, or file path) to a file. If the file already exists, it will be overwritten. If no path is provided, it will attempt to write to the deck's original source path.
    Export {
        deck: String,
        /// Destination file path (e.g. deck.csv, output.json)
        file_path: Option<PathBuf>,
    },
    /// Exports all saved decks into a given directory.
    ExportAll {
        /// Directory to import deck files from
        dir: PathBuf,
        /// Only export decks with no source path (created in Quizzy, not imported from a file)
        #[arg(long)]
        unsourced_only: bool,
    },
    /// Adds a new card to a saved deck (name or deck id).
    Add {
        deck: String,
        term: String,
        definition: String,
    },
    /// Adds terms and definitions from a file or another deck to a saved deck (name or deck id).
    Append {
        deck: String,
        /// Source to import from (e.g. new_cards.csv or "Spanish Phrases")
        source: String,
    },
    /// Removes a card from a saved deck (name or deck id) by term or card id.
    Remove {
        deck: String,
        term_or_card_id: String,
    },
    /// Clears all cards from a saved deck (name or deck id), but keeps the deck itself.
    Clear {
        deck: String,
        #[arg(short, long)]
        confirm: bool,
    },
    /// Renames a saved deck (name or deck id).
    Rename { deck: String, new_name: String },
    /// Edits the contents of a card's term or definition.
    ///
    /// Edits the contents of a card's term or definition. Use `-t="new term"` or `-d="new definition"` to specify arguments.
    Edit {
        deck: String,

        term_or_card_id: String,

        /// Rewrite the term for the card.
        #[arg(short, long)]
        term: Option<String>,

        /// Rewrite the definition for the card.
        #[arg(short, long)]
        definition: Option<String>,
    },
    /// Lists saved decks, or cards in a deck if a deck name or deck id is provided.
    ///
    /// Lists saved decks, or cards in a deck if a deck name or deck id is provided. Use -v/--verbose for card counts and creation dates when listing decks.
    List {
        deck: Option<String>,

        /// If provided, only lists cards or decks containing the pattern in their name (case-insensitive)
        search: Option<String>,

        /// List decks with more details (e.g. card count, last studied)
        #[arg(short, long)]
        verbose: bool,
    },
    /// Spaced-repetition (FSRS) practice by hybrid active recall with typed answers.
    Learn {
        /// Name of the deck to learn (optional; if omitted, shows the interactive dashboard)
        deck: Option<String>,

        /// Ask about terms only (priority)
        #[arg(short, long)]
        terms: bool,

        /// Ask about definitions only
        #[arg(short, long)]
        definitions: bool,
    },
    /// Begins a multiple-choice/written answer test that does not affect memorization stats.
    ///
    /// Multiple-choice/written answer test. By default, it will ask a mix of term and definition questions, prioritizing written questions over multiple choice. Use the flags to customize the question types and quantity.
    Test {
        deck: String,

        /// Instant feedback after every question
        #[arg(short, long)]
        feedback: bool,

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
        /// Name of the deck to review (optional; if omitted, prompts deck selection)
        deck: Option<String>,

        /// Shuffle cards before studying
        #[arg(short, long)]
        shuffle: bool,
    },
    /// "Cram" study mode for memorizing in less than a week: flash cards and simple self-grading
    Study { saved_deck: Option<String> },
    /// "Cram" study mode for memorizing in less than a week: flash cards and simple self-grading
    Cram { saved_deck: Option<String> },
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
    /// Launches an MCP server for AI Agents to interact with Quizzy
    MCP {},
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
                    match read_failed_session_file(&p) {
                        Ok(payload) => match commit_payload_with_retries(storage, &payload, 3) {
                            Ok(()) => {
                                println!(
                                    "Saved session {} successfully; removing file.",
                                    p.display()
                                );
                                if let Err(e) = storage.remove_failed_session_file(&p) {
                                    eprintln!("Warning: failed to remove {}: {}", p.display(), e);
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to save session {}: {}", p.display(), e);
                                eprintln!("File has been preserved; you can retry later.");
                            }
                        },
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

    // Initialize tracing subscriber strictly on stderr before any storage/db calls
    if matches!(cli.command, Command::MCP {}) {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::from_default_env()
                    .add_directive(tracing::Level::DEBUG.into()),
            )
            .with_writer(std::io::stderr)
            .with_ansi(false)
            .try_init();
    } else if std::env::var_os("RUST_LOG").is_some() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .with_writer(std::io::stderr)
            .try_init();
    }

    let mut storage = Storage::open_default()?;
    if !matches!(cli.command, Command::MCP {}) {
        startup(&mut storage)?;
    }
    match cli.command {
        Command::Compare { s1, s2 } => {
            println!("String Distance: {}", string_distance(&s1, &s2));
            Ok(())
        }
        Command::New { name, source } => ui::general::new(&mut storage, name, source),
        Command::Import { name, url_or_json } => {
            import_from_quizlet(name, url_or_json, &mut storage)
        }
        Command::ImportAll { dir, overwrite } => {
            ui::general::import_all(&mut storage, dir, overwrite)
        }
        Command::Export { deck, file_path } => ui::general::export(&mut storage, deck, file_path),
        Command::ExportAll {
            dir,
            unsourced_only,
        } => ui::general::export_all(&mut storage, dir, unsourced_only),
        Command::Add {
            deck,
            term,
            definition,
        } => ui::general::add(&mut storage, deck, term, definition),
        Command::Append { deck, source } => ui::general::append(&mut storage, deck, source),
        Command::Remove {
            deck,
            term_or_card_id,
        } => ui::general::remove(&mut storage, deck, term_or_card_id),
        Command::Clear { deck, confirm } => ui::general::clear(&mut storage, deck, confirm),
        Command::Rename { deck, new_name } => ui::general::rename(&mut storage, deck, new_name),
        Command::Edit {
            deck,
            term_or_card_id,
            term,
            definition,
        } => ui::general::edit(&mut storage, deck, term_or_card_id, term, definition),
        Command::List {
            deck,
            search,
            verbose,
        } => ui::general::list(&mut storage, deck, search, verbose),
        Command::Learn {
            deck,
            terms,
            definitions,
        } => {
            if let Some(deck_name) = deck {
                let deck = get_deck(resolve_deck_source(deck_name.as_str()), &storage)?;
                storage.update_user_last_active()?;
                if let Some(id) = deck.id {
                    storage.update_deck_last_studied(id)?;
                }
                learn_mode(deck, terms, definitions, &mut storage)
            } else {
                learn_dashboard(&mut storage)
            }
        }
        Command::Test {
            deck,
            feedback,
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
            test_mode(
                deck,
                feedback,
                terms,
                definitions,
                written,
                multiple_choice,
                questions,
                &mut storage,
            )
        }
        Command::Cards { deck, shuffle } => {
            let deck = if let Some(deck_name) = deck {
                let deck = get_deck(resolve_deck_source(deck_name.as_str()), &storage)?;
                storage.update_user_last_active()?;
                if let Some(id) = deck.id {
                    storage.update_deck_last_studied(id)?;
                }
                Some(deck)
            } else {
                None
            };
            cards_mode(deck, shuffle, &mut storage)
        }
        Command::Study { saved_deck } | Command::Cram { saved_deck } => {
            let deck = if let Some(deck_name) = saved_deck {
                let deck = get_deck(resolve_deck_source(deck_name.as_str()), &storage)?;
                storage.update_user_last_active()?;
                if let Some(id) = deck.id {
                    storage.update_deck_last_studied(id)?;
                }
                Some(deck)
            } else {
                None
            };
            cram_mode(deck, &mut storage)
        }
        Command::Gamble { deck } | Command::Gauntlet { deck } => {
            let deck = get_deck(resolve_deck_source(deck.as_str()), &storage)?;
            storage.update_user_last_active()?;
            if let Some(id) = deck.id {
                storage.update_deck_last_studied(id)?;
            }
            gauntlet_mode(deck, &mut storage)
        }
        Command::Delete { deck } => match resolve_deck_source(deck.as_str()) {
            DeckSource::Named(name_or_id) => ui::general::delete(&mut storage, name_or_id),
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
        Command::MCP {} => mcp::server::launch(storage),
        #[allow(unreachable_patterns)]
        _ => {
            println!("Unimplemented command");
            Ok(())
        }
    }
}
