use crate::{core::deck::*, ui::cards::cards_mode};
use serde::Deserialize;
use std::io::stdin;
use url::Url;

#[derive(Deserialize)]
struct ApiResponse {
    responses: Vec<ResponseItem>,
}

#[derive(Deserialize)]
struct ResponseItem {
    models: Vec<StudiableItem>,
}

#[derive(Deserialize)]
struct StudiableItem {
    cardSides: Vec<CardSide>,
}

#[derive(Deserialize)]
struct CardSide {
    media: Vec<Media>,
}

#[derive(Deserialize)]
struct Media {
    plainText: String,
}

async fn fetch_cards(set_id: &str) -> anyhow::Result<Vec<Card>> {
    let url = format!(
        "https://quizlet.com/webapi/3.4/studiable-item-documents\
    ?filters[studiableContainerId]={}\
    &filters[studiableContainerType]=1\
    &perPage=1000&page=1",
        set_id
    );

    let res: ApiResponse = reqwest::get(url).await?.json().await?;

    let mut cards = Vec::new();

    for response in res.responses {
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

pub fn import_from_quizlet(name: Option<String>, url: Option<String>) {
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
        input.clone()
    });
    let name = name.unwrap_or_else(|| {
        println!("What would you like to name the set?");
        input.clear();
        stdin().read_line(&mut input).expect("Error reading input.");
        input.clone()
    });

    let set_id = extract_set_id(url.trim())
        .expect("Error parsing input. Check that the url looks similar to the example above.");

    // let mut response = reqwest::blocking::get(format!(
    //     "https://quizlet.com/webapi/3.9/studiable-item-documents?filters%5BstudiableContainerId%5D={}&filters%5BstudiableContainerType%5D=1&perPage=1000&page=1",
    //     set_id
    // ));

    let deck = Deck::named(name);

    println!(
        "Since storage hasn't been implemented yet, you can just study with flashcards for now."
    );
    cards_mode(deck, false);
}
