use crate::core::deck::{DeckSource, resolve_deck_source};
use crate::core::storage::Storage;
use crate::ui::input::{enter_input, type_input};
use anyhow::Context;
use chrono::TimeZone;
use chrono::Utc;
use crossterm::{
    QueueableCommand, cursor,
    event::KeyCode,
    style::Print,
    terminal::{Clear, ClearType},
};
use std::io::{Write, stdout};

pub fn add(
    storage: &mut Storage,
    deck: String,
    term: String,
    definition: String,
) -> anyhow::Result<()> {
    // make sure user isn't trying to add to a file-based deck
    let deck = (match resolve_deck_source(deck.as_str()) {
        DeckSource::Named(name) => Some(storage.get_deck_by_name(&name)?),
        DeckSource::File(_) => {
            println!("To add to a file-backed deck, create or find a saved deck first.");
            None
        }
    })
    .ok_or(anyhow::anyhow!("Cannot add to a file-backed deck."))?;
    if let Some(deck_id) = deck.id {
        storage.add_card_to_deck(deck_id, &term, &definition)?;
        println!("Added card to deck '{}'", deck.name);
    } else {
        anyhow::bail!("Missing deck id from storage?")
    }
    Ok(())
}

pub fn remove(storage: &mut Storage, deck: String, term: String) -> anyhow::Result<()> {
    println!(
        "Removing the first matching term ({}) from deck {}",
        term, deck
    );
    let deck = (match resolve_deck_source(deck.as_str()) {
        DeckSource::Named(name) => Some(storage.get_deck_by_name(&name)?),
        DeckSource::File(_) => {
            println!("Cannot remove from file-backed deck. Create or save a deck from it first.");
            None
        }
    })
    .ok_or(anyhow::anyhow!("Cannot remove from file-backed deck."))?;
    // find card id
    if let Some((card_id, _, _)) = deck
        .cards
        .iter()
        .filter(|c| c.term == term) // \/ does the card need to be cloned here?
        .map(|c| (c.id, c.term.clone(), c.definition.clone())) // could &str be used?
        .find(|(id, _, _)| id.is_some())
    {
        storage.remove_card(card_id.unwrap())?;
        println!("Removed card '{}' from deck '{}'", term, deck.name);
    } else {
        println!("No matching card '{}' found in deck '{}'", term, deck.name);
    }
    Ok(())
}

pub fn delete(storage: &mut Storage, name: String) -> anyhow::Result<()> {
    let mut matches: Vec<_> = storage
        .list_decks_detailed()?
        .into_iter()
        .filter(|item| item.name == name)
        .collect();

    if matches.is_empty() {
        println!("No decks found by that name.");
        return Ok(());
    } else {
        println!("Found the following decks:");
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
    }

    while matches.len() > 1 {
        if let Some(response) = type_input("Enter one of the deck's id number below to delete ")? {
            if let Ok(id) = response.parse::<i64>() {
                if matches.iter().any(|item| item.id == id) {
                    // filter matches to just the one with the matching id
                    matches = matches.into_iter().filter(|item| item.id == id).collect();
                    println!("\n\n");
                    break;
                } else {
                    stdout()
                        .queue(cursor::MoveDown(1))?
                        .queue(Clear(ClearType::CurrentLine))?
                        .queue(cursor::MoveDown(1))?
                        .queue(Print("Invalid ID entered. Please try again."))?
                        .queue(cursor::MoveDown(1))?
                        .flush()
                        .context("Failed to flush output.")?;
                }
            } else {
                stdout()
                    .queue(cursor::MoveDown(1))?
                    .queue(Clear(ClearType::CurrentLine))?
                    .queue(cursor::MoveDown(1))?
                    .queue(Print("Please enter a valid number."))?
                    .queue(cursor::MoveDown(1))?
                    .flush()
                    .context("Failed to flush output.")?;
            }
        } else {
            println!("\n\nDeletion cancelled.");
            return Ok(());
        }
    }

    if let Some(info) = matches.get(0) {
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
