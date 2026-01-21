use crate::core::deck::DeckSource::*;
use serde::Deserialize;
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
    pub id: Option<usize>,
}

impl Deck {
    pub fn named(name: String) -> Self {
        Self {
            name,
            cards: Vec::new(),
            id: None,
        }
    }

    pub fn from_cards(cards: Vec<Card>) -> Self {
        Self {
            name: String::from("Unnamed Deck"),
            cards,
            id: None,
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
        id: None,
    }
}

pub enum DeckSource {
    Named(String),
    File(PathBuf),
}

#[derive(Deserialize)]
struct JsonDeck {
    cards: Vec<JsonCard>,
}

#[derive(Deserialize)]
struct JsonCard {
    term: String,
    definition: String,
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

fn read_deck_tsv(path: PathBuf) -> anyhow::Result<Deck> {
    let file = File::open(path.as_path()).context("Failed to open file.")?;
    Ok(Deck::from_cards(
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
    ))
}

fn read_deck_csv(path: PathBuf) -> anyhow::Result<Deck> {
    let file = File::open(path.as_path()).context("Failed to open file.")?;
    Ok(Deck::from_cards(
        BufReader::new(file)
            .lines()
            .filter_map(|line| {
                if let Ok(line) = line {
                    let mut parts = line.split(",");
                    let term = parts.next()?;
                    let definition = parts.next()?;
                    Some(Card::new(term, definition))
                } else {
                    None
                }
            })
            .collect(),
    ))
}

fn read_deck_json(path: PathBuf) -> anyhow::Result<Deck> {
    let file = File::open(path.as_path()).context("Failed to open file.")?;
    let reader = BufReader::new(file);
    let json_deck: JsonDeck =
        serde_json::from_reader(reader).context("Failed to parse JSON deck.")?;
    Ok(Deck::from_cards(
        json_deck
            .cards
            .into_iter()
            .map(|jc| Card::new(&jc.term, &jc.definition))
            .collect(),
    ))
}

fn detect_read(path: PathBuf) -> anyhow::Result<Deck> {
    let ext = path
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "csv" => read_deck_csv(path),
        "tsv" => read_deck_tsv(path),
        "json" => read_deck_json(path),
        "txt" => read_deck_tsv(path),
        _ => {
            println!(
                "Unknown file extension '{}', defaulting to TSV format.",
                ext
            );
            read_deck_tsv(path)
        }
    }
}

pub fn get_deck(src: DeckSource) -> anyhow::Result<Deck> {
    match src {
        Named(_n) => Ok(example_deck()),
        File(p) => detect_read(p),
    }
}
