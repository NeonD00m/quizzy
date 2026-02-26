use crate::core::storage::Storage;
use crate::ui::{input::cards_input, wrap_text};
use crate::{core::deck::*, ui::input::RawModeGuard};
use anyhow::Context;
use crossterm::{event::KeyCode, terminal::size};
use rand::seq::SliceRandom;
use rand::thread_rng;
use std::cmp::max;

/// View the statistics of a specific card, maybe even timeframe?
fn full_card_stats(deck: Deck, index: i32, storage: &mut Storage) {}

/// View the overview of a deck's stats, with detailed statistics per card
fn deck_by_card(deck: Deck, size: u32, page: u32, storage: &mut Storage) {}

/// View the overview of a deck's stats, cards organized by learning progress
fn deck_by_category(deck: Deck, size: u32, page: u32, storage: &mut Storage) {}

/// View general numbers of all saved decks
fn overview(size: u32, page: u32, storage: &mut Storage) {}

#[allow(clippy::code)]
pub fn stats_mode(
    deck: Option<Deck>,
    size: u32,
    page: u32,
    storage: &mut Storage,
) -> anyhow::Result<()> {
    // if no deck, display overview first for all decks, in order of recently studied
    // allow user to use the indices to inspect further or esc to "go back"
    // save page number at each step of the way for back functionality
    Ok(())
}
