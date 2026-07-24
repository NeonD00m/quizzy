use crate::core::storage::Storage;
use chrono::{TimeZone, Utc};
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::tool::ToolRouter,
    model::{
        CallToolResult, ContentBlock, Implementation, InitializeResult, ProtocolVersion,
        ServerCapabilities, ServerInfo,
    },
    tool, tool_handler, tool_router,
    transport::stdio,
};
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

//debug and inspect with browser: npx @modelcontextprotocol/inspector cargo run mcp

#[derive(Clone)]
pub struct McpServer {
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

    // TODO: tools
    #[tool(description = "List all available decks with card counts and update times")]
    async fn list_decks(&self) -> Result<CallToolResult, McpError> {
        let storage = self.storage.lock().await;
        let items = storage
            .list_decks_detailed()
            .map_err(|e| McpError::internal_error(format!("Error listing decks: {}", e), None))?;

        tracing::info!("{} decks found", items.len());

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
                    "({})\t{}\t {} cards\t Created At: {}\t Updated At: {}",
                    item.id, item.name, item.card_count, create_str, update_str
                )
            })
            .collect::<Vec<String>>()
            .join("\n");

        Ok(CallToolResult::success(vec![ContentBlock::text(
            content_str,
        )]))
    }
}

#[tool_handler]
impl ServerHandler for McpServer {
    fn get_info(&self) -> rmcp::model::ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools() // only enables tools, not prompts/resources
            .build())
        .with_protocol_version(ProtocolVersion::V_2025_06_18)
        .with_server_info(Implementation::from_build_env())
        .with_instructions("I manage a terminal-based study app with locally stored flashcard decks for the user to study.

            The available actions are:
            * list_decks: List all available decks with card counts and update times
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
    // initialize stdout logging
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::DEBUG.into()))
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();
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
