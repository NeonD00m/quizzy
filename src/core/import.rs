use crate::{core::deck::*, ui::cards::cards_mode};
use anyhow::Context;
use serde::Deserialize;
use std::fs::File;
use std::io::{BufReader, stdin};
use std::path::Path;
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
#[derive(Deserialize, Clone)]
struct StudiableItem {
    cardSides: Vec<CardSide>,
}

#[derive(Deserialize, Clone)]
struct CardSide {
    media: Vec<Media>,
}

#[allow(non_snake_case)]
#[derive(Deserialize, Clone)]
struct Media {
    r#type: String,
    plainText: Option<String>,
    url: Option<String>,
}

fn extract_set_id(parsed: Url) -> Option<String> {
    // let parsed = Url::parse(url).ok()?;
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
    let name = name.unwrap_or_else(|| {
        println!("What would you like to name the set?");
        input.clear();
        stdin().read_line(&mut input).expect("Error reading input.");
        input.trim().to_string()
    });
    let url = url.unwrap_or_else(|| {
        println!(
            "Please paste in the url for the quizlet deck you would like to import (or the path to the generated json), it should look something like this:"
        );
        println!("https://quizlet.com/1234567890/some-study-deck-flash-cards/...");
        input.clear();
        stdin().read_line(&mut input).expect("Error reading input.");
        input.trim().to_string()
    });

    let json_path: String;
    let parsed = Url::parse(url.as_str());
    if let Ok(exact_url) = parsed
        && exact_url.scheme() == "https"
    {
        let set_id = extract_set_id(exact_url).context(
            "Error parsing a set id. Check that the url looks similar to the example above.",
        )?;

        println!(
            "Please open the url on the next line with your browser to retrieve the json from the api. Once it is done loading, right click and click \"Save as...\" then enter the path to the json file in the prompt below or run `quizzy import <required-name> <file-path>`"
        );
        println!(
            "https://quizlet.com/webapi/3.9/studiable-item-documents?filters%5BstudiableContainerId%5D={}&filters%5BstudiableContainerType%5D=1&perPage=100&page=1",
            set_id
        );
        input.clear();
        stdin()
            .read_line(&mut input)
            .context("Error reading json path (2).")?;
        json_path = input.trim().to_string();
    } else {
        json_path = url;
    }

    let file = File::open(Path::new(&json_path)).expect("Failed to open file.");
    let reader = BufReader::new(file);
    let json_deck: ApiResponse =
        serde_json::from_reader(reader).expect("Failed to parse JSON deck.");
    let first_response = json_deck
        .responses
        .get(0)
        .context("No responses found in JSON.")?;
    let cards: Vec<Card> = first_response
        .models
        .clone()
        .into_iter()
        .map(|item| {
            Card::new(
                &item.cardSides[0]
                    .media
                    .iter()
                    .filter(|m| m.r#type == String::from("1"))
                    .next()
                    .expect("Media of type 1 not found for front side of card")
                    .plainText
                    .clone()
                    .expect("Media (1) was type 1 and did not have plainText"),
                &item.cardSides[1]
                    .media
                    .iter()
                    .filter(|m| m.r#type == String::from("1"))
                    .next()
                    .expect("Media of type 1 not found for back side of card")
                    .plainText
                    .clone()
                    .expect("Media (2) was type 1 and did not have plainText"),
            )
        })
        .collect();
    println!("Successfully fetched {} cards from Quizlet.", cards.len());

    let mut deck = Deck::from_cards(cards);
    deck.name = name;

    println!(
        "Since storage hasn't been implemented yet, you can just study with flashcards for now."
    );
    cards_mode(deck, false)
}
