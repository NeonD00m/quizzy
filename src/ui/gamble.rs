use crate::core::deck::*;
use crate::core::learn::get_multiple_choice_for_card;
use anyhow::Context;
use crossterm::{
    ExecutableCommand, QueueableCommand, cursor,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers, read},
    style::{Color, Print, SetForegroundColor},
    terminal::{Clear, ClearType, size},
};
use rand::seq::SliceRandom;
use rand::{rngs::ThreadRng, thread_rng};
use std::cmp::max;
use std::io::{Write, stdout};
use std::time::{Duration, Instant};

enum GameResult {
    Answer(usize), // Returns index 1-4
    Bank,
    Double,
    Exit,
    Timeout,
}

// This function handles the "High Pressure" input
fn read_input_with_timer(time_limit: Duration) -> std::io::Result<GameResult> {
    let start = Instant::now();
    let mut stdout = stdout();

    // How wide is the timer bar?
    let bar_width = 30;

    loop {
        let elapsed = start.elapsed();
        if elapsed >= time_limit {
            return Ok(GameResult::Timeout);
        }

        // 1. CALCULATE PROGRESS
        let remaining = time_limit - elapsed;
        let percent_left = remaining.as_secs_f32() / time_limit.as_secs_f32();
        let fill_count = (percent_left * bar_width as f32).ceil() as usize;

        // 2. RENDER TIMER BAR (Using Carriage Return \r to overwrite line)
        // We use ANSI colors to make it look urgent (Red if low, Green if high)
        let color = if percent_left < 0.3 {
            Color::Red
        } else {
            Color::Green
        };

        let bar_str = format!(
            "[{}{}] {:.1}s",
            "#".repeat(fill_count),
            "-".repeat(bar_width - fill_count),
            remaining.as_secs_f32()
        );

        stdout
            .queue(cursor::MoveToColumn(0))?
            .queue(Clear(ClearType::CurrentLine))?
            .queue(Print("Time: "))?
            .queue(SetForegroundColor(color))?
            .queue(Print(bar_str))?
            .queue(SetForegroundColor(Color::Reset))?
            .flush()?;

        // 3. POLL FOR INPUT (Non-blocking check)
        // We wait up to 50ms for an event. If no event, we loop back to update timer.
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('1') => return Ok(GameResult::Answer(1)),
                        KeyCode::Char('2') => return Ok(GameResult::Answer(2)),
                        KeyCode::Char('3') => return Ok(GameResult::Answer(3)),
                        KeyCode::Char('4') => return Ok(GameResult::Answer(4)),
                        KeyCode::Char('b') | KeyCode::Char('B') => return Ok(GameResult::Bank),
                        KeyCode::Char('d') | KeyCode::Char('D') => return Ok(GameResult::Double),
                        KeyCode::Esc => return Ok(GameResult::Exit),
                        _ => {} // Ignore other keys
                    }
                }
            }
        }
    }
}

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
        if matches!(
            event.code,
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right
        ) {
            return event.code;
        }
    }
    KeyCode::Esc
}

pub fn gauntlet_mode(deck: &Deck, bank: i64) -> anyhow::Result<()> {
    let mut current_streak = 0;
    let mut rng = thread_rng();
    let cards = deck.cards;
    let deck_size = cards.len();
    let mut bucket: Vec<usize> = Vec::new();

    // refill bucket by cloning cards and shuffling
    fn refill_bucket(cards: &[Card], bucket: &mut Vec<usize>, rng: &mut ThreadRng) {
        bucket.clear();
        for i in 0..cards.len() {
            bucket.push(i);
        }
        bucket.shuffle(rng);
    }

    loop {
        // 1. Clear screen and show Header
        print!("\x1B[2J\x1B[1;1H"); // ANSI clear screen code
        println!("========================================");
        println!("           STUDY CASINO: OPEN           ");
        println!("========================================");
        println!("DECK: {}", deck.name);
        println!("BANK: {}", bank);
        println!("STREAK: {}", current_streak);
        println!("----------------------------------------");

        // 2. Get random card logic here...
        if bucket.is_empty() {
            refill_bucket(&cards, &mut bucket, &mut rng);
        }
        let index = bucket.pop().context("Bucket unexpected empty.")?;
        let card = &cards.get(index).context("Expected card for index.")?;

        // 3. Draw Card Front
        display_card(&card, false);

        // 4. Double Down Prompt (Standard blocking input)
        print!("> Double down (2x risk/reward)? (y/n): ");
        stdout().flush()?;
        let mut double_input = String::new();
        std::io::stdin().read_line(&mut double_input)?;
        let is_doubled = double_input.trim().eq_ignore_ascii_case("y");

        // 5. Display Options
        println!("What's on the other side?");
        let choices = get_multiple_choice_for_card(card, &cards, &mut rng, false, None);
        println!(
            "(1) {}\t\t\t(2) {}\n(3) {}\t\t\t(4) {}",
            choices[0].definition,
            choices[1].definition,
            choices[2].definition,
            choices[3].definition,
        );

        // 6. START THE TIMER LOOP
        // Base time is 10s, minus 1s for every streak level (min 3s)
        let time_allowed = Duration::from_secs(std::cmp::max(5, 20 - current_streak));

        // Wait for result
        let result = read_input_with_timer(time_allowed)?;

        // 7. Handle Result
        match result {
            GameResult::Timeout => {
                println!("\n\nBUST! You ran out of time.");
                bank -= 100; // Penalty
                current_streak = 0;
            }
            GameResult::Answer(idx) => {
                if idx == card.correct_index {
                    println!("\n\n✓ Correct!");
                    let reward = if is_doubled { 200 } else { 100 };
                    bank += reward;
                    current_streak += 1;
                } else {
                    println!("\n\nX Wrong! Answer was: {}", card.correct_answer);
                    bank -= if is_doubled { 200 } else { 100 };
                    current_streak = 0;
                }
            }
            GameResult::Bank => {
                println!("\n\n$$ Banked safely.");
                // TODO: immediately save currency
            }
            GameResult::Exit => break,
            _ => {}
        }

        // 8. Pause before next round so they can see the result
        std::thread::sleep(Duration::from_secs(2));
    }
    Ok(())
}

pub fn gamble_mode(deck: Deck, shuffle: bool) -> anyhow::Result<()> {
    Ok(())
}
