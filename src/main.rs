use clap::{Parser, Subcommand, command};
use std::path::PathBuf;
mod core;
mod ui;

use crate::ui::learn::*;

#[derive(Parser)]
#[command(name = "quizzy")]
pub struct Cli {
    #[command(Subcommand)]
    command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    New {
        deck: String,
        file: Option<PathBuf>,
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
    },
    Cards {
        deck: String,
    },
    Delete {
        deck: String,
    },
}

fn get_deck(deck: String) -> Deck {

}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::New { deck, file } => {
            println!("creating deck by name: {}", deck);
        }
        Command::Add {
            deck,
            term,
            definition,
        } => {}
        Command::Remove { deck, term } => {}
        Command::List { deck } => match deck {
            Some(name) => {
                println!("Listing out cards in deck: {}", name);
            }
            None => {
                println!("Listing out saved decks")
            }
        },
        Command::Learn { deck } => learn(get_deck(deck))
        Command::Cards { deck } => {}
        Command::Delete { deck } => {}
    }
}
