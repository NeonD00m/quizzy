use crate::core::storage::Storage;
use crate::ui::general::select_deck_by_name;
use crate::ui::input::choice_input;
use crate::ui::{
    input::{cards_input, key_input},
    wrap_text,
};
use crate::{core::deck::*, ui::input::RawModeGuard};
use anyhow::Context;
use crossterm::{event::KeyCode, terminal::size};
use rand::seq::SliceRandom;
use rand::thread_rng;
use std::cmp::max;
use std::collections::{HashMap, VecDeque};

fn display_card(c: &Card, flipped: bool) {
    let (term_w, _term_h) = size().unwrap_or((80, 24)); // 80x24 fallback
    let content = if flipped { &c.definition } else { &c.term };
    let hidden = if !flipped { &c.definition } else { &c.term };
    let term_width = term_w as usize;

    // sizing math
    let max_content_width = term_width.saturating_sub(6).max(1);
    let mut wrapped = wrap_text(content.trim(), max_content_width);
    let wrapped_hidden = wrap_text(hidden.trim(), max_content_width);
    // get the longest line length from either side of the card
    let max_line_len = wrapped.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    let max_line_len2 = wrapped_hidden
        .iter()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(0);
    // if the hidden side of the card has more lines, add vertical space
    let diff = wrapped_hidden.len().saturating_sub(wrapped.len());
    if diff > 0 {
        let top = diff / 2; // round down
        let bottom = diff - top; // round up
        // let mut pre = Vec::with_capacity(top);
        // pre.fill("".to_string());
        wrapped.splice(0..0, vec!["".to_string(); top]);
        for _ in 0..bottom {
            wrapped.push("".to_string());
        }
    }
    // finalize card length
    let mut len = 4 + max(max_line_len, max_line_len2);
    if len + 2 > term_width {
        len = term_width.saturating_sub(2);
    }

    println!("╭{:─^len$}╮", "", len = len);
    for line in wrapped.iter() {
        println!("|{:^len$}|", line, len = len);
    }
    println!("╰{:─^len$}╯", "", len = len);
}

pub fn cards_mode(deck: Option<Deck>, shuffle: bool, storage: &mut Storage) -> anyhow::Result<()> {
    let deck = match deck {
        Some(d) => d,
        None => {
            if let Some(item) = select_deck_by_name(storage, "", "study")? {
                let d = storage.get_deck_by_id(item.id)?;
                let _ = storage.update_user_last_active();
                if let Some(id) = d.id {
                    let _ = storage.update_deck_last_studied(id);
                }
                d
            } else {
                return Ok(());
            }
        }
    };
    println!("To see options like -s for shuffling, use `quizzy help cards`");
    let mut flipped = false;
    let mut index: usize = 0;
    let mut cards = deck.cards;
    let len = cards.len();

    println!(
        "Beginning practice of {}. Press Escape at any time to end the session.",
        deck.name
    );
    if shuffle {
        let mut rng = thread_rng();
        cards.shuffle(&mut rng);
    }

    let _guard = RawModeGuard::new();
    loop {
        let option = cards.get(index);
        if option.is_none() {
            println!("No card found at index {}, exiting.", index);
            break;
        }
        let current = option.context("Expected current card since option was not none.")?;
        if !flipped {
            println!("Term        (space/enter to flip, a for previous, d for next)")
        } else {
            println!("Definition  (space to flip, a for previous, d/enter for next)")
        }
        display_card(current, flipped);
        match cards_input() {
            KeyCode::Char(' ') => {
                flipped = !flipped;
            }
            KeyCode::Left | KeyCode::Char('a') | KeyCode::Char('A') => {
                if index > 0 {
                    index -= 1;
                    flipped = false;
                } else {
                    println!("No previous card!");
                }
            }
            KeyCode::Right | KeyCode::Char('d') | KeyCode::Char('D') => {
                index += 1;
                flipped = false;
            }
            KeyCode::Enter => {
                flipped = !flipped;
                if !flipped {
                    index += 1;
                }
            }
            _ => {
                break;
            }
        }

        println!();
        if index >= len {
            println!("Restarting from beginning. Press Escape to exit.");
            index = 0;
        }
    }
    Ok(())
}

pub fn cram_mode(deck: Option<Deck>, storage: &mut Storage) -> anyhow::Result<()> {
    let deck = match deck {
        Some(d) => d,
        None => {
            if let Some(item) = select_deck_by_name(storage, "", "cram")? {
                let d = storage.get_deck_by_id(item.id)?;
                let _ = storage.update_user_last_active();
                if let Some(id) = d.id {
                    let _ = storage.update_deck_last_studied(id);
                }
                d
            } else {
                return Ok(());
            }
        }
    };
    println!("Cram mode: We're going to get you through this buddy.");
    let deck_id = deck
        .id
        .context("Deck must be saved to database to study.")?;
    let weak_cards = storage.get_weakest_cards(deck_id, 20)?;
    if weak_cards.is_empty() {
        println!("No cards to cram! Add some cards to your deck.");
        return Ok(());
    }

    let mut queue: VecDeque<Card> = VecDeque::new();
    for (card, _) in weak_cards {
        queue.push_back(card);
    }
    let mut session_deltas: HashMap<i64, i64> = HashMap::new();

    let _guard = RawModeGuard::new();

    // The Survival Loop
    'cram_loop: while let Some(current) = queue.pop_front() {
        println!(
            "Term (space/enter to flip) | Cards left: {}",
            queue.len() + 1
        );
        display_card(&current, false);

        // Wait for flip or exit
        match key_input(KeyCode::Char(' '))? {
            KeyCode::Esc => {
                queue.push_front(current); // put it back before breaking
                break;
            }
            _ => { /* Flipped! */ }
        }

        display_card(&current, true);
        println!("Did you know it? (1: No / Again, 2: Yes / Was close)");

        loop {
            match choice_input()? {
                KeyCode::Char('1') => {
                    if let Some(id) = current.id {
                        *session_deltas.entry(id).or_insert(0) -= 1;
                    }
                    queue.push_back(current); // Push to back to see again
                    break;
                }
                KeyCode::Char('2') => {
                    if let Some(id) = current.id {
                        *session_deltas.entry(id).or_insert(0) += 1;
                    }
                    // Card is done, drops out of queue
                    break;
                }
                KeyCode::Esc => {
                    queue.push_front(current); // Put card back before exiting
                    break 'cram_loop;
                }
                _ => continue, // Invalid input, loop again and wait for 1, 2, or Esc
            }
        }
        println!();
    }
    drop(_guard); // manually drop so we can print

    // Commit the short-term progress to the database
    if !session_deltas.is_empty() {
        let updates: Vec<(i64, i64)> = session_deltas.into_iter().collect();
        storage.commit_cram_session(&updates)?;
        println!("Saved cram session progress!");
    } else {
        println!("Session ended with no changes.");
    }
    Ok(())
}
