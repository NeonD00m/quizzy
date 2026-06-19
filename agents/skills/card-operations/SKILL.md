---
name: quizzy-card-operations
description: Add, append, remove, or clear cards from a saved deck. Use this when the user wants to edit the cards in an existing saved deck.
---
# Card Operations

## Inputs expected
- deck: deck name (required)
- term: the card term/front (required for add and remove)
- definition: the card definition/back (required for add)
- source: path to a CSV/TSV/JSON file or another deck name (required for append)
- confirm: boolean flag to skip prompt for clearing (optional)

## Instructions
- Use `quizzy add <deck> <term> <definition>` to add a new card.
- Use `quizzy append <deck> <source>` to append cards from another file or deck.
- Use `quizzy remove <deck> <term>` to remove a card by its term.
- Use `quizzy clear <deck> --confirm` to clear all cards from a deck.

## Example prompts
- "Add 'coche' -> 'car' to Spanish vocab" -> `quizzy add "Spanish vocab" coche car`
- "Append terms from 'new_terms.tsv' into French" -> `quizzy append French new_terms.tsv`
- "Delete the card for 'mitosis' in Biology" -> `quizzy remove Biology mitosis`
- "Clear all cards from the Temp deck without asking" -> `quizzy clear Temp --confirm`

## Edge cases
- Removing a card that doesn't exist: first, you can search for similar cards using `quizzy list <deck> <search_term>` and if either term or defintion contains search term, it will be listed. If not found, inform the user that the term was not found in the specified deck.
- Appending from an invalid source: verify file path or deck existence before calling.
- Clear command: always use `--confirm` or `-c` if you want to perform it non-interactively without user prompts.

## Related skills
- `create-deck` to make a deck first before adding cards.
- `deck-management` for deck-level operations like rename or delete.
