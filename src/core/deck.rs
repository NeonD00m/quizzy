use std::vec::Vec;

pub struct Card {
    term: String,
    definition: String,
}

pub struct Deck {
    name: String,
    // personal statistics?
    cards: Vec<Card>,
}
