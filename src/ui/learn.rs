use crate::core::deck::*;
use crate::core::learn::*;
use crate::core::storage::Storage;
use crate::core::string_distance::string_distance;
use anyhow::Context;
use core::f64;
use crossterm::{
    event::{KeyCode, KeyModifiers, read},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use rand::rngs::ThreadRng;
use rand::seq::SliceRandom;
use rand::thread_rng;
use std::collections::{HashMap, HashSet};
use std::io::Write as IoWrite;
use std::io::{stdin, stdout};

/// ALWAYS DISABLE RAW MODE AFTER
fn choice_input() -> anyhow::Result<KeyCode> {
    enable_raw_mode()?;
    while let Ok(event) = read() {
        let Some(event) = event.as_key_press_event() else {
            continue;
        };
        if event.modifiers == KeyModifiers::CONTROL
            && (event.code == KeyCode::Char('c') || event.code == KeyCode::Char('d'))
        {
            return Ok(KeyCode::Esc);
        }
        if event.modifiers != KeyModifiers::NONE {
            println!("Ignoring input due to mofidier {:}\r", event.modifiers);
            continue;
        }
        if matches!(
            event.code,
            KeyCode::Esc
                | KeyCode::Char('1')
                | KeyCode::Char('2')
                | KeyCode::Char('3')
                | KeyCode::Char('4')
        ) {
            return Ok(event.code);
        }
    }
    Ok(KeyCode::Esc)
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
    let mut input = String::new();
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
    println!(
        "\nBeginning lesson: {}. Press Escape at any time to end the session.",
        deck.name
    );

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
            println!("Term: {}\t\t\t({i}/{questions})", c.term);
        } else {
            println!("Definition: {}\t\t\t({i}/{questions})", c.definition);
        }
        if ask_written {
            print!("Type the answer or 'quit': ");
            stdout().flush().context("Failed to flush output.")?;
            loop {
                input.clear();
                if stdin().read_line(&mut input).is_err() {
                    println!("Error reading input, try again.");
                    input.clear();
                    continue;
                }
                let response = input.trim();
                if response == "quit" {
                    break 'questions;
                }
                let expected = if ask_term {
                    c.definition.clone()
                } else {
                    c.term.clone()
                };
                // check if typed answer is close enough
                let is_right = (expected.len() as f64 * 0.3_f64)
                    > (string_distance(response.to_string(), expected.clone()) as f64);
                if is_right {
                    println!("✓: {}\n", expected);
                } else {
                    println!("X: {}\t\t\t✓: {}\n", response, expected);
                }
                session_answered += 1;
                answer(
                    &is_right,
                    c,
                    &mut session_correct,
                    &mut session_learned,
                    &mut session_still_learning,
                );
                break;
            }
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
            if ask_term {
                println!(
                    "(1) {}\t\t\t(2) {}\n(3) {}\t\t\t(4) {}",
                    choices[0].definition,
                    choices[1].definition,
                    choices[2].definition,
                    choices[3].definition,
                );
            } else {
                println!(
                    "(1) {}\t\t\t(2) {}\n(3) {}\t\t\t(4) {}",
                    choices[0].term, choices[1].term, choices[2].term, choices[3].term,
                );
            }
            print!("Type 1-4 or press Esc to exit: ");
            stdout()
                .flush()
                .context("Failed to flush output before choice input.")?;
            let n = match choice_input()? {
                KeyCode::Char('1') => 0,
                KeyCode::Char('2') => 1,
                KeyCode::Char('3') => 2,
                KeyCode::Char('4') => 3,
                _ => {
                    disable_raw_mode()?;
                    break 'questions;
                }
            };
            disable_raw_mode()?;
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
            if is_right {
                println!("\n✓: {}\n", response);
            } else {
                println!("\nX: {}\t\t\t✓: {}\n", response, expected);
            }
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
    }

    // use nostats to decide whether to update the saved stats for this deck
    if !nostats && !session_updates.is_empty() {
        // transform the data, so sad but it had to be done
        let mut updates_vec: Vec<(i64, i64, i64)> = Vec::new();
        for (card_id, (corrects, incorrects)) in session_updates.into_iter() {
            updates_vec.push((card_id, corrects, incorrects));
        }

        // try to commit with retries for "wtf" errors
        match commit_session_with_retries(storage, &updates_vec, 3) {
            Ok(()) => println!("Session stats saved."),
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

    println!("Continue to view results:");
    input.clear();
    if let Err(e) = stdin().read_line(&mut input) {
        println!("Error reading line but continuing anyways: {}", e);
    }

    println!(
        "Questions Answered Correctly: {}/{}",
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
