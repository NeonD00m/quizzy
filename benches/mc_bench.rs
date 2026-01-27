use criterion::{Criterion, criterion_group, criterion_main};
use quizzy::core::deck::read_deck_from_file;
use quizzy::core::learn::get_multiple_choice_for_card;
use rand::{prelude::SliceRandom, thread_rng};
use std::hint::black_box;
use std::path::PathBuf;

// measure the time to generate choices for each card once
fn bench_mc_all_cards(c: &mut Criterion) {
    // adjust the relative path to your econ.txt location in the repo
    let deck = read_deck_from_file(PathBuf::from("econ.txt")).expect("Failed to get deck.");
    let cards = deck.cards;
    let mut rng = thread_rng();

    c.bench_function("mc_all_cards", |b| {
        b.iter(|| {
            // Do one full pass over the deck; Criterion will repeat b.iter many times
            for card in cards.iter() {
                // measure generating options (ask_term = true or false as desired)
                // use black_box to avoid being optimized away
                let _choices =
                    black_box(get_multiple_choice_for_card(card, &cards, &mut rng, true));
            }
        })
    });
}

// as an example, measure time for a single random card
fn bench_mc_single_card(c: &mut Criterion) {
    let deck = read_deck_from_file(PathBuf::from("econ.txt")).expect("Failed to get deck.");
    let mut rng = thread_rng();
    let mut cards = deck.cards;
    cards.shuffle(&mut rng);
    let card = &cards[0];

    c.bench_function("mc_single_card", |b| {
        b.iter(|| {
            let _choices = black_box(get_multiple_choice_for_card(card, &cards, &mut rng, true));
        })
    });
}

criterion_group!(benches, bench_mc_all_cards, bench_mc_single_card);
criterion_main!(benches);
