---
name: quizzy-import-quizlet
description: Import a Quizlet set by requesting or loading its API JSON response. Use this when the user asks to import a deck from a Quizlet URL or you have a JSON file of the API response.
---
# Import from Quizlet

## Inputs expected
- name: desired name for the imported deck (optional, will be prompted if missing)
- url_or_json: Quizlet URL or path to a saved Quizlet Web API JSON file (optional, will be prompted if missing)

## Instructions
- Run `quizzy import <name> <url_or_json>`.
- If a Quizlet URL is provided, the tool will output a Quizlet Web API link and instruct the user to open it in a browser, save the JSON response to a file, and input that file path.
- If a local JSON file path is provided directly, the tool will parse and import cards from it immediately.

## Example prompts
- "Import Quizlet deck from 'vocab.json' as 'German Vocab'" -> `quizzy import "German Vocab" vocab.json`
- "Import from Quizlet URL https://quizlet.com/1234567890/some-deck/" -> `quizzy import` (then follow the interactive instructions, since no name was provided, or assume a name like "Some Deck" based on the URL and prompt for confirmation or change)

## Edge cases
- Direct Quizlet URL scrapes: Quizlet requires browser authentication/cookies to fetch study items directly, so the tool prints a specific API endpoint URL. The user must copy/paste that URL into their browser, save the output JSON, and feed the file path to the importer.
- Missing name/url: run the command without arguments to be prompted step-by-step.

## Related skills
- `create-deck` to manually initialize empty decks.
- `card-operations` to modify cards after import.
