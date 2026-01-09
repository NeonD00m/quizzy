use crate::core::deck::*;
use crossterm::{
    event::{KeyCode, KeyModifiers, read},
    terminal::{disable_raw_mode, enable_raw_mode, size},
};
use rand::seq::SliceRandom;
use rand::thread_rng;
use std::cmp::max;

fn wrap_text(s: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        println!("What the helliante");
        return vec!["".to_string()];
    }

    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();

    for word in s.split_whitespace() {
        let word_len = word.chars().count();
        let cur_len = current.chars().count();

        if cur_len == 0 {
            // current line empty: if word fits, push, otherwise break the word
            if word_len <= max_width {
                current.push_str(word);
            } else {
                // break long word into chunks
                let mut start = 0;
                let chars: Vec<char> = word.chars().collect();
                while start < chars.len() {
                    let end = usize::min(start + max_width, chars.len());
                    let chunk: String = chars[start..end].iter().collect();
                    lines.push(chunk);
                    start = end;
                }
            }
        } else {
            // consider adding a space + word
            if cur_len + 1 + word_len <= max_width {
                current.push(' ');
                current.push_str(word);
            } else {
                // flush current and start new line
                lines.push(current);
                current = String::new();
                if word_len <= max_width {
                    current.push_str(word);
                } else {
                    // break long word into chunks
                    let mut start = 0;
                    let chars: Vec<char> = word.chars().collect();
                    while start < chars.len() {
                        let end = usize::min(start + max_width, chars.len());
                        let chunk: String = chars[start..end].iter().collect();
                        lines.push(chunk);
                        start = end;
                    }
                }
            }
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

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

fn cards_input() -> KeyCode {
    while let Ok(event) = read() {
        let Some(event) = event.as_key_press_event() else {
            continue;
        };
        if event.modifiers == KeyModifiers::CONTROL
            && (event.code == KeyCode::Char('c') || event.code == KeyCode::Char('d'))
        {
            return KeyCode::Esc;
        }
        if event.modifiers != KeyModifiers::NONE {
            println!("Ignoring input due to mofidier {:}\r", event.modifiers);
            continue;
        }
        if match event.code {
            KeyCode::Esc => true,
            KeyCode::Enter => true,
            KeyCode::Char(' ') => true,
            KeyCode::Left => true,
            KeyCode::Right => true,
            _ => false,
        } {
            return event.code;
        }
    }
    return KeyCode::Esc;
}

pub fn cards_mode(deck: Deck, shuffle: bool) -> anyhow::Result<()> {
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

    enable_raw_mode()?;
    loop {
        let option = cards.get(index);
        if option.is_none() {
            println!("No card found at index {}, exiting.", index);
            break;
        }
        let current = option.unwrap();
        if !flipped {
            println!("Term        (space/enter to flip, a for previous, d for next)")
        } else {
            println!("Definition  (space to flip, a for previous, d/enter for next)")
        }
        display_card(&current, flipped);
        match cards_input() {
            KeyCode::Char(' ') => {
                flipped = !flipped;
            }
            KeyCode::Left => {
                if index > 0 {
                    index -= 1;
                    flipped = false;
                } else {
                    println!("No previous card!");
                }
            }
            KeyCode::Right => {
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
        if index >= len {
            println!("Restarting from beginning. Press Escape to exit.");
            index = 0;
        }
    }
    disable_raw_mode()?;
    return Ok(());
}
