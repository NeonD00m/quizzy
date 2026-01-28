// run with `cargo bench -v` PLEASE REMEMBER
use criterion::{Criterion, criterion_group, criterion_main};
use quizzy::core::deck::Card;
use quizzy::core::deck::read_deck_from_file;
use quizzy::core::learn::get_multiple_choice_for_card;
use rand::Rng;
use rand::rngs::ThreadRng;
use rand::{prelude::SliceRandom, thread_rng};
use std::cmp::min;
use std::collections::HashMap;
use std::hint::black_box;
use std::path::PathBuf;

fn generate_confusions(cards: &mut Vec<Card>, rng: &mut ThreadRng) -> Vec<(i64, i64)> {
    // assign deterministic fake ids (1..N) so mistaken_card_id values can match
    for (i, card) in cards.iter_mut().enumerate() {
        card.id = Some(i as i64 + 1);
    }

    let mut conf_map: HashMap<i64, i64> = HashMap::new();
    let deck_len = cards.len();
    let num_samples = 4; //min(100, deck_len / 10); // tune volume of confusions
    for _ in 0..num_samples {
        let idx = rng.gen_range(0..deck_len);
        let mistaken_id = cards[idx].id.unwrap();
        let count: i64 = rng.gen_range(1..=8); // random count 1..8
        *conf_map.entry(mistaken_id).or_insert(0) += count;
    }
    // converts to Vec<(i64, i64)>
    return conf_map.into_iter().collect();
}

// measure the time to generate choices for each card once
fn bench_mc_all_cards_no_confusions(c: &mut Criterion) {
    // adjust the relative path to your econ.txt location in the repo
    let deck = read_deck_from_file(PathBuf::from("econ.txt")).expect("Failed to get deck.");
    let cards = deck.cards;
    let mut rng = thread_rng();

    c.bench_function("mc_all_cards_no_confusions", |b| {
        b.iter(|| {
            // Do one full pass over the deck; Criterion will repeat b.iter many times
            for card in cards.iter() {
                // measure generating options (ask_term = true or false as desired)
                // use black_box to avoid being optimized away
                let _choices = black_box(get_multiple_choice_for_card(
                    card, &cards, &mut rng, true, None,
                ));
            }
        })
    });
}

// measure the time to generate choices for each card once
fn bench_mc_all_cards(c: &mut Criterion) {
    let deck = read_deck_from_file(PathBuf::from("econ.txt")).expect("Failed to get deck.");
    let mut cards = deck.cards.clone();
    let mut rng = thread_rng();
    let confusions = generate_confusions(&mut cards, &mut rng);

    c.bench_function("mc_all_cards_with_confusions", |b| {
        b.iter(|| {
            // Do one full pass over the deck; Criterion will repeat b.iter many times
            for card in cards.iter() {
                // measure generating options (ask_term = true or false as desired)
                // use black_box to avoid being optimized away
                let _choices = black_box(get_multiple_choice_for_card(
                    card,
                    &cards,
                    &mut rng,
                    true,
                    Some(&confusions),
                ));
            }
        })
    });
}

// as an example, measure time for a single random card
fn bench_mc_single_card_no_confusions(c: &mut Criterion) {
    let deck = read_deck_from_file(PathBuf::from("econ.txt")).expect("Failed to get deck.");
    let mut rng = thread_rng();
    let mut cards = deck.cards;
    cards.shuffle(&mut rng);
    let card = &cards[0];

    c.bench_function("mc_single_card_no_confusions", |b| {
        b.iter(|| {
            let _choices = black_box(get_multiple_choice_for_card(
                card, &cards, &mut rng, true, None,
            ));
        })
    });
}

// as an example, measure time for a single random card
fn bench_mc_single_card(c: &mut Criterion) {
    let deck = read_deck_from_file(PathBuf::from("econ.txt")).expect("Failed to get deck.");
    let mut rng = thread_rng();
    let mut cards = deck.cards.clone();
    let mut cards2 = cards.clone();
    cards.shuffle(&mut rng);
    let card = &cards[0];
    let confusions = generate_confusions(&mut cards2, &mut rng);

    c.bench_function("mc_single_card_with_confusions", |b| {
        b.iter(|| {
            let _choices = black_box(get_multiple_choice_for_card(
                card,
                &cards,
                &mut rng,
                true,
                Some(&confusions),
            ));
        })
    });
}

criterion_group!(
    benches_single,
    bench_mc_single_card,
    bench_mc_single_card_no_confusions
);
criterion_group!(
    benches_all,
    bench_mc_all_cards,
    bench_mc_all_cards_no_confusions
);
criterion_main!(benches_single, benches_all);
