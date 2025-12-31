use crate::core::deck::*;
use rand::seq::SliceRandom;
use rand::thread_rng;
use std::cmp::max;
use std::io::stdin;

fn display_card(c: &Card, flipped: bool) {
    let len = max(c.term.len(), c.definition.len()) + 4;
    let content = if flipped { &c.definition } else { &c.term };

    // Use padding format to create the top/bottom bars without heap allocation
    println!("╭{:─^len$}╮", "", len = len);
    println!("|{:^len$}|", content, len = len);
    println!("╰{:─^len$}╯", "", len = len);
}

pub fn cards(src: DeckSource, shuffle: bool) {
    println!("To see options like -s for shuffling, use `quizzy help cards`");
    let deck = get_deck(src);
    let mut flipped = false;
    let mut index: usize = 0;
    let mut cards = deck.cards;
    let mut input = String::new();
    let len = cards.len();

    if shuffle {
        let mut rng = thread_rng();
        cards.shuffle(&mut rng);
    }

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
        input.clear();
        while stdin().read_line(&mut input).is_err() {
            println!("Error reading input, try again.");
            input.clear();
        }
        input = input.to_lowercase().replace("\n", "").replace("\r", "");
        match input.as_str() {
            " " => {
                flipped = !flipped;
            }
            "a" => {
                if index > 0 {
                    index -= 1;
                    flipped = false;
                } else {
                    println!("No previous card!");
                }
            }
            "d" => {
                index += 1;
                flipped = false;
            }
            _ => {
                flipped = !flipped;
                if !flipped {
                    index += 1;
                }
            }
        }
        if index >= len {
            println!("Done? [Y/n]");
            input.clear();
            while stdin().read_line(&mut input).is_err() {
                println!("Error reading input, try again.");
                input.clear();
            }
            input = input.to_lowercase().replace("\n", "").replace("\r", "");
            if input.as_str() == "n" {
                println!("Restarting from beginning.");
                index = 0;
            } else {
                break;
            }
        }
    }
}
