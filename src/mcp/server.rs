use crate::core::deck::{DeckSource, resolve_deck_source};
use crate::core::storage::{DeckListItem, Storage, get_deck};
use anyhow::Context;
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::tool::ToolRouter,
    model::{
        CallToolResult, Implementation, InitializeResult, ProtocolVersion, ServerCapabilities,
        ServerInfo,
    },
    tool, tool_handler, tool_router,
    transport::stdio,
};
use tokio::runtime::Runtime;
use tracing_subscriber::EnvFilter;

//debug and inspect with browser: npx @modelcontextprotocol/inspector cargo run mcp

#[derive(Clone)]
pub struct McpServer {
    tool_router: ToolRouter<Self>, // This is a required field
}

#[tool_router]
impl McpServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    // TODO: tools
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
pub async fn launch(storage: &Storage) -> anyhow::Result<()> {
    // initialize stdout logging
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::DEBUG.into()))
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();
    tracing::info!("Starting Quizzy MCP Server");

    // create instance of MCP server on stdio
    let service = McpServer::new().serve(stdio()).await.inspect_err(|e| {
        tracing::error!("serving error: {:?}", e);
    })?;
    service.waiting().await?;
    Ok(())
}
