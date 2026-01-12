use crate::{core::deck::*, ui::cards::cards_mode};
use anyhow::Context;
use rand::distributions::uniform::UniformFloat;
use serde::Deserialize;
use std::io::{Write, stdin, stdout};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::Duration;
use url::Url;

#[derive(Deserialize)]
struct ApiResponse {
    responses: Vec<ResponseItem>,
}

#[derive(Deserialize)]
struct ResponseItem {
    models: Vec<StudiableItem>,
}

#[allow(non_snake_case)]
#[derive(Deserialize)]
struct StudiableItem {
    cardSides: Vec<CardSide>,
}

#[derive(Deserialize)]
struct CardSide {
    media: Vec<Media>,
}

#[allow(non_snake_case)]
#[derive(Deserialize)]
struct Media {
    plainText: String,
}

fn fetch_cards(set_id: &str) -> anyhow::Result<Vec<Card>> {
    let url = format!(
        "https://quizlet.com/webapi/3.9/studiable-item-documents?filters%5BstudiableContainerId%5D={}&filters%5BstudiableContainerType%5D=1&perPage=100&page=1",
        set_id
    );

    // build a client first to simular a browser
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        // .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36")
        .build()?;

    let response = client.get(&url).header(
            reqwest::header::USER_AGENT,
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/116.0.0.0 Safari/537.3",
        )
        .header(
            reqwest::header::ACCEPT,
            "application/json, text/javascript, */*; q=0.01",
        ).header(
            reqwest::header::REFERER,
            "https://quizlet.com/",
        ).send()?;

    // debug :(
    let status = response.status();
    let final_url = response.url().to_string();
    // let content_type = response.
    //     .headers()
    //     .get(reqwest::header::CONTENT_TYPE)
    //     .and_then(|v| v.to_str().ok())
    //     .unwrap_or("<none>");
    let body_text = response.text()?;

    eprintln!("DEBUG: status = {}", status);
    eprintln!("DEBUG: final url = {}", final_url);
    // eprintln!("DEBUG: content type = {}", content_type);
    eprintln!(
        "DEBUG: body (first 1000 chars):\n{}",
        &body_text.chars().take(1000).collect::<String>()
    );

    let api: ApiResponse = serde_json::from_str(&body_text)
        .context("Failed to parse JSON from Quizlet response. Check debug output above.")?;
    let mut cards = Vec::new();

    for response in api.responses {
        for item in response.models {
            let term = &item.cardSides[0].media[0].plainText;
            let def = &item.cardSides[1].media[0].plainText;
            cards.push(Card::new(term.as_str(), def.as_str()));
        }
    }

    Ok(cards)
}

fn extract_set_id(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    parsed
        .path_segments()?
        .find(|seg| seg.chars().all(|c| c.is_ascii_digit()))
        .map(|s| s.to_string())
}

pub fn import_from_quizlet(name: Option<String>, url: Option<String>) -> anyhow::Result<()> {
    println!(
        "Quizzy currently only has the ability to import from Quizlet, let me know via github to create other options!"
    );

    let mut input = String::new();
    let url = url.unwrap_or_else(|| {
        println!(
            "Please paste in the url for the quizlet deck you would like to import, it should look something like this:"
        );
        println!("https://quizlet.com/1234567890/some-study-deck-flash-cards/...");
        input.clear();
        stdin().read_line(&mut input).expect("Error reading input.");
        input.trim().to_string()
    });
    let name = name.unwrap_or_else(|| {
        println!("What would you like to name the set?");
        input.clear();
        stdin().read_line(&mut input).expect("Error reading input.");
        input.trim().to_string()
    });

    let set_id = extract_set_id(url.as_str())
        .context("Error parsing input. Check that the url looks similar to the example above.")?;

    // make a spinner a build a runtime for it
    let spinner_running = Arc::new(AtomicBool::new(true));
    let spinner_flag = spinner_running.clone();
    let spinner_handle = thread::spawn(move || {
        let mut count = 0u8;
        while spinner_flag.load(Ordering::Relaxed) {
            let ch = match count % 4 {
                0 => "|",
                1 => "/",
                2 => "-",
                _ => "\\",
            };
            print!("\r{}", ch);
            let _ = stdout().flush();
            count = count.wrapping_add(1);
            thread::sleep(Duration::from_millis(250));
        }
        print!("\r                   \r");
        let _ = stdout().flush();
    });
    let rt = tokio::runtime::Runtime::new()?;

    let result = fetch_cards(set_id.as_str());

    spinner_running.store(false, Ordering::Relaxed);
    let _ = spinner_handle.join();

    let cards = result.context("Error fetching cards from Quizlet.")?;
    println!("Successfully fetched {} cards from Quizlet.", cards.len());

    let deck = Deck {
        name,
        cards,
        id: Some(0),
    };

    println!(
        "Since storage hasn't been implemented yet, you can just study with flashcards for now."
    );
    cards_mode(deck, false)
}
