use crate::core::deck::*;
use crate::core::storage::Storage;
use crate::ui::input::{StatsInput, stats_input};
use chrono::{TimeZone, Utc};
use comfy_table::{ContentArrangement, Table, modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL};
use std::cmp::min;

#[derive(Clone)]
enum StatsViewState {
    Overview {
        page: u32,
    },
    DeckCategory {
        deck_id: i64,
        deck_name: String,
    },
    DeckByCard {
        deck_id: i64,
        deck_name: String,
        page: u32,
    },
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() > max_len {
        let truncated: String = s.chars().take(max_len - 3).collect();
        format!("{}...", truncated)
    } else {
        s.to_string()
    }
}

pub fn stats_mode(
    deck_option: Option<Deck>,
    size: u32,
    page: u32,
    storage: &mut Storage,
) -> anyhow::Result<()> {
    let mut stack = Vec::new();

    // Initialize the stack based on whether a deck was provided
    if let Some(deck) = deck_option {
        if page > 0 {
            stack.push(StatsViewState::DeckByCard {
                deck_id: deck.id.unwrap_or(0),
                deck_name: deck.name,
                page: page - 1,
            });
        } else {
            stack.push(StatsViewState::DeckCategory {
                deck_id: deck.id.unwrap_or(0),
                deck_name: deck.name.clone(),
            });
        }
    } else {
        stack.push(StatsViewState::Overview { page: page });
    }

    println!("\nNavigation: Down/Up (Pages) | Enter/Index (Select) | Esc (Back) | q (Exit)");

    while let Some(current_state) = stack.last().cloned() {
        match current_state {
            StatsViewState::Overview { page } => {
                let decks = storage.list_decks()?;
                let total_decks = decks.len();
                let total_pages = (total_decks as f32 / size as f32).ceil() as u32;
                let page = min(page, total_pages - 1);
                let start = (page * size) as usize;
                let end = min(start + size as usize, total_decks);

                let mut table = Table::new();
                table
                    .load_preset(UTF8_FULL)
                    .apply_modifier(UTF8_ROUND_CORNERS)
                    .set_content_arrangement(ContentArrangement::Dynamic)
                    .set_header(vec!["#", "Deck Name", "Cards", "Last Studied"]);

                for (i, (id, name)) in decks[start..end].iter().enumerate() {
                    let summary = storage.get_deck_stats_summary(*id)?;
                    // We need last_studied_at from deck_stats
                    let last_studied: Option<i64> = storage
                        .conn
                        .query_row(
                            "SELECT last_studied_at FROM deck_stats WHERE deck_id = ?1",
                            [id],
                            |r| r.get(0),
                        )
                        .ok();

                    let date_str = last_studied
                        .map(|ts| {
                            Utc.timestamp_opt(ts, 0)
                                .unwrap()
                                .format("%Y-%m-%d")
                                .to_string()
                        })
                        .unwrap_or_else(|| "Never".to_string());

                    table.add_row(vec![
                        (start + i).to_string(),
                        name.clone(),
                        summary.total_cards.to_string(),
                        date_str,
                    ]);
                }

                println!(
                    "\n--- All Decks (Page {}/{}) ---\n{}",
                    page + 1,
                    total_pages.max(1),
                    table
                );

                match stats_input("Select a deck index or use arrows to paginate: ")? {
                    StatsInput::Down if page + 1 < total_pages => {
                        stack.pop();
                        stack.push(StatsViewState::Overview { page: page + 1 });
                    }
                    StatsInput::Up if page > 0 => {
                        stack.pop();
                        stack.push(StatsViewState::Overview { page: page - 1 });
                    }
                    StatsInput::Index(n) => {
                        if let Some((id, name)) = decks.get(n as usize) {
                            stack.push(StatsViewState::DeckCategory {
                                deck_id: *id,
                                deck_name: name.clone(),
                            });
                        }
                    }
                    StatsInput::Exit => {
                        println!("\n");
                        break;
                    }
                    StatsInput::Back => {
                        stack.pop();
                    }
                    _ => {}
                }
                println!("\n");
            }

            StatsViewState::DeckCategory { deck_id, deck_name } => {
                let summary = storage.get_deck_stats_summary(deck_id)?;
                let leeches = storage.get_leech_cards(deck_id, 5)?;

                let mut table = Table::new();
                table
                    .load_preset(UTF8_FULL)
                    .apply_modifier(UTF8_ROUND_CORNERS)
                    .set_header(vec!["Status", "Count", "Percentage"]);

                let total = summary.total_cards as f32;
                let row = |label: &str, count: i64| {
                    let pct = if total > 0.0 {
                        (count as f32 / total) * 100.0
                    } else {
                        0.0
                    };
                    vec![label.to_string(), count.to_string(), format!("{:.1}%", pct)]
                };

                table.add_row(row("Mature (Interval >= 7d)", summary.mature_count));
                table.add_row(row("Learning", summary.learning_count));
                table.add_row(row("New", summary.new_count));

                println!("\n--- Deck: {} ---\n{}", deck_name, table);
                println!("Average Easiness: {:.2}", summary.average_easiness);

                if !leeches.is_empty() {
                    println!("\nTop Leech Cards (Most Missed):");
                    for (term, count) in leeches {
                        println!(" - {}: {} misses", term, count);
                    }
                }
                println!();
                match stats_input("[Enter] View Cards | [Esc] Back | [q] Exit: ")? {
                    StatsInput::Confirm => {
                        stack.push(StatsViewState::DeckByCard {
                            deck_id,
                            deck_name: deck_name.clone(),
                            page: 0,
                        });
                    }
                    StatsInput::Back => {
                        stack.pop();
                    }
                    StatsInput::Exit => {
                        println!("\n");
                        break;
                    }
                    _ => {}
                }
                println!("\n");
            }

            StatsViewState::DeckByCard {
                deck_id,
                deck_name,
                page,
            } => {
                let limit = size;
                let offset = page * size;
                let total_cards = storage.get_deck_card_count(deck_id)?;
                let total_pages = (total_cards as f32 / size as f32).ceil() as u32;
                let cards = storage.get_cards_paginated(deck_id, limit, offset)?;

                let mut table = Table::new();
                table
                    .load_preset(UTF8_FULL)
                    .apply_modifier(UTF8_ROUND_CORNERS)
                    .set_content_arrangement(ContentArrangement::Dynamic)
                    .set_header(vec![
                        "Term",
                        "Definition",
                        "Score",
                        "Interval",
                        "EF",
                        "Next Due",
                    ]);

                for c in cards {
                    let due_str = if c.next_due == 0 {
                        "Now".to_string()
                    } else {
                        Utc.timestamp_opt(c.next_due, 0)
                            .unwrap()
                            .format("%Y-%m-%d")
                            .to_string()
                    };

                    table.add_row(vec![
                        truncate(&c.term, 20),
                        truncate(&c.definition, 30),
                        c.learning_score.to_string(),
                        format!("{}d", c.interval),
                        format!("{:.2}", c.easiness),
                        due_str,
                    ]);
                }

                println!(
                    "\n--- {}: Cards (Page {}/{}) ---\n{}",
                    deck_name,
                    page + 1,
                    total_pages.max(1),
                    table
                );

                match stats_input("Use arrows to paginate | [Esc] Back | [q] Exit: ")? {
                    StatsInput::Down if page + 1 < total_pages => {
                        stack.pop();
                        stack.push(StatsViewState::DeckByCard {
                            deck_id,
                            deck_name: deck_name.clone(),
                            page: page + 1,
                        });
                    }
                    StatsInput::Up if page > 0 => {
                        stack.pop();
                        stack.push(StatsViewState::DeckByCard {
                            deck_id,
                            deck_name: deck_name.clone(),
                            page: page - 1,
                        });
                    }
                    StatsInput::Back => {
                        stack.pop();
                    }
                    StatsInput::Exit => {
                        println!("\n");
                        break;
                    }
                    _ => {}
                }
                println!("\n");
            }
        }
    }

    Ok(())
}
