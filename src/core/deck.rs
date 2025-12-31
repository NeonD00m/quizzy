use crate::core::deck::DeckSource::*;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::vec::Vec;

#[derive(Clone)]
pub struct Card {
    pub term: String,
    pub definition: String,
}

impl Card {
    pub fn new(t: &str, d: &str) -> Self {
        Self {
            term: t.to_string(),
            definition: d.to_string(),
        }
    }
}

pub struct Deck {
    pub name: String,
    // personal statistics? probably in storage separately
    pub cards: Vec<Card>,
}

impl Deck {
    pub fn named(name: String) -> Self {
        Self {
            name,
            cards: Vec::new(),
        }
    }

    pub fn from_cards(cards: Vec<Card>) -> Self {
        Self {
            name: String::new(),
            cards,
        }
    }
}

pub fn example_deck() -> Deck {
    Deck {
        name: "EXAMPLE".to_string(),
        cards: vec![
            Card::new("hola", "hello"),
            Card::new("la cama", "the bed"),
            Card::new("la puerta", "the door"),
            Card::new("el reloj", "the watch"),
            Card::new("el libro", "the book"),
        ],
    }
}

pub enum DeckSource {
    Named(String),
    File(PathBuf),
}

pub fn resolve_deck_source(arg: &str) -> DeckSource {
    let path = Path::new(arg);

    // Rule 1: If the argument contains "/" or "\" -> path
    let is_explicit_path = arg.contains('/') || arg.contains('\\');

    // Rule 2: If it ends with known extension -> path
    let has_extension = path
        .extension()
        .and_then(|x| x.to_str())
        .map(|ext| matches!(ext, "txt" | "quiz"))
        .unwrap_or(false);

    // Rule 3: If the file actually exists -> path
    let exists = path.exists();

    if is_explicit_path || has_extension || exists {
        DeckSource::File(path.to_path_buf())
    } else {
        DeckSource::Named(arg.to_string())
    }
}

pub fn import_deck(path: PathBuf) -> Deck {
    let file = File::open(path.as_path()).expect("Failed to open file.");
    Deck::from_cards(
        BufReader::new(file)
            .lines()
            .filter_map(|line| {
                if let Ok(line) = line {
                    let mut parts = line.split("\t");
                    let term = parts.next()?;
                    let definition = parts.next()?;
                    Some(Card::new(term, definition))
                } else {
                    None
                }
            })
            .collect(),
    )
}

pub fn get_deck(src: DeckSource) -> Deck {
    match src {
        Named(n) => example_deck(),
        File(p) => import_deck(p),
    }
}
