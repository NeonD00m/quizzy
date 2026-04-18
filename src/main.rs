use clap::{Parser, Subcommand};
use std::path::PathBuf;
mod core;
mod ui;
use crate::core::deck::{Deck, DeckSource, read_deck_from_file, resolve_deck_source};
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
    Compare {
        s1: String,
        s2: String,
    },
    New {
        name: String,
        // pass in a file with tab separated terms and definitions
        file: Option<PathBuf>,
    },
    Import {
        name: Option<String>,
        // using url requires browser available, json can be used directly
        url_or_json: Option<String>,
    },
    Add {
        deck: String,
        term: String,
        definition: String,
    },
    Remove {
        deck: String,
        term: String,
    },
    List {
        deck: Option<String>,
    },
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
        multiplechoice: bool,

        /// Set the amount of questions
        #[arg(short, long, default_value_t = 20)]
        questions: u8,
    },
    Cards {
        deck: String,

        /// Shuffle cards before studying
        #[arg(short, long)]
        shuffle: bool,
    },
    Gauntlet {
        deck: String,
    },
    Gamble {
        deck: String,
    },
    Delete {
        deck: String,
    },
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
    if let Ok(Some(last_active)) = storage.get_user_last_active() {
        let now = Utc::now().timestamp();
        let secs_since = now - last_active;
        let seven_days = 7 * 24 * 60 * 60;
        if secs_since >= seven_days {
            println!(
                "Welcome back! It's been {} days since you last used Quizzy.",
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
            println!("String Distance: {}", string_distance(s1, s2));
            Ok(())
        }
        Command::New { name, file } => {
            println!("creating deck by name: {}", name);
            let deck = match file {
                Some(p) => {
                    let mut d = read_deck_from_file(p)?;
                    d.name = name.to_string();
                    d
                }
                None => Deck::named(name),
            };
            println!("Saving deck {}", deck.name); // double check name just in case
            let deck_id = storage.create_deck_from_core(deck, None, None)?;
            println!("Successfully saved deck. ({})", deck_id);
            Ok(())
        }
        Command::Import { name, url_or_json } => {
            import_from_quizlet(name, url_or_json, &mut storage)
        }
        Command::Add {
            deck,
            term,
            definition,
        } => ui::general::add(&mut storage, deck, term, definition),
        Command::Remove { deck, term } => ui::general::remove(&mut storage, deck, term),
        Command::List { deck } => match deck {
            Some(name) => {
                println!("Listing out cards in deck: {}", name);
                let deck = get_deck(resolve_deck_source(name.as_str()), &storage)?;
                for c in deck.cards {
                    println!("{} -> {}", c.term, c.definition)
                }
                Ok(())
            }
            None => {
                println!("Listing out saved decks:");
                for (id, name) in storage.list_decks()? {
                    println!("({})\t{}", id, name);
                }
                Ok(())
            }
        },
        Command::Learn {
            deck,
            nostats,
            terms,
            definitions,
            written,
            multiplechoice,
            questions,
        } => learn_mode(
            get_deck(resolve_deck_source(deck.as_str()), &storage)?,
            nostats,
            terms,
            definitions,
            written,
            multiplechoice,
            questions,
            &mut storage,
        ),
        Command::Cards { deck, shuffle } => cards_mode(
            get_deck(resolve_deck_source(deck.as_str()), &storage)?,
            shuffle,
        ),
        Command::Gamble { deck } => gauntlet_mode(
            get_deck(resolve_deck_source(deck.as_str()), &storage)?,
            &mut storage,
        ),
        Command::Gauntlet { deck } => gauntlet_mode(
            get_deck(resolve_deck_source(deck.as_str()), &storage)?,
            &mut storage,
        ),
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
