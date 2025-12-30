use crate::core::deck::*;
use core::f64;
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

/// Returns a vector including the original card and 3 others, randomly sorted
fn get_multiple_choice_for_card(c: &Card, _cards: &Vec<Card>, _rng: &mut ThreadRng) -> Vec<Card> {
    vec![
        Card::new("t1X", "d1X"),
        Card::new("t2X", "d2X"),
        c.clone(),
        Card::new("t3X", "d3X"),
    ]
    // let mut choices: Vec<Card> = Vec::new();
    // // pick up to 3 other random distinct cards
    // let mut others: Vec<Card> = cards.iter().filter(|x| x.term != c.term).cloned().collect();
    // others.shuffle(rng);
    // choices.extend(others.into_iter().take(3));
    // choices.push(c.clone());
    // choices.shuffle(rng);
    // choices
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

pub fn learn(
    src: DeckSource,
    nostats: bool,
    terms: bool,
    definitions: bool,
    written: bool,
    multiple_choice: bool,
    questions: u8,
) {
    println!("For options like -q=10 to set the number of questions, use `quizzy help learn`");
    // use a "bucket" of cards from the deck and refill bucket to get enough questions
    let mut correct: usize = 0;
    let mut answered: usize = 0;
    let mut learned: HashSet<String> = HashSet::new();
    let mut still_learning: HashSet<String> = HashSet::new();
    let mut input = String::new();

    let deck = example_deck();
    let mut rng = thread_rng();
    let mut bucket: Vec<Card> = Vec::new();
    let mut random_cards = deck.cards.to_vec();
    random_cards.shuffle(&mut rng);

    for i in 1..=questions {
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
            println!("Definitions: {}\t\t\t({i}/{questions})", c.definition);
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
                    break;
                }
                // TODO: check if typed answer is close enough
                let is_right = response
                    == (if ask_term {
                        c.definition.as_str()
                    } else {
                        c.term.as_str()
                    });
                if is_right {
                    println!(
                        "✓: {}\n",
                        if ask_term {
                            c.definition.as_str()
                        } else {
                            c.term.as_str()
                        }
                    );
                } else {
                    println!(
                        "X: {}\t\t\t✓: {}\n",
                        response,
                        if ask_term {
                            c.definition.as_str()
                        } else {
                            c.term.as_str()
                        }
                    );
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
            let choices = get_multiple_choice_for_card(&c, &random_cards, &mut rng);
            if ask_term {
                println!(
                    "(1) {}\t\t\t(2) {}\n(3) {}\t\t\t(4) {}",
                    choices[0].definition, //.get(0).expect("Choice 1 error.").definition,
                    choices[1].definition, //.get(1).expect("Choice 2 error").definition,
                    choices[2].definition, //.get(2).expect("Choice 3 error.").definition,
                    choices[3].definition, //.get(3).expect("Choice 4 error").definition
                );
            } else {
                println!(
                    "(1) {}\t\t\t(2) {}\n(3) {}\t\t\t(4) {}",
                    choices[0].term, //.get(0).expect("Choice 1 error.").term,
                    choices[1].term, //.get(1).expect("Choice 2 error").term,
                    choices[2].term, //.get(2).expect("Choice 3 error.").term,
                    choices[3].term, //.get(3).expect("Choice 4 error").term
                );
            }
            print!("Type 1-4 or 'quit': ");
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
                    break;
                }
                let n: usize = match response {
                    "1" => 0,
                    "2" => 1,
                    "3" => 2,
                    "4" => 3,
                    _ => {
                        println!("Unrecognized response, please try again.");
                        continue;
                    }
                };
                if choices.get(n).is_none() {
                    continue;
                }
                let chosen = choices.get(n).expect("Expected valid choice.");
                let is_right = if ask_term {
                    c.definition == chosen.definition
                } else {
                    c.term == chosen.term
                };
                if is_right {
                    println!("✓: {}\n", response);
                } else {
                    println!(
                        "X: {}\t\t\t✓: {}\n",
                        response,
                        if ask_term {
                            c.definition.as_str()
                        } else {
                            c.term.as_str()
                        }
                    );
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

                // match response.parse::<u8>() {
                //     Ok(n) => {
                //         if n == 0 || n > 4 {
                //             println!("Must choose a number between 1 and 4.");
                //             continue;
                //         }
                //         let chosen = choices
                //             .get((n - 1) as usize)
                //             .expect("No card exists at given choice answer.");
                //         // TODO: Implement a better algorithm to detect whether an answer is right enough
                //         let is_right = if ask_term {
                //             c.definition == chosen.definition
                //         } else {
                //             c.term == chosen.term
                //         };
                //         if is_right {
                //             println!("✓: {}\n", response);
                //         } else {
                //             println!(
                //                 "X: {}\t\t\t✓: {}\n",
                //                 response,
                //                 if ask_term {
                //                     c.definition.as_str()
                //                 } else {
                //                     c.term.as_str()
                //                 }
                //             );
                //         }
                //         answered += 1;
                //         answer(
                //             &is_right,
                //             &c,
                //             &mut correct,
                //             &mut learned,
                //             &mut still_learning,
                //         );
                //         break;
                //     }
                //     Err(e) => {
                //         println!("Error parsing number: {}", e);
                //         continue;
                //     }
                // }
            }
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
}
