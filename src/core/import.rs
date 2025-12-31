use crate::core::deck::*;
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

pub fn import() {
    println!(
        "Quizzy currently only has the ability to import from quizlet, let me know via github to create other options!"
    );

    let mut input = String::new();

    println!(
        "Please paste in the url for the quizlet deck you would like to import, it should look something like this:"
    );
    println!("https://quizlet.com/1234567890/some-study-deck-flash-cards/...");
    stdin().read_line(&mut input).expect("Error reading input.");
    let set_id = extract_set_id(input.trim())
        .expect("Error parsing input. Check that the url looks similar to the example above.");
    todo!();
}
