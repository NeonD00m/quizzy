use crate::core::deck::*;
use crate::core::string_distance::string_distance;
use core::f64;
use crossterm::{
    event::{KeyCode, KeyModifiers, read},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use rand::rngs::ThreadRng;
use rand::seq::SliceRandom;
use rand::{Rng, thread_rng};
use std::collections::HashSet;
use std::io::{Write, stdin, stdout};

fn decide(condition1: bool, condition2: bool, rng: &mut ThreadRng, probability: f64) -> bool {
    if condition1 {
        true
    } else if condition2 {
        false
    } else {
        rng.gen_bool(probability)
    }
}

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
        if match event.code {
            KeyCode::Esc => true,
            KeyCode::Char('1') => true,
            KeyCode::Char('2') => true,
            KeyCode::Char('3') => true,
            KeyCode::Char('4') => true,
            _ => false,
        } {
            return Ok(event.code);
        }
    }
    return Ok(KeyCode::Esc);
}

/// Returns a vector including the original card and 3 others, randomly sorted
pub fn get_multiple_choice_for_card(
    c: &Card,
    cards: &Vec<Card>,
    rng: &mut ThreadRng,
    ask_term: bool,
) -> Vec<Card> {
    let expected = if ask_term {
        c.definition.clone()
    } else {
        c.term.clone()
    };

    // build a list of candidate cards (exclude the card itself)
    let mut candidates: Vec<(u8, Card)> = cards
        .iter()
        .filter(|other| other.term != c.term && other.definition != c.definition)
        .map(|other| {
            let candidate_str = if ask_term {
                other.definition.clone()
            } else {
                other.term.clone()
            };
            let dist = string_distance(candidate_str, expected.clone());
            (dist, other.clone())
        })
        .collect();

    // sort ascending by distance (most similar first)
    candidates.sort_by_key(|(dist, _)| *dist);

    // TODO: do non-deterministicly weighted by similarity
    let mut choices: Vec<Card> = candidates
        .into_iter()
        .take(3)
        .map(|(_, card)| card)
        .collect();

    // if fewer than 3 similar choices found, fill randomly
    if choices.len() < 3 {
        let mut additional: Vec<Card> = cards
            .iter()
            .filter(|other| other.term != c.term || other.definition != c.definition)
            .filter(|other| {
                !choices
                    .iter()
                    .any(|ch| ch.term == other.term && ch.definition == other.definition)
            })
            .cloned()
            .collect();
        additional.shuffle(rng);
        for card in additional.into_iter().take(3 - choices.len()) {
            choices.push(card);
        }
    }

    // add the correct card and shuffle
    choices.push(c.clone());
    choices.shuffle(rng);

    choices
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

pub fn learn_mode(
    deck: Deck,
    _nostats: bool,
    terms: bool,
    definitions: bool,
    written: bool,
    multiple_choice: bool,
    questions: u8,
) -> anyhow::Result<()> {
    println!("For options like -q=10 to set the number of questions, use `quizzy help learn`");
    // use a "bucket" of cards from the deck and refill bucket to get enough questions
    let mut correct: usize = 0;
    let mut answered: usize = 0;
    let mut learned: HashSet<String> = HashSet::new();
    let mut still_learning: HashSet<String> = HashSet::new();
    let mut input = String::new();
    let mut rng = thread_rng();
    let mut bucket: Vec<Card> = Vec::new();
    let mut random_cards = deck.cards.to_vec();
    random_cards.shuffle(&mut rng);

    println!(
        "Beginning lesson: {}. Press Escape at any time to end the session.",
        deck.name
    );
    'questions: for i in 1..=questions {
        // TODO: eventually we want to prioritize asking questions for "still learning" cards
        let count = bucket.iter().count();
        if count < 1 {
            bucket = deck.cards.to_vec();
            bucket.shuffle(&mut rng);
        }
        let c = bucket
            .pop()
            .expect(format!("None value in bucket when count is {count}").as_str());

        let ask_term: bool = decide(terms, definitions, &mut rng, 0.5);
        let ask_written: bool = decide(
            written,
            multiple_choice,
            &mut rng,
            0.7 * (i as f64 / questions as f64) + 0.3,
        );
        if ask_term {
            println!("Term: {}\t\t\t({i}/{questions})", c.term);
        } else {
            println!("Definition: {}\t\t\t({i}/{questions})", c.definition);
        }
        if ask_written {
            print!("Type the answer or 'quit': ");
            stdout().flush().unwrap();
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
                // TODO: check if typed answer is close enough
                let expected = if ask_term {
                    c.definition.clone()
                } else {
                    c.term.clone()
                };
                let is_right = (expected.len() as f64 * 0.3f64)
                    > (string_distance(response.to_string(), expected.clone()) as f64);
                if is_right {
                    println!("✓: {}\n", expected);
                } else {
                    println!("X: {}\t\t\t✓: {}\n", response, expected);
                }
                answered += 1;
                answer(
                    &is_right,
                    &c,
                    &mut correct,
                    &mut learned,
                    &mut still_learning,
                );
                break;
            }
        } else {
            // ask multiple choice
            let choices = get_multiple_choice_for_card(&c, &random_cards, &mut rng, ask_term);
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
            let n = match choice_input()? {
                KeyCode::Char('1') => 0,
                KeyCode::Char('2') => 1,
                KeyCode::Char('3') => 2,
                KeyCode::Char('4') => 3,
                _ => {
                    println!("should be exiting");
                    disable_raw_mode()?;
                    break 'questions;
                }
            };
            disable_raw_mode()?;
            if choices.get(n).is_none() {
                continue;
            }
            let chosen = choices.get(n).expect("Expected valid choice.");
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
                println!("✓: {}\n", response);
            } else {
                println!("X: {}\t\t\t✓: {}\n", response, expected);
            }
            answered += 1;
            answer(
                &is_right,
                &c,
                &mut correct,
                &mut learned,
                &mut still_learning,
            );
        }
    }

    println!("Continue to view results:");
    input.clear();
    if let Err(e) = stdin().read_line(&mut input) {
        println!("Error reading line but continuing anyways: {}", e);
    }

    println!("Questions Answered Correctly: {}/{}", correct, answered);
    println!(
        "{} Terms Learned: {}",
        learned.len(),
        learned.into_iter().collect::<Vec<_>>().join(", ")
    );
    println!(
        "{} Terms Still Learning: {}",
        still_learning.len(),
        still_learning.into_iter().collect::<Vec<_>>().join(", ")
    );
    // use nostats to decide whether to update the saved stats for this deck
    Ok(())
}
