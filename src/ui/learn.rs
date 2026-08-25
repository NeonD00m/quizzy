use crate::core::deck::*;
use crate::core::fsrs::{CardState, FsrsCard, FsrsEngine, Rating};
use crate::core::learn::*;
use crate::core::storage::{FSRSStats, Storage};
use crate::core::string_distance::string_distance;
use crate::ui::{
    input::{choice_input, enter_input, type_input},
    print_split_aligned,
};
use anyhow::Context;
use chrono::{TimeZone, Utc};
use comfy_table::{Table, presets::UTF8_FULL};
use core::f64;
use crossterm::event::KeyCode;
use crossterm::style::Stylize;
use rand::rngs::ThreadRng;
use rand::seq::SliceRandom;
use rand::thread_rng;
use std::collections::{HashMap, HashSet};
use std::io::{Write as IoWrite, stdout};
use std::time::{Duration, Instant, SystemTime};

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

fn format_next_due(
    due_cards: i64,
    total_cards: i64,
    next_due_at: Option<i64>,
    now_secs: i64,
) -> String {
    if total_cards == 0 {
        return "-".to_string();
    }
    if due_cards > 0 {
        return "Now".to_string();
    }
    match next_due_at {
        Some(ts) => {
            let diff = ts - now_secs;
            if diff <= 0 {
                "Now".to_string()
            } else if diff < 3600 {
                format!("In {}m", (diff / 60).max(1))
            } else if diff < 86400 {
                format!("In {}h", diff / 3600)
            } else if diff < 86400 * 7 {
                format!("In {}d", diff / 86400)
            } else {
                Utc.timestamp_opt(ts, 0)
                    .single()
                    .map(|dt| dt.format("%Y-%m-%d").to_string())
                    .unwrap_or_else(|| "Later".to_string())
            }
        }
        None => "Caught up".to_string(),
    }
}

pub fn learn_dashboard(storage: &mut Storage) -> anyhow::Result<()> {
    let items = storage.get_deck_dashboard_items()?;
    if items.is_empty() {
        println!("\nNo decks found in database. Create one with `quizzy new <name> <file>`.");
        return Ok(());
    }

    let now_secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec![
        "#",
        "Deck Name",
        "Due Cards",
        "New Cards",
        "Total Cards",
        "Next Due",
    ]);

    for (i, item) in items.iter().enumerate() {
        let next_due_str =
            format_next_due(item.due_cards, item.total_cards, item.next_due_at, now_secs);
        table.add_row(vec![
            format!("{}", i + 1),
            item.name.clone(),
            format!("{}", item.due_cards),
            format!("{}", item.new_cards),
            format!("{}", item.total_cards),
            next_due_str,
        ]);
    }

    println!("\n==================== Learn Dashboard ====================");
    println!("{table}");
    println!("=========================================================\n");

    let selected_deck;
    loop {
        let input = match type_input(
            format!(
                "Select a deck number (1-{}) or type deck name to study ([ESC] to exit) ",
                items.len()
            )
            .as_str(),
        )? {
            Some(s) if !s.trim().is_empty() => s.trim().to_string(),
            None => {
                println!("\n");
                return Ok(());
            }
            _ => {
                print!("\n\nNo input detected.\n");
                stdout().flush().context("Failed to flush output.")?;
                continue;
            }
        };

        selected_deck = if let Ok(idx) = input.parse::<usize>() {
            if idx >= 1 && idx <= items.len() {
                items[idx - 1].name.clone()
            } else {
                print!("\n\nInvalid deck selection.\n");
                stdout().flush().context("Failed to flush output.")?;
                continue;
            }
        } else if let Some(item) = items.iter().find(|it| it.name == input) {
            item.name.clone()
        } else {
            print!("\n\nInvalid deck selection.\n");
            stdout().flush().context("Failed to flush output.")?;
            continue;
        };
        break;
    }

    let deck = storage.get_deck_by_name(&selected_deck)?;
    storage.update_user_last_active()?;
    if let Some(id) = deck.id {
        storage.update_deck_last_studied(id)?;
    }
    learn_mode(deck, false, false, storage)
}

#[allow(clippy::too_many_arguments)]
pub fn learn_mode(
    deck: Deck,
    terms: bool,
    definitions: bool,
    storage: &mut Storage,
) -> anyhow::Result<()> {
    let deck_id = deck.id;
    let card_fsrs_list: Vec<(Card, FSRSStats)> = if let Some(id) = deck_id {
        storage.get_cards_with_fsrs_for_deck(id)?
    } else {
        deck.cards
            .iter()
            .cloned()
            .map(|c| (c, FSRSStats::default()))
            .collect()
    };

    if card_fsrs_list.is_empty() {
        println!("\nDeck '{}' has no cards to study.", deck.name);
        return Ok(());
    }

    let engine = FsrsEngine::default();
    let mut rng = thread_rng();

    let mut now_secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let mut queue: Vec<(Card, FSRSStats)> = card_fsrs_list;
    let due_count = queue
        .iter()
        .filter(|(_, fsrs)| fsrs.last_review == 0 || fsrs.next_due <= now_secs)
        .count();

    println!(
        "\nStarting FSRS Learn Session for '{}' ({} due/new, {} total cards).",
        deck.name,
        due_count,
        queue.len()
    );
    println!("Press [ENTER] to begin or [ESC] at any prompt to finish.");
    stdout().flush().context("Failed to flush output.")?;
    if enter_input()? == KeyCode::Esc {
        println!("Cancelled session.");
        return Ok(());
    }

    let mut updates_map: HashMap<i64, (FSRSStats, i64, i64, i64)> = HashMap::new();
    let mut session_corrects: usize = 0;
    let mut session_incorrects: usize = 0;
    let mut session_reviews: usize = 0;
    let mut due_reviewed_count: usize = 0;

    let mut index = 0;
    while index < queue.len() {
        let current_index = index;
        let (card, mut fsrs) = queue[index].clone();
        index += 1;

        let card_id = card.id.unwrap_or(0);
        let ask_term = decide(terms, definitions, &mut rng, 0.5);

        let q_info = format!("({}/{})", index, queue.len());
        println!();
        if ask_term {
            print_split_aligned(&format!("Term: {}", card.term), &q_info, Some(60));
        } else {
            print_split_aligned(
                &format!("Definition: {}", card.definition),
                &q_info,
                Some(60),
            );
        }

        let before = Instant::now();
        let response = match type_input("Type the answer or [ESC] ")? {
            Some(s) => s,
            None => {
                println!("\nEnding study session early.");
                break;
            }
        };
        let elapsed = Instant::now().duration_since(before).as_millis() as f64;

        let expected = if ask_term {
            card.definition.as_str()
        } else {
            card.term.as_str()
        };

        let distance = string_distance(
            response.trim().to_lowercase().as_str(),
            expected.trim().to_lowercase().as_str(),
        ) as f64;

        let len = expected.trim().len().max(1) as f64;
        let distance_ratio = distance / len;

        let grade: Rating;
        println!();
        if distance_ratio <= 0.15 {
            // TODO: check if they typed answer fast enough and mark as Easy if it is
            let expected_secs = expected.trim().len() as f64 / 3.3; // baseline 40 WPM speed
            let target_time = expected_secs + 1.5; // reading buffer
            grade = if elapsed / 1000.0 <= target_time {
                Rating::Easy
            } else if elapsed / 1000.0 <= target_time * 2.0 {
                Rating::Good
            } else {
                Rating::Hard
            };
            display_feedback(&response, expected, true);
        } else if distance_ratio <= 0.40 {
            println!("\n{}", "Close answer! Please self-grade:".yellow().bold());
            println!("   Your answer: {}", response);
            println!("   Expected:    {}", expected);
            println!(
                "Select grade: (1) Again [Forgot], (2) Hard [Effort], (3) Good [Correct], (4) Easy [Instant]"
            );

            let choice = choice_input()?;
            grade = match choice {
                KeyCode::Char('1') => Rating::Again,
                KeyCode::Char('2') => Rating::Hard,
                KeyCode::Char('3') => Rating::Good,
                KeyCode::Char('4') => Rating::Easy,
                _ => Rating::Hard,
            };
        } else {
            grade = Rating::Again;
            display_feedback(&response, expected, false);
        }

        let card_state = CardState::from_u8(fsrs.state);
        let fsrs_card = FsrsCard {
            stability: fsrs.stability,
            difficulty: fsrs.difficulty,
            last_review: fsrs.last_review,
            reps: fsrs.repetition_count as u32,
            lapses: fsrs.lapses as u32,
            state: card_state,
        };

        now_secs = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let sched = engine.review_card(fsrs_card, grade, now_secs);

        fsrs.stability = sched.card.stability;
        fsrs.difficulty = sched.card.difficulty;
        fsrs.repetition_count = sched.card.reps as i64;
        fsrs.lapses = sched.card.lapses as i64;
        fsrs.state = sched.card.state as u8;
        fsrs.last_review = sched.card.last_review;
        fsrs.next_due = sched.next_due;

        let is_correct = grade == Rating::Good || grade == Rating::Easy;
        if is_correct {
            session_corrects += 1;
        } else {
            session_incorrects += 1;
            queue.push((card.clone(), fsrs));
        }

        session_reviews += 1;
        if current_index < due_count {
            due_reviewed_count += 1;
        }

        if card_id > 0 {
            let entry = updates_map.entry(card_id).or_insert((fsrs, 0, 0, 0));
            entry.0 = fsrs;
            if is_correct {
                entry.1 += 1;
            } else {
                entry.2 += 1;
            }
            entry.3 += grade.score_delta();
        }

        std::thread::sleep(Duration::from_millis(600));
    }

    if deck_id.is_some() && !updates_map.is_empty() {
        let updates: Vec<(i64, FSRSStats, i64, i64, i64)> = updates_map
            .into_iter()
            .map(|(cid, (fstat, c, inc, sdelta))| (cid, fstat, c, inc, sdelta))
            .collect();

        let payload = SessionPayload::Learn { updates };
        match commit_payload_with_retries(storage, &payload, 5) {
            Ok(()) => println!("\n[FSRS] Successfully saved session metrics."),
            Err(e) => {
                eprintln!("\n[FSRS] Failed to commit session to database: {e}");
                if let Ok(path) = write_failed_session_file(&payload) {
                    eprintln!("[FSRS] Saved recovery file to {}", path.display());
                }
            }
        }
    }

    let unreviewed_due = due_count.saturating_sub(due_reviewed_count);
    if unreviewed_due > 0 {
        println!(
            "\nSession Complete! Reviewed {} cards ({} correct, {} again, {} unreviewed).",
            session_reviews, session_corrects, session_incorrects, unreviewed_due
        );
    } else {
        println!(
            "\nSession Complete! Reviewed {} cards ({} correct, {} again).",
            session_reviews, session_corrects, session_incorrects
        );
    }
    Ok(())
}

#[allow(dead_code, clippy::too_many_arguments)]
pub fn test_mode(
    deck: Deck,
    feedback: bool,
    terms: bool,
    definitions: bool,
    written: bool,
    multiple_choice: bool,
    questions: u8,
    storage: &mut Storage,
) -> anyhow::Result<()> {
    println!("For options like -q=10 to set the number of questions, use `quizzy help test`");

    // session-level accumulators
    let mut session_correct: usize = 0;
    let mut session_answered: usize = 0;
    let mut session_learned: HashSet<String> = HashSet::new();
    let mut session_still_learning: HashSet<String> = HashSet::new();
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
        "Press [ENTER] to begin test on {} or [ESC] at any time to end the session. > ",
        deck.name
    );
    stdout().flush().context("Failed to flush output.")?;
    if enter_input()? == KeyCode::Esc {
        println!("\nCancelled Test.");
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

        println!();
        let q_info = format!("({i}/{questions})");
        if ask_term {
            print_split_aligned(&format!("Term: {}", c.term), &q_info, Some(60));
        } else {
            print_split_aligned(&format!("Definition: {}", c.definition), &q_info, Some(60));
        }

        if ask_written {
            let response = if let Some(str) = type_input("Type the answer of [ESC] ")? {
                str
            } else {
                println!();
                break 'questions;
            };
            println!();
            let expected = if ask_term {
                c.definition.as_str()
            } else {
                c.term.as_str()
            };
            // check if typed answer is close enough
            let is_right = (expected.len() as f64 * 0.3_f64)
                > (string_distance(
                    response.to_lowercase().as_str(),
                    expected.to_lowercase().as_str(),
                ) as f64);

            if feedback {
                display_feedback(&response, expected, is_right);
            }

            if let Some(id) = c.id {
                let (c_delta, i_delta) = if is_right { (1, 0) } else { (0, 1) };
                let immediate = [(id, c_delta, i_delta)];
                if storage.commit_test_session(&immediate).is_err() {
                    let entry = session_updates.entry(id).or_insert((0, 0));
                    entry.0 += c_delta;
                    entry.1 += i_delta;
                }
                let cur = scores_by_card.get(&id).copied().unwrap_or(0);
                let new_score = cur + (if is_right { 2 } else { -1 });
                scores_by_card.insert(id, new_score);
            }
        } else {
            // fetch recorded confusions for this card (if persisted)
            let mut confusions_vec: Vec<(i64, i64)> = Vec::new();
            if let Some(card_id) = c.id {
                match storage.get_bidirectional_confusions(card_id) {
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

            if feedback {
                display_feedback(&response, &expected, is_right);
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
                let (c_delta, i_delta) = if is_right { (1, 0) } else { (0, 1) };
                let immediate = [(id, c_delta, i_delta)];
                if storage.commit_test_session(&immediate).is_err() {
                    let entry = session_updates.entry(id).or_insert((0, 0));
                    entry.0 += c_delta;
                    entry.1 += i_delta;
                }
                let cur = scores_by_card.get(&id).copied().unwrap_or(0);
                let new_score = cur + (if is_right { 2 } else { -1 });
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

    // save any uncommitted session updates (e.g. from failed immediate commits)
    if !session_updates.is_empty() {
        let mut updates_vec: Vec<(i64, i64, i64)> = Vec::new();
        for (card_id, (corrects, incorrects)) in session_updates.into_iter() {
            updates_vec.push((card_id, corrects, incorrects));
        }

        let payload = SessionPayload::Test {
            updates: updates_vec,
        };
        match commit_payload_with_retries(storage, &payload, 3) {
            Ok(()) => println!("\nSession stats saved."),
            Err(e) => {
                eprintln!("Failed to persist session stats after retries: {}", e);
                match write_failed_session_file(&payload) {
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
