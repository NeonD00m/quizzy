use crate::core::learn::get_multiple_choice_for_card;
use crate::core::{deck::*, storage::Storage};
use crate::ui::input::{RoundAction, enter_input, read_input_with_fuse};
use anyhow::Context;
use crossterm::{event::KeyCode, terminal::size};
use rand::seq::SliceRandom;
use rand::{rngs::ThreadRng, thread_rng};
use std::cmp::max;
use std::io::{Write, stdout};
use std::time::{Duration, Instant};

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

fn gauntlet_reward(consecutive: i64) -> i64 {
    (consecutive * 50) + 100
}

pub fn gauntlet_mode(deck: Deck, storage: &mut Storage) -> anyhow::Result<()> {
    let mut rng = thread_rng();
    let cards = deck.cards;
    let mut bucket: Vec<usize> = Vec::new();
    let mut balance = storage.get_currency()?;
    let mut current_streak = storage.get_streak()?;

    // refill bucket by cloning cards and shuffling
    fn refill_bucket(cards: &[Card], bucket: &mut Vec<usize>, rng: &mut ThreadRng) {
        bucket.clear();
        for i in 0..cards.len() {
            bucket.push(i);
        }
        bucket.shuffle(rng);
    }

    'main: loop {
        // 1. Clear screen and show Header
        // print!("\x1B[2J\x1B[1;1H"); // ANSI clear screen code
        println!("\n\n\n");
        println!("========================================");
        println!("           STUDY CASINO: OPEN           ");
        println!("========================================");
        println!("DECK: {}", deck.name);
        println!("BANK: {}", balance);
        println!("STREAK: {}", current_streak);
        println!("----------------------------------------");
        print!("Press [ENTER] to deal the first card or [ESC] to cancel > ");
        stdout().flush().context("Failed to flush output.")?;
        let prompt = enter_input();
        if prompt? == KeyCode::Esc {
            println!("\nEnded Gauntlet session.");
            return Ok(());
        }
        println!();
        'streak: loop {
            // 2. Get random card logic here...
            if bucket.is_empty() {
                refill_bucket(&cards, &mut bucket, &mut rng);
            }
            let index = bucket.pop().context("Bucket unexpected empty.")?;
            let card = &cards.get(index).context("Expected card for index.")?;

            // DISPLAY CARD AND OPTIONS
            println!();
            display_card(card, false);
            let mut bet = gauntlet_reward(current_streak);
            let mut is_doubled = false;
            println!("What's on the other side?\t\tBet: ${}", bet);
            let choices = get_multiple_choice_for_card(card, &cards, &mut rng, false, None);
            println!(
                "(1) {}\t\t\t(2) {}\n(3) {}\t\t\t(4) {}",
                choices[0].definition,
                choices[1].definition,
                choices[2].definition,
                choices[3].definition,
            );

            // START THE INPUT LOOP
            let now = Instant::now();
            let mut time_allowed = std::cmp::max(10, 20 - current_streak) as f64;
            'input: loop {
                let result = read_input_with_fuse(
                    time_allowed as u64,
                    "Enter 1-4, \"DOUBLE\", or \"BANK\" ",
                )?;
                match result {
                    RoundAction::Timeout => {
                        println!("\n\nBUST! You ran out of time.");
                        balance -= bet; // Penalty
                        storage.update_currency(-bet)?;
                        storage.update_streak(-current_streak)?;
                        current_streak = 0;
                        break 'streak;
                    }
                    RoundAction::Answer(num_char) => {
                        let idx = match num_char {
                            '1' => 0,
                            '2' => 1,
                            '3' => 2,
                            _ => 3,
                        };
                        if let Some(choice) = choices.get(idx)
                            && choice == *card
                        {
                            println!("\n\n✓ Correct!");
                            balance += bet;
                            storage.update_currency(bet)?;
                            storage.update_streak(1)?;
                            current_streak += 1;
                            break 'input;
                        } else {
                            println!("\n\nX Wrong! Answer was: {}", card.definition);
                            balance -= bet;
                            storage.update_currency(-bet)?;
                            storage.update_streak(-current_streak)?;
                            current_streak = 0;
                            break 'streak;
                        }
                    }
                    RoundAction::Bank => {
                        println!("\n\n$$$ Banked safely.");
                        // don't subtract from balance
                        break 'streak;
                    }
                    RoundAction::Double => {
                        if is_doubled {
                            println!("ALREADY DOUBLED DOWN. RESUMING TIMER.");
                            time_allowed -= now.elapsed().as_secs_f64();
                            continue 'input;
                        }
                        if balance < 2 * bet {
                            println!("NOT ENOUGH BALANCE TO DOUBLE DOWN. RESUMING TIMER.");
                            time_allowed -= now.elapsed().as_secs_f64();
                            continue 'input;
                        }
                        // timer resets when you double, should I change that?
                        bet *= 2;
                        is_doubled = true;
                        println!("DOUBLE DOWN ACTIVATED! RESET TIMER. Bet: ${}", bet);
                        stdout().flush().context("Failed to flush output")?;
                    }
                    RoundAction::Exit => {
                        println!("Quit game. Lost bet and streak of {}.", current_streak);
                        storage.update_currency(-bet)?;
                        storage.update_streak(-current_streak)?;
                        break 'main;
                    }
                }
            }
            // 8. Pause before next round so they can see the result
            std::thread::sleep(Duration::from_secs(2));
        }
        print!(
            "Session ended with {} successful answers! > ",
            current_streak
        );
        stdout().flush().context("Failed to flush output.")?;
        enter_input()?;
    }
    Ok(())
}
