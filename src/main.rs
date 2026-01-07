use clap::{Parser, Subcommand, command};
use std::path::PathBuf;
mod core;
mod ui;

use crate::core::deck::{Deck, get_deck, import_deck, resolve_deck_source};
use crate::core::import::import_from_quizlet;
use crate::core::string_distance::string_distance;
use crate::ui::cards::cards_mode;
use crate::ui::learn::learn_mode;

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
        file: Option<PathBuf>,
    },
    Import {
        name: Option<String>,
        url: Option<String>,
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

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Test { s1, s2 } => {
            println!("String Distance: {}", string_distance(s1, s2));
        }
        Command::New { name, file } => {
            println!("creating deck by name: {}", name);
            let deck = match file {
                Some(p) => {
                    let mut d = import_deck(p);
                    d.name = name.to_string();
                    d
                }
                None => Deck::named(name),
            };
            println!("Saving deck {}", deck.name); // double check name just in case
            todo!("Need to implement storage.");
        }
        Command::Import { name, url } => import_from_quizlet(name, url),
        Command::Add {
            deck,
            term,
            definition,
        } => {
            println!("Adding term ({term}) and definition ({definition}) to deck {deck}");
        }
        Command::Remove { deck, term } => {
            println!("Removing term ({term}) from deck {deck}");
        }
        Command::List { deck } => match deck {
            Some(name) => {
                println!("Listing out cards in deck: {}", name);
                let deck = get_deck(resolve_deck_source(name.as_str()));
                for c in deck.cards {
                    println!("{} -> {}", c.term, c.definition)
                }
            }
            None => {
                println!("Listing out saved decks:");
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
            get_deck(resolve_deck_source(deck.as_str())),
            nostats,
            terms,
            definitions,
            written,
            multiplechoice,
            questions,
        ),
        Command::Cards { deck, shuffle } => {
            cards_mode(get_deck(resolve_deck_source(deck.as_str())), shuffle)
        }
        Command::Delete { deck } => {
            println!(
                "Are you sure you want to delete from database?\n(This means removing the saved deck by this name)"
            );
            println!(
                "Would you also like to delete all stats associated with this deck?\n(They can be preserved and then accessed by `quizzy stats {deck}`"
            )
        }
    }
}
