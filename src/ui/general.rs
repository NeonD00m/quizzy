use crate::core::deck::{DeckSource, resolve_deck_source};
use crate::core::storage::{DeckListItem, Storage};
use crate::ui::input::{enter_input, type_input};
use anyhow::Context;
use chrono::{TimeZone, Utc};
use crossterm::event::KeyCode;
use std::io::{Write, stdout};

/// Helper to select a deck by name. If multiple exist, prompts the user.
fn select_deck_by_name(
    storage: &Storage,
    name: &str,
    action_verb: &str,
) -> anyhow::Result<Option<DeckListItem>> {
    let mut matches: Vec<_> = storage
        .list_decks_detailed()?
        .into_iter()
        .filter(|item| item.name == name)
        .collect();

    if matches.is_empty() {
        println!("No decks found by the name '{}'.", name);
        return Ok(None);
    }

    if matches.len() == 1 {
        return Ok(Some(matches.remove(0)));
    }

    println!("Found the following decks with the name '{}':", name);
    for item in &matches {
        let date_str = Utc
            .timestamp_opt(item.created_at, 0)
            .unwrap()
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        println!(
            "ID: {}\t| Name: {}\t| Created: {}\t| Cards: {}",
            item.id, item.name, date_str, item.card_count
        );
    }

    loop {
        let prompt = format!("Enter the ID of the deck you want to {} ", action_verb);
        if let Some(response) = type_input(&prompt)? {
            if let Ok(id) = response.parse::<i64>() {
                if let Some(idx) = matches.iter().position(|item| item.id == id) {
                    println!("\n");
                    return Ok(Some(matches.remove(idx)));
                } else {
                    println!("Invalid ID entered. Please try again.");
                }
            } else {
                println!("Please enter a valid number.");
            }
        } else {
            return Ok(None);
        }
    }
}

pub fn add(
    storage: &mut Storage,
    deck_name: String,
    term: String,
    definition: String,
) -> anyhow::Result<()> {
    match resolve_deck_source(deck_name.as_str()) {
        DeckSource::Named(name) => {
            if let Some(deck) = select_deck_by_name(storage, &name, "add to")? {
                storage.add_card_to_deck(deck.id, &term, &definition)?;
                println!("Added card to deck '{}'", deck.name);
            }
        }
        DeckSource::File(_) => {
            println!("To add to a file-backed deck, create or find a saved deck first.");
        }
    }
    Ok(())
}

pub fn remove(storage: &mut Storage, deck_name: String, term: String) -> anyhow::Result<()> {
    match resolve_deck_source(deck_name.as_str()) {
        DeckSource::Named(name) => {
            if let Some(deck_info) = select_deck_by_name(storage, &name, "remove from")? {
                let deck = storage.get_deck_by_id(deck_info.id)?;
                // find card id
                if let Some((card_id, _, _)) = deck
                    .cards
                    .iter()
                    .filter(|c| c.term == term)
                    .map(|c| (c.id, c.term.clone(), c.definition.clone()))
                    .find(|(id, _, _)| id.is_some())
                {
                    storage.remove_card(card_id.unwrap())?;
                    println!("Removed card '{}' from deck '{}'", term, deck.name);
                } else {
                    println!("No matching card '{}' found in deck '{}'", term, deck.name);
                }
            }
        }
        DeckSource::File(_) => {
            println!("Cannot remove from file-backed deck. Create or save a deck from it first.");
        }
    }
    Ok(())
}

pub fn delete(storage: &mut Storage, name: String) -> anyhow::Result<()> {
    if let Some(info) = select_deck_by_name(storage, &name, "delete")? {
        println!("Are you sure you want to delete this deck and all its associated stats?");
        print!("Press [ENTER] to confirm deletion or [ESC] to cancel > ");
        stdout().flush().context("Failed to flush output.")?;

        if enter_input()? == KeyCode::Enter {
            storage.delete_deck_by_id(info.id)?;
            println!(
                "\nSuccessfully deleted deck '{}' and all associated stats.",
                info.name
            );
        } else {
            println!("\nDeletion cancelled.");
        }
    }
    Ok(())
}

pub fn clear(storage: &mut Storage, deck_name: String) -> anyhow::Result<()> {
    match resolve_deck_source(deck_name.as_str()) {
        DeckSource::Named(name) => {
            if let Some(deck) = select_deck_by_name(storage, &name, "clear")? {
                println!(
                    "Are you sure you want to clear all cards from deck '{}'?",
                    deck.name
                );
                print!("Press [ENTER] to confirm or [ESC] to cancel > ");
                stdout().flush().context("Failed to flush output.")?;

                if enter_input()? == KeyCode::Enter {
                    storage.clear_deck(deck.id)?;
                    println!(
                        "\nSuccessfully cleared all cards from deck '{}'.",
                        deck.name
                    );
                } else {
                    println!("\nClear cancelled.");
                }
            }
        }
        DeckSource::File(_) => {
            println!("Cannot clear a file-backed deck. Use the deck name of a saved deck.");
        }
    }
    Ok(())
}

pub fn rename(storage: &mut Storage, deck_name: String, new_name: String) -> anyhow::Result<()> {
    if let Some(deck) = select_deck_by_name(storage, &deck_name, "rename")? {
        storage.rename_deck(deck.id, &new_name)?;
        println!(
            "Successfully renamed deck '{}' to '{}'.",
            deck.name, new_name
        );
    }
    Ok(())
}

pub fn new(
    storage: &mut Storage,
    name: String,
    source_arg: Option<String>,
) -> anyhow::Result<()> {
    println!("creating deck by name: {}", name);
    let deck = if let Some(source) = source_arg {
        match resolve_deck_source(source.as_str()) {
            DeckSource::File(path) => {
                println!("Reading cards from file {}...", path.display());
                let mut d = crate::core::deck::read_deck_from_file(path)?;
                d.name = name.clone();
                d
            }
            DeckSource::Named(src_name) => {
                if let Some(source_info) = select_deck_by_name(storage, &src_name, "clone from")? {
                    println!("Cloning cards from deck '{}'...", source_info.name);
                    let mut d = storage.get_deck_by_id(source_info.id)?;
                    d.name = name.clone();
                    // strip IDs so they are treated as new cards
                    for card in &mut d.cards {
                        card.id = None;
                    }
                    d
                } else {
                    return Ok(()); // selection cancelled
                }
            }
        }
    } else {
        crate::core::deck::Deck::named(name.clone())
    };

    println!("Saving deck {}", deck.name);
    let deck_id = storage.create_deck_from_core(deck, None, None)?;
    println!("Successfully saved deck. ({})", deck_id);
    Ok(())
}

pub fn append(
    storage: &mut Storage,
    deck_name: String,
    source_arg: String,
) -> anyhow::Result<()> {
    let target_deck = match resolve_deck_source(deck_name.as_str()) {
        DeckSource::Named(name) => select_deck_by_name(storage, &name, "append to")?,
        DeckSource::File(_) => {
            println!("Cannot append to a file-backed deck directly. Save it to the database first.");
            None
        }
    };

    if let Some(target) = target_deck {
        let cards_to_append = match resolve_deck_source(source_arg.as_str()) {
            DeckSource::File(path) => {
                println!("Reading cards from file {}...", path.display());
                crate::core::deck::read_deck_from_file(path)?.cards
            }
            DeckSource::Named(name) => {
                if let Some(source_info) = select_deck_by_name(storage, &name, "append from")? {
                    println!("Reading cards from deck '{}'...", source_info.name);
                    storage.get_deck_by_id(source_info.id)?.cards
                } else {
                    return Ok(()); // selection cancelled
                }
            }
        };

        let count = cards_to_append.len();
        if count == 0 {
            println!("No cards found to append.");
            return Ok(());
        }

        storage.add_cards_to_deck_batch(target.id, cards_to_append)?;
        println!(
            "Successfully appended {} cards to deck '{}'.",
            count, target.name
        );
    }
    Ok(())
}

