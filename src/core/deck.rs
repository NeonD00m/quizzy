use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::vec::Vec;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Card {
    pub id: Option<i64>, // the database id when persisted
    pub term: String,
    pub definition: String,
}

impl Card {
    pub fn new(t: &str, d: &str) -> Self {
        Self {
            id: None,
            term: t.to_string(),
            definition: d.to_string(),
        }
    }

    // Explict semantic comparison of cards' contents, ignoring id
    pub fn same(&self, other: &Card) -> bool {
        self.term == other.term && self.definition == other.definition
    }

    // Explict semantic contrast of cards' contents, ignoring id
    pub fn different(&self, other: &Card) -> bool {
        self.term != other.term && self.definition != other.definition
    }
}

pub struct Deck {
    pub name: String,
    // personal statistics? probably in storage separately
    pub cards: Vec<Card>,
    pub id: Option<i64>,
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

// todo: maybe implement a tutorial deck?
// pub fn example_deck() -> Deck {
//     Deck {
//         name: "EXAMPLE".to_string(),
//         cards: vec![
//             Card::new("hola", "hello"),
//             Card::new("la cama", "the bed"),
//             Card::new("la puerta", "the door"),
//             Card::new("el reloj", "the watch"),
//             Card::new("el libro", "the book"),
//         ],
//         id: None,
//     }
// }

pub enum DeckSource {
    Named(String),
    File(PathBuf),
}

#[derive(Serialize, Deserialize)]
struct JsonDeck {
    cards: Vec<JsonCard>,
}

#[derive(Serialize, Deserialize)]
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
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_path(path)
        .context("Failed to open CSV file.")?;

    let mut cards = Vec::new();
    for result in rdr.records() {
        let record = result.context("Failed to read CSV record.")?;
        if record.len() >= 2 {
            cards.push(Card::new(&record[0], &record[1]));
        }
    }

    Ok(Deck::from_cards(cards))
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

pub fn read_deck_from_file(path: PathBuf) -> anyhow::Result<Deck> {
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

fn write_deck_csv(deck: &Deck, path: PathBuf) -> anyhow::Result<()> {
    let mut wtr = csv::Writer::from_path(path).context("Failed to create CSV writer.")?;
    for card in &deck.cards {
        wtr.write_record([&card.term, &card.definition])
            .context("Failed to write CSV record.")?;
    }
    wtr.flush().context("Failed to flush CSV writer.")?;
    Ok(())
}

fn write_deck_tsv(deck: &Deck, path: PathBuf) -> anyhow::Result<()> {
    let mut wtr = csv::WriterBuilder::new()
        .delimiter(b'\t')
        .from_path(path)
        .context("Failed to create TSV writer.")?;
    for card in &deck.cards {
        wtr.write_record([&card.term, &card.definition])
            .context("Failed to write TSV record.")?;
    }
    wtr.flush().context("Failed to flush TSV writer.")?;
    Ok(())
}

fn write_deck_json(deck: &Deck, path: PathBuf) -> anyhow::Result<()> {
    let file = File::create(path).context("Failed to create JSON file.")?;
    let json_deck = JsonDeck {
        cards: deck
            .cards
            .iter()
            .map(|c| JsonCard {
                term: c.term.clone(),
                definition: c.definition.clone(),
            })
            .collect(),
    };
    serde_json::to_writer_pretty(file, &json_deck).context("Failed to write JSON deck.")?;
    Ok(())
}

pub fn write_deck_to_file(deck: &Deck, path: PathBuf) -> anyhow::Result<()> {
    let ext = path
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "csv" => write_deck_csv(deck, path),
        "tsv" => write_deck_tsv(deck, path),
        "json" => write_deck_json(deck, path),
        "txt" => write_deck_tsv(deck, path),
        _ => {
            println!(
                "Unknown file extension '{}', defaulting to TSV format.",
                ext
            );
            write_deck_tsv(deck, path)
        }
    }
}

// still debating if I just make this use the storage or what???
// fn get_deck(src: DeckSource) -> anyhow::Result<Deck> {
//     match src {
//         DeckSource::Named(_n) => {
//             println!(
//                 "Warning: Tried to obtain named deck without storage, returning example deck."
//             );
//             Ok(example_deck())
//         }
//         DeckSource::File(p) => read_deck_from_file(p),
//     }
// }
