use crate::core::storage::Storage;
use chrono::{TimeZone, Utc};
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    model::{
        CallToolResult, ContentBlock, Implementation, InitializeResult, ProtocolVersion,
        ServerCapabilities, ServerInfo,
    },
    tool, tool_handler, tool_router,
    transport::stdio,
};
use schemars::JsonSchema;
use serde::Deserialize;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

#[derive(Deserialize, JsonSchema)]
pub struct NewCard {
    pub term: String,
    pub definition: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct ListDecksRequest {
    pub search: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct GetDeckCardsRequest {
    pub deck_id: i64,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Deserialize, JsonSchema)]
pub struct DeckIdRequest {
    pub deck_id: i64,
}

#[derive(Deserialize, JsonSchema)]
pub struct CreateDeckRequest {
    pub name: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct AddCardsRequest {
    pub deck_id: i64,
    pub cards: Vec<NewCard>,
}

#[derive(Deserialize, JsonSchema)]
pub struct RemoveCardsRequest {
    pub card_ids: Vec<i64>,
}

#[derive(Deserialize, JsonSchema)]
pub struct EditCardRequest {
    pub card_id: i64,
    pub term: Option<String>,
    pub definition: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct RenameDeckRequest {
    pub deck_id: i64,
    pub new_name: String,
}

#[derive(Clone)]
pub struct McpServer {
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>, // This is a required field
    storage: Arc<tokio::sync::Mutex<Storage>>,
}

#[tool_router]
impl McpServer {
    pub fn new(storage: Storage) -> Self {
        Self {
            tool_router: Self::tool_router(),
            storage: Arc::new(tokio::sync::Mutex::new(storage)),
        }
    }

    #[tool(description = "List all available decks, optionally filtering by name")]
    async fn list_decks(
        &self,
        params: Parameters<ListDecksRequest>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!("list_decks: called with search: {:?}", params.0.search);
        let storage = self.storage.lock().await;
        tracing::debug!("list_decks: storage lock acquired");
        let mut items = storage.list_decks_detailed().map_err(|e| {
            tracing::error!("list_decks: database error: {:?}", e);
            McpError::internal_error(format!("Error listing decks: {}", e), None)
        })?;

        if let Some(search_str) = &params.0.search {
            let search_lower = search_str.to_lowercase();
            items.retain(|item| item.name.to_lowercase().contains(&search_lower));
        }

        tracing::info!("list_decks: {} decks found after filtering", items.len());

        if items.is_empty() {
            return Ok(CallToolResult::success(vec![ContentBlock::text(
                "No decks found.",
            )]));
        }

        let content_str: String = items
            .iter()
            .map(|item| {
                let create_str = Utc
                    .timestamp_opt(item.created_at, 0)
                    .single()
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_else(|| "Never".to_string());
                let update_str = Utc
                    .timestamp_opt(item.updated_at, 0)
                    .single()
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_else(|| "Never".to_string());
                format!(
                    "Deck ID: {}\tName: {}\tCards: {}\tCreated: {}\tUpdated: {}",
                    item.id, item.name, item.card_count, create_str, update_str
                )
            })
            .collect::<Vec<String>>()
            .join("\n");

        tracing::info!("list_decks: completed successfully");
        Ok(CallToolResult::success(vec![ContentBlock::text(
            content_str,
        )]))
    }

    #[tool(description = "Get paginated cards belonging to a deck by its unique deck_id")]
    async fn get_deck_cards(
        &self,
        params: Parameters<GetDeckCardsRequest>,
    ) -> Result<CallToolResult, McpError> {
        let req = params.0;
        tracing::info!(
            "get_deck_cards: called for deck_id = {}, limit = {:?}, offset = {:?}",
            req.deck_id,
            req.limit,
            req.offset
        );
        let storage = self.storage.lock().await;
        tracing::debug!("get_deck_cards: storage lock acquired");
        let limit_val = req.limit.unwrap_or(50);
        let offset_val = req.offset.unwrap_or(0);

        let cards = storage
            .get_cards_paginated(req.deck_id, limit_val, offset_val)
            .map_err(|e| {
                tracing::error!("get_deck_cards: database error: {:?}", e);
                McpError::internal_error(
                    format!("Error fetching cards for deck {}: {}", req.deck_id, e),
                    None,
                )
            })?;

        tracing::info!("get_deck_cards: fetched {} cards", cards.len());

        if cards.is_empty() {
            return Ok(CallToolResult::success(vec![ContentBlock::text(
                "No cards found in this range.",
            )]));
        }

        let content_str = cards
            .iter()
            .map(|card| {
                format!(
                    "Card ID: {}\tTerm: {}\tDefinition: {}\tLearning Score: {}", //\tInterval: {}\tEasiness: {:.2}",
                    card.card_id,
                    card.term,
                    card.definition,
                    card.learning_score //, card.interval, card.easiness
                )
            })
            .collect::<Vec<String>>()
            .join("\n");

        tracing::info!("get_deck_cards: completed successfully");
        Ok(CallToolResult::success(vec![ContentBlock::text(
            content_str,
        )]))
    }

    #[tool(
        description = "Get performance statistics and leech cards for a deck by its unique deck_id"
    )]
    async fn get_deck_stats(
        &self,
        params: Parameters<DeckIdRequest>,
    ) -> Result<CallToolResult, McpError> {
        let deck_id = params.0.deck_id;
        tracing::info!("get_deck_stats: called for deck_id = {}", deck_id);
        let storage = self.storage.lock().await;
        tracing::debug!("get_deck_stats: storage lock acquired");

        let summary = storage.get_deck_stats_summary(deck_id).map_err(|e| {
            tracing::error!("get_deck_stats: stats summary database error: {:?}", e);
            McpError::internal_error(format!("Error fetching stats summary: {}", e), None)
        })?;

        let leeches = storage.get_leech_cards(deck_id, 5).map_err(|e| {
            tracing::error!("get_deck_stats: leech cards database error: {:?}", e);
            McpError::internal_error(format!("Error fetching leech cards: {}", e), None)
        })?;

        tracing::info!("get_deck_stats: fetched stats summary and leech cards successfully");

        let mut lines = vec![
            format!("Deck ID: {}", deck_id),
            format!("Total Cards: {}", summary.total_cards),
            format!("New Cards (Unstudied): {}", summary.new_count),
            format!("Learning Cards: {}", summary.learning_count),
            format!("Mature Cards: {}", summary.mature_count),
            format!("Average Stability (Days): {:.2}", summary.average_easiness),
        ];

        if !leeches.is_empty() {
            lines.push("\nTop Leech Cards (Most Incorrect Answers):".to_string());
            for (term, count) in leeches {
                lines.push(format!("  - {} (failed {} times)", term, count));
            }
        } else {
            lines.push("\nNo leech cards found (no incorrect answers recorded yet).".to_string());
        }

        tracing::info!("get_deck_stats: completed successfully");
        Ok(CallToolResult::success(vec![ContentBlock::text(
            lines.join("\n"),
        )]))
    }

    #[tool(description = "Create a new empty deck")]
    async fn create_deck(
        &self,
        params: Parameters<CreateDeckRequest>,
    ) -> Result<CallToolResult, McpError> {
        let name = params.0.name;
        tracing::info!("create_deck: called with name = '{}'", name);
        let mut storage = self.storage.lock().await;
        tracing::debug!("create_deck: storage lock acquired");

        let new_deck = crate::core::deck::Deck {
            id: None,
            name: name.clone(),
            cards: vec![],
        };

        let (new_id, new_name) = storage.create_deck_from_core(new_deck, None).map_err(|e| {
            tracing::error!("create_deck: database error: {:?}", e);
            McpError::internal_error(format!("Failed to create deck '{}': {}", name, e), None)
        })?;

        tracing::info!(
            "create_deck: successfully created deck '{}' with ID = {}",
            new_name,
            new_id
        );
        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
            "Successfully created deck '{}' with ID: {}",
            new_name, new_id
        ))]))
    }

    #[tool(description = "Add multiple new cards to a deck in bulk")]
    async fn add_cards(
        &self,
        params: Parameters<AddCardsRequest>,
    ) -> Result<CallToolResult, McpError> {
        let req = params.0;
        tracing::info!(
            "add_cards: called for deck_id = {} with {} card(s)",
            req.deck_id,
            req.cards.len()
        );
        let mut storage = self.storage.lock().await;
        tracing::debug!("add_cards: storage lock acquired");

        // Verify deck exists
        if let Err(e) = storage.get_deck_by_id(req.deck_id) {
            tracing::error!("add_cards: deck lookup failed: {:?}", e);
            return Err(McpError::invalid_params(
                format!("Deck with ID {} not found: {}", req.deck_id, e),
                None,
            ));
        }

        let core_cards: Vec<crate::core::deck::Card> = req
            .cards
            .into_iter()
            .map(|c| crate::core::deck::Card {
                id: None,
                term: c.term,
                definition: c.definition,
            })
            .collect();

        let count = core_cards.len();
        storage
            .add_cards_to_deck_batch(req.deck_id, core_cards, false)
            .map_err(|e| {
                tracing::error!("add_cards: batch insertion failed: {:?}", e);
                McpError::internal_error(format!("Failed to add cards: {}", e), None)
            })?;

        tracing::info!(
            "add_cards: successfully added {} card(s) to deck {}",
            count,
            req.deck_id
        );
        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
            "Successfully added {} card(s) to deck ID: {}",
            count, req.deck_id
        ))]))
    }

    #[tool(
        description = "Remove multiple cards from the database by their unique card_ids in bulk"
    )]
    async fn remove_cards(
        &self,
        params: Parameters<RemoveCardsRequest>,
    ) -> Result<CallToolResult, McpError> {
        let card_ids = params.0.card_ids;
        tracing::info!("remove_cards: called with {} card_id(s)", card_ids.len());
        let mut storage = self.storage.lock().await;
        tracing::debug!("remove_cards: storage lock acquired");

        let count = card_ids.len();
        for id in &card_ids {
            storage.remove_card(*id).map_err(|e| {
                tracing::error!(
                    "remove_cards: database error removing card ID {}: {:?}",
                    id,
                    e
                );
                McpError::internal_error(
                    format!("Failed to remove card with ID {}: {}", id, e),
                    None,
                )
            })?;
        }

        tracing::info!("remove_cards: successfully removed {} card(s)", count);
        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
            "Successfully removed {} card(s).",
            count
        ))]))
    }

    #[tool(
        description = "Update the term and/or definition of an existing card by its unique card_id"
    )]
    async fn edit_card(
        &self,
        params: Parameters<EditCardRequest>,
    ) -> Result<CallToolResult, McpError> {
        let req = params.0;
        tracing::info!(
            "edit_card: called for card_id = {} with term = {:?}, definition = {:?}",
            req.card_id,
            req.term,
            req.definition
        );
        let mut storage = self.storage.lock().await;
        tracing::debug!("edit_card: storage lock acquired");

        if req.term.is_none() && req.definition.is_none() {
            tracing::error!("edit_card: missing fields to update");
            return Err(McpError::invalid_params(
                "Must provide at least a term or definition to update.",
                None,
            ));
        }

        storage
            .update_card(req.card_id, req.term.as_deref(), req.definition.as_deref())
            .map_err(|e| {
                tracing::error!("edit_card: database error: {:?}", e);
                McpError::internal_error(
                    format!("Failed to update card ID {}: {}", req.card_id, e),
                    None,
                )
            })?;

        tracing::info!("edit_card: successfully updated card ID {}", req.card_id);
        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
            "Successfully updated card ID: {}",
            req.card_id
        ))]))
    }

    #[tool(description = "Rename a deck by its unique deck_id")]
    async fn rename_deck(
        &self,
        params: Parameters<RenameDeckRequest>,
    ) -> Result<CallToolResult, McpError> {
        let req = params.0;
        tracing::info!(
            "rename_deck: called for deck_id = {} to new_name = '{}'",
            req.deck_id,
            req.new_name
        );
        let mut storage = self.storage.lock().await;
        tracing::debug!("rename_deck: storage lock acquired");

        storage
            .rename_deck(req.deck_id, &req.new_name)
            .map_err(|e| {
                tracing::error!("rename_deck: database error: {:?}", e);
                McpError::internal_error(
                    format!("Failed to rename deck ID {}: {}", req.deck_id, e),
                    None,
                )
            })?;

        tracing::info!(
            "rename_deck: successfully renamed deck ID {} to '{}'",
            req.deck_id,
            req.new_name
        );
        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
            "Successfully renamed deck ID {} to '{}'.",
            req.deck_id, req.new_name
        ))]))
    }
}

#[tool_handler]
impl ServerHandler for McpServer {
    fn get_info(&self) -> rmcp::model::ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
        .with_protocol_version(ProtocolVersion::V_2025_06_18)
        .with_server_info(Implementation::from_build_env())
        .with_instructions("I manage a terminal-based study app with locally stored flashcard decks for the user.

            You must use unique ID numbers (deck_id and card_id) instead of literal names or terms when performing operations on decks and cards. Always reference them by their IDs.

            Key guidelines:
            - Start by listing decks using `list_decks` to find their deck IDs.
            - To fetch the cards in a deck, use `get_deck_cards` with the appropriate deck_id.
            - To see performance stats, use `get_deck_stats` with the deck_id.
            - You can modify the decks using `add_cards`, `remove_cards`, `edit_card`, and `rename_deck`.
            - Destructive bulk actions (deleting decks, clearing decks) are not supported via MCP for safety reasons.
            ".to_string())
    }

    async fn initialize(
        &self,
        _request: rmcp::model::InitializeRequestParams,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> anyhow::Result<InitializeResult, McpError> {
        Ok(self.get_info())
    }
}

/// launch the mcp server with tokio and rmcp
#[tokio::main]
pub async fn launch(storage: Storage) -> anyhow::Result<()> {
    // initialize stdout logging (to stderr)
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::DEBUG.into()))
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .try_init();
    tracing::info!("Starting Quizzy MCP Server");

    // create instance of MCP server on stdio
    let service = McpServer::new(storage)
        .serve(stdio())
        .await
        .inspect_err(|e| {
            tracing::error!("serving error: {:?}", e);
        })?;
    service.waiting().await?;
    Ok(())
}
