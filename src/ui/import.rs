use crate::core::deck::*;
use crate::core::import::*;
use crate::core::storage::Storage;
use anyhow::Context;
use std::fs::File;
use std::io::{BufReader, stdin};
use std::path::Path;
use url::Url;

fn ask_for_json(url: String) -> anyhow::Result<String> {
    let parsed = Url::parse(url.as_str());
    Ok(
        if let Ok(exact_url) = parsed
            && exact_url.scheme() == "https"
        {
            let set_id = extract_set_id(exact_url).context(
                "Error parsing a set id. Check that the url looks similar to the example above.",
            )?;

            println!(
                "Please open the url on the next line with your browser to retrieve the json from the api. Once it is done loading, right click and select \"Save as...\" then enter the path to the json file in the prompt below or run `quizzy import <required-name> <file-path>`\n"
            );
            println!(
                "https://quizlet.com/webapi/3.9/studiable-item-documents?filters%5BstudiableContainerId%5D={}&filters%5BstudiableContainerType%5D=1&perPage=100&page=1\n",
                set_id
            );
            let mut input = String::new();
            stdin()
                .read_line(&mut input)
                .context("Error reading json path.")?;
            input.trim().to_string()
        } else {
            url
        },
    )
}

pub fn import_from_quizlet(
    name: Option<String>,
    url: Option<String>,
    storage: &mut Storage,
) -> anyhow::Result<()> {
    println!(
        "Quizzy currently only has the ability to import from Quizlet, let me know via github to create other options!"
    );

    let mut input = String::new();
    let name = match name {
        Some(n) => n,
        None => {
            println!("What would you like to name the set?");
            input.clear();
            stdin()
                .read_line(&mut input)
                .context("Error reading input for set name.")?;
            input.trim().to_string()
        }
    };
    let url = match url {
        Some(u) => u,
        None => {
            println!(
                "Please paste in the url for the quizlet deck you would like to import (or the path to the generated json), it should look something like this:\n"
            );
            println!("https://quizlet.com/1234567890/some-study-deck-flash-cards/...\n");
            input.clear();
            stdin()
                .read_line(&mut input)
                .context("Error reading input for url.")?;
            input.trim().to_string()
        }
    };

    let json_path = ask_for_json(url).context("Error asking for json.")?;
    let file = File::open(Path::new(&json_path)).context("Failed to open file.")?;
    let reader = BufReader::new(file);
    let json_deck: ApiResponse =
        serde_json::from_reader(reader).context("Failed to parse JSON deck.")?;
    let cards = extract_cards(json_deck)?;
    println!("Successfully fetched {} cards from Quizlet.", cards.len());

    let mut deck = Deck::from_cards(cards);
    deck.name = name;

    storage.create_deck_from_core(deck, None)?;
    println!("Deck successfully saved to registry.");
    Ok(())
}
