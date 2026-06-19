---
name: quizzy-create-deck
description: Create a new saved study deck, optionally importing cards from a file or another deck. Use this when the user wants to start a new deck by name or clone an old deck to a saved deck.
---
# Create Study Deck

## Inputs expected
- name: name of the new deck (required)
- source: path to a CSV/TSV/JSON file or another deck name to clone/import cards from (optional)

## Instructions
- Use `quizzy new <name>` to create an empty deck saved in the local database.
- Use `quizzy new <name> <source>` to create a deck and seed it with cards from the file or other deck.

## Example prompts
- "Create an empty deck called Chemistry" -> `quizzy new Chemistry`
- "Create a deck named 'Biology Terms' and seed it with cards from 'examples/tutorial.csv'" -> `quizzy new "Biology Terms" examples/tutorial.csv`
- "Clone 'Spanish Phrases' into a new deck named 'Spanish 101'" -> `quizzy new "Spanish 101" "Spanish Phrases"`

## Edge cases
- Duplicate deck name: Quizzy will create the deck, but if there are multiple decks with the same name, subcommands will prompt you to select by ID. It's recommended to use unique names.
- File-backed imports: ensure the source file path is valid and readable before calling the command.

## Related skills
- `card-operations` to add/remove cards once the deck is created.
- `import-quizlet` to import cards from a Quizlet URL.
- `deck-management` to rename, delete,  decks.
