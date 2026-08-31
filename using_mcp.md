# Model Context Protocol (MCP) Integration

Quizzy includes a built-in MCP server (`quizzy mcp`) that allows AI agents (Claude, Cursor, etc.) to query, generate, edit, and manage flashcards and decks directly.

## Local Client Configuration

Ensure `quizzy` is built and accessible in your system PATH.

### Desktop Agent Applications (Claude, Antigravity, Cursor)
This same code snippet probably works in all of them, they just have different files or file paths:

In Antigravity, MCP servers are registered via `.antigravity/mcp.json` at the root of your workspace or globally in `~/.config/antigravity/mcp.json`.
For Claude, add to `~/.claude.json` (the file the cli uses) or:
- macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`
- Windows: `%APPDATA%\Claude\claude_desktop_config.json (typically C:\Users\YourName\AppData\Roaming\Claude\claude_desktop_config.json)`
In Cursor, write this in `.cursor/mcp.json` or project `.mcp.json`:
```json
{
  "mcpServers": {
    "quizzy": {
      "command": "quizzy",
      "args": ["mcp"]
    }
  }
}

```

### CLI Agent tools (Claude Code, Antigravity CLI, Codex CLI, etc.)
Just swap in your preferred tool (claude) and they all have the same command pretty much.

Run:
`<tool> mcp add quizzy -- quizzy mcp`
Where "<tool>" = `claude` or `agy` or `codex` or whatever

### Antigravity CLI

To register Quizzy directly into an active Antigravity CLI environment over standard I/O:

```bash
# Register Quizzy as an active MCP tool endpoint
antigravity mcp add quizzy -- quizzy mcp

# Verify tool registration
antigravity mcp list

```

## Capabilities & Tools Exposed

* `list_decks`: Search and list local decks and card counts.
* `create_deck`: Create a new empty deck.
* `get_deck_cards`: List cards in a deck by `deck_id` (paginated).
* `add_cards`: Append cards to a deck in bulk.
* `edit_card`: Update existing card content by `card_id`.
* `remove_cards`: Remove cards by `card_ids` (bulk).
* `rename_deck`: Rename a deck by `deck_id`.
* `get_deck_stats`: View deck retention, card mastery scores, and review metrics.

The AI Agent is NOT given the ability to delete a deck (at all) or clear all of a deck's cards (in one command).

## Testing Agent Functionality

Try prompting your AI assistant with:

1. *"Create a deck called 'Docker Basics' with 3 core commands."*
2. *"List my decks to verify 'Docker Basics' exists."*
3. *"Edit card 1 in 'Docker Basics' to fix typos."*

## Environment Overrides & Stdio Hygiene

| Variable | Description | Default |
| --- | --- | --- |
| `QUIZZY_DB` | Overrides the default platform path to target a specific SQLite database file. | Platform default directory via `dirs-next`<br> |
| `RUST_LOG` | Controls internal log verbosity (keep set to `error` to avoid stream pollution). | `error` |

> **Warning on Stdio Transport:** When building custom agent scripts or running CLI harnesses, ensure debug output from the parent process is not piped into `stdout`. Non-JSON messages written to standard output will break the MCP frame parser.
