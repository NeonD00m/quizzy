use crate::core::deck::*;
use crate::core::storage::Storage;
use crate::ui::input::{StatsInput, stats_input};
use comfy_table::{
    Cell, CellAlignment, ContentArrangement, Table, modifiers::UTF8_ROUND_CORNERS,
    presets::UTF8_FULL,
};
use std::cmp::min;

type StatsPage = (Option<Deck>, Option<i32>, u32);

/// View the statistics of a specific card, maybe even timeframe?
fn card_stats(deck: &Deck, index: i32, storage: &mut Storage) -> anyhow::Result<()> {
    // should I even implement this?
    Ok(())
}

/// View the overview of a deck's stats, with detailed statistics per card
fn deck_by_card(deck: &Deck, size: u32, page: u32, storage: &mut Storage) -> anyhow::Result<bool> {
    // get all cards from this deck or use special storage method?
    // rows: Learned, Learning, Reviewing, Unlearned
    // columns: category, count, truncated card terms
    let mut p = page;
    let total_pages = 1;
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec!["Index", "Term", "Learning Score"]); // add other stats in the future

    'main: loop {
        let mut ptable = table.clone();
        for i in 0..size {
            ptable.add_row(vec![
                i.to_string().as_str(),
                "TERM HERE",
                23.to_string().as_str(),
            ]);
        }
        println!(
            "{}:\t\t\tPage {} of {}\n{}",
            deck.name, p, total_pages, table
        );

        'input: loop {
            match stats_input(
                "Use arrows to move through pages or type an index to view a particular card ",
            )? {
                StatsInput::Up => p = min(p + 1, total_pages),
                StatsInput::Down => p = p.saturating_sub(1),
                StatsInput::Index(n) => {
                    if false {
                        card_stats(deck, n as i32, storage)?;
                    }
                    break 'input;
                }
                StatsInput::Exit => return Ok(true),
                _ => break 'main,
            }
        }
    }
    Ok(false)
}

/// View the overview of a deck's stats, cards organized by learning progress
fn deck_by_category(deck: Deck, size: u32, storage: &mut Storage) -> anyhow::Result<()> {
    // get all cards from this deck or use special storage method?
    // rows: Learned, Learning, Reviewing, Unlearned
    // columns: category, count, truncated card terms
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec!["Progress", "Card Count", "Terms"])
        .add_row(vec!["Learned", 4.to_string().as_str(), "Hi, Bye, No, Yes"])
        .add_row(vec!["Learning", 2.to_string().as_str(), "Move, Go"])
        .add_row(vec![
            "Need To Review",
            4.to_string().as_str(),
            "Hard, Easy, Difficult, Impossible",
        ])
        .add_row(vec!["Unlearned", 1.to_string().as_str(), "Never"]);

    let decks = vec![
        Deck::named(String::from("Learned")),
        Deck::named(String::from("Learning")),
        Deck::named(String::from("Need To Review")),
        Deck::named(String::from("Unlearned")),
    ];

    'main: loop {
        println!("{} by category:\n{}", deck.name, table);

        'input: loop {
            match stats_input(
                "Use down arrow to view the deck card-by-card or type an index for just that section ",
            )? {
                StatsInput::Down => {
                    deck_by_card(&deck, size, 0, storage)?;
                    break 'input;
                }
                StatsInput::Up => {}
                StatsInput::Index(n) => {
                    // construct a Deck with only the cards from this category
                    if let Some(section) = decks.get(n as usize) {
                        deck_by_card(&section, size, 0, storage)?;
                        break 'input;
                    } else {
                        println!("Section doesn't exist!");
                    }
                }
                _ => break 'main,
            }
        }
    }
    Ok(())
}

/// View general numbers of all saved decks
fn overview(size: u32, page: u32, storage: &mut Storage) -> anyhow::Result<()> {
    let mut p = page;
    let total_pages = 1;
    let decks = storage.list_decks()?;

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec!["Deck Id", "Deck Name", "# Cards", "Terms"]);

    'main: loop {
        // loop through decks and add rows as needed for page
        let mut ptable = table.clone();
        for i in 0..size {
            ptable.add_row(vec![
                i.to_string().as_str(),
                "deck name",
                23.to_string().as_str(),
                "Hi, Yes, No, Bye",
            ]);
        }
        println!("All Decks:\t\t\tPage {} of {}\n{}", p, total_pages, table);

        loop {
            match stats_input(
                "Use arrows to move through pages or type an index to view a particular deck ",
            )? {
                StatsInput::Up => p = min(p + 1, total_pages),
                StatsInput::Down => p = p.saturating_sub(1),
                StatsInput::Index(n) => {
                    if let Some((id, _)) = decks.get(n as usize) {
                        let deck = storage.get_deck_by_id(*id)?;
                        deck_by_category(deck, size, storage)?;
                    } else {
                        println!("Invalid index {}: no deck found!", n);
                    }
                }
                _ => break 'main,
            }
        }
    }
    Ok(())
}

#[allow(dead_code)]
pub fn stats_mode(
    deck_option: Option<Deck>,
    size: u32,
    page: u32,
    storage: &mut Storage,
) -> anyhow::Result<()> {
    // if no deck, display overview first for all decks, in order of recently studied
    // allow user to use the indices to inspect further or esc to "go back"
    // save page number at each step of the way for back functionality
    println!(
        "\nUse the left or right arrows to navigate the pages of a table or press Esc to go back to the previous table."
    );

    if let Some(deck) = deck_option {
        if page == 0 {
            deck_by_category(deck, size, storage)?;
        } else {
            deck_by_card(&deck, size, page, storage)?;
        }
    } else {
        overview(size, page, storage)?;
    }
    Ok(())
}
