---
name: quizzy-deck-management
description: Manage deck-level operations like rename, delete, listing, and showing statistics. Use this when the user wants to rename, delete, list, or view stats/metadata of saved decks.
---
# Deck Management

## Inputs expected
- deck: deck name or ID (required for rename, delete, optional for stats, list)
- new_name: new name for the deck (required for rename)
- search: search term to filter decks or cards (optional for list)
- verbose: boolean flag for detailed output (optional for list)
- size: page size (optional for stats)
- page: page number (optional for stats)

## Instructions
- Use `quizzy rename <deck> <new_name>` to rename a saved deck.
- Use `quizzy delete <deck>` to delete a deck and its stats from the database.
- Use `quizzy list` to list all saved decks (or `quizzy list -v` for details like card count).
- Use `quizzy list <deck>` to list all cards in a deck.
- Use `quizzy list <deck> <search_term>` to list all cards that contain the search term.
- Use `quizzy stats` to view overall learning stats, or `quizzy stats <deck>` for a specific deck.

## Example prompts
- "Rename deck 'Biology Old' to 'Biology 101'" -> `quizzy rename "Biology Old" "Biology 101"`
- "Delete the deck 'Draft Vocab' from the database" -> `quizzy delete "Draft Vocab"`
- "List all of my saved study decks" -> `quizzy list`
- "Show me detailed stats for the 'Spanish' deck" -> `quizzy stats Spanish`
- "List cards in 'German' containing 'haus'" -> `quizzy list German haus`

## Edge cases
- Renaming or deleting: if there are multiple decks with the same name, you will be prompted to select by ID.
- Deletion is permanent and cannot be undone (associated learning stats are also deleted)---ASK THE USER

## Related skills
- `create-deck` to make new decks.
- `card-operations` to add/remove/append individual cards.
