use clap::{Parser, Subcommand, command};
use std::path::PathBuf;
mod core;
mod ui;
use quizzy::core::deck::{Deck, DeckSource, get_deck, resolve_deck_source};
use quizzy::core::import::import_from_quizlet;
use quizzy::core::string_distance::string_distance;
use quizzy::ui::cards::cards_mode;
use quizzy::ui::learn::learn_mode;

#[derive(Parser)]
#[command(name = "quizzy")]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    Test {
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
    Delete {
        deck: String,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    return match cli.command {
        Command::Test { s1, s2 } => {
            println!("String Distance: {}", string_distance(s1, s2));
            Ok(())
        }
        Command::New { name, file } => {
            println!("creating deck by name: {}", name);
            let deck = match file {
                Some(p) => {
                    let mut d = get_deck(DeckSource::File(p))?;
                    d.name = name.to_string();
                    d
                }
                None => Deck::named(name),
            };
            println!("Saving deck {}", deck.name); // double check name just in case
            anyhow::bail!("Storage not yet implemented");
        }
        Command::Import { name, url_or_json } => import_from_quizlet(name, url_or_json),
        Command::Add {
            deck,
            term,
            definition,
        } => {
            println!(
                "Adding term ({}) and definition ({}) to deck {}",
                term, definition, deck
            );
            Ok(())
        }
        Command::Remove { deck, term } => {
            println!("Removing term ({}) from deck {}", term, deck);
            Ok(())
        }
        Command::List { deck } => match deck {
            Some(name) => {
                println!("Listing out cards in deck: {}", name);
                let deck = get_deck(resolve_deck_source(name.as_str()))?;
                for c in deck.cards {
                    println!("{} -> {}", c.term, c.definition)
                }
                Ok(())
            }
            None => {
                println!("Listing out saved decks:");
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
            get_deck(resolve_deck_source(deck.as_str()))?,
            nostats,
            terms,
            definitions,
            written,
            multiplechoice,
            questions,
        ),
        Command::Cards { deck, shuffle } => {
            cards_mode(get_deck(resolve_deck_source(deck.as_str()))?, shuffle)
        }
        Command::Delete { deck } => {
            println!(
                "Are you sure you want to delete from database?\n(This means removing the saved deck by this name)"
            );
            println!(
                "Would you also like to delete all stats associated with this deck?\n(They can be preserved and then accessed by `quizzy stats {deck}`"
            );
            Ok(())
        }
    };
}
