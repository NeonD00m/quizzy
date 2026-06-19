---
name: quizzy-export-deck-to-file
description: Exports a quizzy deck by name or file to a new file. Use this when the user asks to clone/copy a deck to a file, save a changed deck back to its original file, or export to a specific format (CSV/JSON/Markdown).
---
# Export Deck / Clone Deck File Skill

## Inputs expected
- source_deck (required): deck name or path
- destination_path with output format (required): new file path with target format (json, csv, tsv)

## Instructions
- Use `quizzy export <deck> <destination>` to export cards or clone decks
- Ask user before overwriting files or if it fails because of name conflict, ask them to provide a new name or delete the old file first.
- Return the new file path and a brief summary (card count, format)

## Example prompts
- "Export 'Biology' as CSV" -> `quizzy export Biology bio.csv`
- "Clone deck file 'decks/old.json'" -> `quizzy export decks/old.json decks/old-copy.json`

## Edge cases
- Format-specific limitations (e.g., tabs within card text for tsv): warn the user if the chosen format may cause data loss or require escaping
- If dealing with non-text media, try reading it through another method and try your best to describe it appropriately in the term/definition space of the card.

## Related skills
If the user isn't specific enough about which deck they want you to export, you may use `quizzy list` to view all decks by their deck id and name (add `-v` to see card count and creation date), or `quizzy stats` to view decks by pages with their last studied dates. Then you can prompt the user to clarify which deck or rename duplicate decks to have unique names.
- `card-operations` and `deck-management` for pre-export cleanup or stats
