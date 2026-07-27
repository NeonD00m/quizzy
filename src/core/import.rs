use crate::core::deck::*;
use anyhow::{Context, anyhow};
use serde::Deserialize;
use url::Url;

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct ApiResponse {
    responses: Vec<ResponseItem>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct ResponseItem {
    models: ResponseModel,
}

#[derive(Deserialize, Clone)]
#[allow(non_snake_case)]
#[allow(dead_code)]
struct ResponseModel {
    studiableItem: Vec<StudiableItem>,
}

#[derive(Deserialize, Clone)]
#[allow(non_snake_case)]
#[allow(dead_code)]
struct StudiableItem {
    cardSides: Vec<CardSide>,
}

#[derive(Deserialize, Clone)]
#[allow(dead_code)]
struct CardSide {
    media: Vec<Media>,
}

#[derive(Deserialize, Clone)]
#[allow(non_snake_case)]
#[allow(dead_code)]
struct Media {
    r#type: i8,
    plainText: Option<String>,
    url: Option<String>,
}

pub fn extract_set_id(parsed: Url) -> Option<String> {
    // let parsed = Url::parse(url).ok()?;
    parsed
        .path_segments()?
        .find(|seg| seg.chars().all(|c| c.is_ascii_digit()))
        .map(|s| s.to_string())
}

pub fn extract_cards(json_deck: ApiResponse) -> anyhow::Result<Vec<Card>> {
    let response = json_deck
        .responses
        .first()
        .context("No responses found in JSON.")?;

    let studiable = &response.models.studiableItem;

    let mut cards = Vec::with_capacity(studiable.len());
    for (idx, item) in studiable.iter().enumerate() {
        let front_side = item
            .cardSides
            .first()
            .ok_or_else(|| anyhow!("Card {} is missing a front side", idx))?;
        let back_side = item
            .cardSides
            .get(1)
            .ok_or_else(|| anyhow!("Card {} is missing a back side", idx))?;

        let front_text = front_side
            .media
            .iter()
            .find(|m| m.r#type == 1)
            .and_then(|m| m.plainText.clone())
            .ok_or_else(|| {
                anyhow!(
                    "Front side of card {}: media type 1 with plainText not found",
                    idx
                )
            })?;

        let back_text = back_side
            .media
            .iter()
            .find(|m| m.r#type == 1)
            .and_then(|m| m.plainText.clone())
            .ok_or_else(|| {
                anyhow!(
                    "Back side of card {}: media type 1 with plainText not found",
                    idx
                )
            })?;

        cards.push(Card::new(&front_text, &back_text));
    }

    Ok(cards)
}
