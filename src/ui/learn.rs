use crate::core::deck::*;
use crate::core::learn::*;
use crate::core::storage::Storage;
use crate::core::string_distance::string_distance;
use crate::ui::input::type_input;
use crate::ui::input::{choice_input, enter_input};
use anyhow::Context;
use core::f64;
use crossterm::event::KeyCode;
use rand::rngs::ThreadRng;
use rand::seq::SliceRandom;
use rand::thread_rng;
use std::collections::{HashMap, HashSet};
use std::io::Write as IoWrite;
use std::io::stdout;
use std::time::Duration;

pub fn display_multiple_choice(choices: &[Card], ask_term: bool) {
    let (width, _) = crossterm::terminal::size().unwrap_or((80, 24));
    let width = width as usize;

    // midpoint of screen, but cap it to avoid extreme spacing
    let midpoint = std::cmp::min(width / 2, 50);
    // column padding and max width for text itself
    let col_padding = 4;
    let max_col_width = midpoint.saturating_sub(col_padding);

    fn get_choice_text(c: &Card, ask_term: bool) -> String {
        if ask_term {
            c.definition.clone()
        } else {
            c.term.clone()
        }
    }

    // helper to print two wrapped strings side-by-side
    let print_row = |idx1: usize, idx2: usize| {
        let text1 = format!(
            "({}) {}",
            idx1 + 1,
            get_choice_text(&choices[idx1], ask_term)
        );
        let text2 = format!(
            "({}) {}",
            idx2 + 1,
            get_choice_text(&choices[idx2], ask_term)
        );

        let wrapped1 = crate::ui::wrap_text(&text1, max_col_width);
        let wrapped2 = crate::ui::wrap_text(&text2, max_col_width);

        let max_lines = std::cmp::max(wrapped1.len(), wrapped2.len());
        for i in 0..max_lines {
            let left = wrapped1.get(i).map(|s| s.as_str()).unwrap_or("");
            let right = wrapped2.get(i).map(|s| s.as_str()).unwrap_or("");

            // print left column and pad to midpoint
            print!("{:<width$}", left, width = midpoint);
            // print right column
            println!("{}", right);
        }
        println!(); // space between pairs
    };

    if choices.len() >= 4 {
        print_row(0, 1);
        print_row(2, 3);
    } else {
        // Fallback for weird cases where we don't have 4 choices
        for (i, c) in choices.iter().enumerate() {
            println!("({}) {}", i + 1, get_choice_text(c, ask_term));
        }
    }
}

pub fn display_feedback(response: &str, expected: &str, is_right: bool) {
    let (width, _) = crossterm::terminal::size().unwrap_or((80, 24));
    let width = width as usize;
    let midpoint = std::cmp::min(width / 2, 50);
    let max_col_width = midpoint.saturating_sub(4);

    use crossterm::style::Stylize;

    println!();
    if is_right {
        let wrapped = crate::ui::wrap_text(expected, width.saturating_sub(5));
        for (i, line) in wrapped.iter().enumerate() {
            if i == 0 {
                println!("{} {}", "✓:".green().bold(), line);
            } else {
                println!("   {}", line);
            }
        }
    } else {
        let wrapped_left = crate::ui::wrap_text(response, max_col_width);
        let wrapped_right = crate::ui::wrap_text(expected, max_col_width);

        let max_lines = std::cmp::max(wrapped_left.len(), wrapped_right.len());
        for i in 0..max_lines {
            let left_line = wrapped_left.get(i).map(|s| s.as_str()).unwrap_or("");
            let right_line = wrapped_right.get(i).map(|s| s.as_str()).unwrap_or("");

            if i == 0 {
                print!(
                    "{} {:<width$}",
                    "X:".red().bold(),
                    left_line,
                    width = midpoint.saturating_sub(3)
                );
                println!("{} {}", "✓:".green().bold(), right_line);
            } else {
                print!(
                    "   {:<width$}",
                    left_line,
                    width = midpoint.saturating_sub(3)
                );
                println!("   {}", right_line);
            }
        }
    }
}

/// Needs to be able to take in whatever context and card then update state like 'still_learning'
fn answer(
    success: &bool,
    c: &Card,
    correct: &mut usize,
    learned: &mut HashSet<String>,
    still_learning: &mut HashSet<String>,
) {
    if *success {
        // increment correct, if card is not in still_learning, push it to learning
        *correct += 1;
        if !still_learning.contains(&c.term) {
            learned.insert(c.term.clone());
        }
    } else {
        // remove from learning if found, add card to still_learning
        learned.remove(&c.term);
        still_learning.insert(c.term.clone());
    }
}

fn initial_fill(
    cards: &mut Vec<Card>,
    threshold: i64,
    card_by_term: &mut HashMap<String, Card>,
    learned: &mut HashSet<String>,
    still_learning: &mut HashSet<String>,
    scores_by_card: &mut HashMap<i64, i64>,
    storage: &mut Storage,
) {
    for c in cards {
        card_by_term.insert(c.term.clone(), c.clone());
        if let Some(id) = c.id {
            // persisted deck: read current score, ignore errors and default to 0
            match storage.get_card_learning_score(id) {
                Ok(s) => {
                    scores_by_card.insert(id, s);
                    // classify for live sets
                    if s >= threshold {
                        learned.insert(c.term.clone());
                    } else if s >= (threshold / 2) {
                        still_learning.insert(c.term.clone()); // halfway
                    } else {
                        // low score initially: still learning
                        still_learning.insert(c.term.clone());
                    }
                }
                Err(_) => {
                    // if DB read fails, treat as unscored
                    still_learning.insert(c.term.clone());
                }
            }
        } else {
            // file-backed deck: no persistence
            still_learning.insert(c.term.clone());
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn learn_mode(
    deck: Deck,
    nostats: bool,
    terms: bool,
    definitions: bool,
    written: bool,
    multiple_choice: bool,
    questions: u8,
    storage: &mut Storage,
) -> anyhow::Result<()> {
    println!("For options like -q=10 to set the number of questions, use `quizzy help learn`");

    // session-level accumulators
    let mut session_correct: usize = 0;
    let mut session_answered: usize = 0;
    let mut session_learned: HashSet<String> = HashSet::new();
    let mut session_still_learning: HashSet<String> = HashSet::new();
    // let mut input = String::new();
    let mut rng = thread_rng();

    // map accumulated session delta for batch update
    let mut session_updates: HashMap<i64, (i64, i64)> = HashMap::new();

    // prepare card list and threshold
    let mut cards: Vec<Card> = deck.cards.to_vec();
    let deck_size = cards.len();
    let threshold = learned_threshold(deck_size); // for now: static for deck size

    // map card id to score (for persistent decks)
    let mut scores_by_card: HashMap<i64, i64> = HashMap::new();
    // map term to card for quick confusion lookups
    let mut card_by_term: HashMap<String, Card> = HashMap::new();

    // set up cards by term and persisted scores
    initial_fill(
        &mut cards,
        threshold,
        &mut card_by_term,
        &mut session_learned,
        &mut session_still_learning,
        &mut scores_by_card,
        storage,
    );

    // use a "bucket" of cards from the deck and refill bucket to get enough questions
    let mut bucket: Vec<usize> = Vec::new();
    fn weight_for_score(threshold: i64, score: i64) -> usize {
        let raw = threshold - score;
        let w = if raw < 1 { 1 } else { raw as usize };
        std::cmp::min(w, 12)
    }

    fn refill_bucket(
        cards: &[Card],
        scores_by_card: &HashMap<i64, i64>,
        bucket: &mut Vec<usize>,
        rng: &mut ThreadRng,
        threshold: i64,
    ) {
        bucket.clear();
        for (i, c) in cards.iter().enumerate() {
            let score =
                c.id.and_then(|id| scores_by_card.get(&id).copied())
                    .unwrap_or(0);
            let w = weight_for_score(threshold, score);
            for _ in 0..w {
                bucket.push(i);
            }
        }
        bucket.shuffle(rng);
    }
    refill_bucket(&cards, &scores_by_card, &mut bucket, &mut rng, threshold);

    if deck.id.is_none() {
        println!(
            "\nUsing a file-backed deck means stats won't be persisted. If you'd like to keep track of your progress and have more adaptive learning, use `quizzy new <name> <file>` and then `quizzy learn <name>`."
        )
    }

    print!(
        "Press [ENTER] to begin lesson on {} or [ESC] at any time to end the session. > ",
        deck.name
    );
    stdout().flush().context("Failed to flush output.")?;
    if enter_input()? == KeyCode::Esc {
        println!("Cancelled Lesson.");
        return Ok(());
    }
    println!();
    'questions: for i in 1..=questions {
        if bucket.is_empty()
            || (deck_size > 10 && bucket.len() < 1 + (deck_size as f64 * 0.25_f64) as usize)
        {
            refill_bucket(&cards, &scores_by_card, &mut bucket, &mut rng, threshold);
        }
        let index = bucket.pop().context("Bucket unexpected empty.")?;
        let c = &cards.get(index).context("Expected card for index.")?;

        // Decide what to ask:
        // - prefer term vs definition according to args/random
        // - prefer written if card is halfway-to-learned and written is allowed
        let ask_term: bool = decide(terms, definitions, &mut rng, 0.5);
        let cur_score =
            c.id.and_then(|id| scores_by_card.get(&id).copied())
                .unwrap_or(0);
        let is_halfway = cur_score >= (threshold / 2);

        // If the card is halfway and written flag is enabled, prefer written
        let ask_written: bool = if is_halfway && written {
            true
        } else {
            // Otherwise use the provided flags and a progressive probability
            decide(
                written,
                multiple_choice,
                &mut rng,
                0.7 * (i as f64 / questions as f64) + 0.3,
            )
        };

        if ask_term {
            println!("\nTerm: {}\t\t\t({i}/{questions})", c.term);
        } else {
            println!("\nDefinition: {}\t\t\t({i}/{questions})", c.definition);
        }
        if ask_written {
            // TODO: rewrite to use raw mode with input buffer
            let response = if let Some(str) = type_input("Type the answer of [ESC] ")? {
                str
            } else {
                println!("\n");
                break 'questions;
            };
            let expected = if ask_term {
                c.definition.clone()
            } else {
                c.term.clone()
            };
            // check if typed answer is close enough
            let is_right = (expected.len() as f64 * 0.3_f64)
                > (string_distance(response.to_lowercase(), expected.to_lowercase()) as f64);

            display_feedback(&response, &expected, is_right);

            session_answered += 1;
            answer(
                &is_right,
                c,
                &mut session_correct,
                &mut session_learned,
                &mut session_still_learning,
            );
        } else {
            // fetch recorded confusions for this card (if persisted)
            let mut confusions_vec: Vec<(i64, i64)> = Vec::new();
            if let Some(card_id) = c.id {
                match storage.get_confusions(card_id) {
                    Ok(v) => confusions_vec = v,
                    Err(_) => { /* ignore DB read error; fallback to pure heuristic */ }
                }
            }
            let choices =
                get_multiple_choice_for_card(c, &cards, &mut rng, ask_term, Some(&confusions_vec));

            display_multiple_choice(&choices, ask_term);

            print!("Enter 1-4 > ");
            stdout()
                .flush()
                .context("Failed to flush output before choice input.")?;
            let n: usize = match choice_input()? {
                KeyCode::Char('1') => 0,
                KeyCode::Char('2') => 1,
                KeyCode::Char('3') => 2,
                KeyCode::Char('4') => 3,
                _ => {
                    println!();
                    break 'questions;
                }
            };
            if choices.get(n).is_none() {
                continue;
            }
            let chosen = choices.get(n).context("Expected valid choice.")?;
            let expected = if ask_term {
                c.definition.clone()
            } else {
                c.term.clone()
            };
            let response = if ask_term {
                chosen.definition.clone()
            } else {
                chosen.term.clone()
            };
            let is_right = expected == response;

            display_feedback(&response, &expected, is_right);

            session_answered += 1;
            answer(
                &is_right,
                c,
                &mut session_correct,
                &mut session_learned,
                &mut session_still_learning,
            );

            if let Some(id) = c.id {
                let entry = session_updates.entry(id).or_insert((0, 0));
                if is_right {
                    entry.0 += 1;
                } else {
                    entry.1 += 1;
                }
                let cur = scores_by_card.get(&id).copied().unwrap_or(0);
                let new_score = cur + (if is_right { 3 } else { -1 });
                scores_by_card.insert(id, new_score);
            }

            // record confusion immediate just to make it easy
            if !is_right && let (Some(correct_id), Some(mistaken_id)) = (c.id, chosen.id) {
                let _ = storage.adjust_confusion(correct_id, mistaken_id, 1);
            } else if is_right && let Some(correct_id) = c.id {
                for mistaken in choices.iter().filter(|x| x != c) {
                    if let Some(mistaken_id) = mistaken.id {
                        // ignore errors since this is not fatal (nothing to cry abou)
                        let _ = storage.adjust_confusion(correct_id, mistaken_id, -1);
                    }
                }
            }
        }

        // a nice pause to keep things at a calm pace
        std::thread::sleep(Duration::from_secs(2));
    }

    // use nostats to decide whether to update the saved stats for this deck
    if !nostats && !session_updates.is_empty() {
        // transform the data, so sad but it had to be done
        let mut updates_vec: Vec<(i64, i64, i64, Option<SM2Stats>)> = Vec::new();
        for (card_id, (corrects, incorrects)) in session_updates.into_iter() {
            // For now we pass None for SM2Stats until we implement the UI for quality rating
            updates_vec.push((card_id, corrects, incorrects, None));
        }

        // try to commit with retries for "wtf" errors
        match commit_session_with_retries(storage, &updates_vec, 3) {
            Ok(()) => println!("\nSession stats saved."),
            Err(e) => {
                eprintln!("Failed to persist session stats after retries: {}", e);
                // try to write fallback file so data is not lost
                match write_failed_session_file(&updates_vec) {
                    Ok(p) => eprintln!("Saved failed session to {:?}", p),
                    Err(e2) => eprintln!("Also failed to write fallback session file: {}", e2),
                }
            }
        }
    }

    print!("Press [ENTER] to view results or [ESC] to skip > ");
    stdout().flush().context("Failed to flush output.")?;
    if enter_input()? == KeyCode::Esc {
        return Ok(());
    }

    println!(
        "\n\nQuestions Answered Correctly: {}/{}",
        session_correct, session_answered
    );
    println!(
        "{} Terms Learned: {}",
        session_learned.len(),
        session_learned.into_iter().collect::<Vec<_>>().join(", ")
    );
    println!(
        "{} Terms Still Learning: {}",
        session_still_learning.len(),
        session_still_learning
            .into_iter()
            .collect::<Vec<_>>()
            .join(", ")
    );
    Ok(())
}
