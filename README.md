# **Quizzy**

[![CI](https://github.com/NeonD00m/quizzy/actions/workflows/ci.yml/badge.svg)](https://github.com/NeonD00m/quizzy/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/NeonD00m/quizzy?style=flat-square)](https://github.com/NeonD00m/quizzy/releases)
[![License: MIT](https://img.shields.io/github/license/NeonD00m/quizzy)](LICENSE)

A terminal-first, Quizlet-like flashcard tool written in Rust. Import decks (Quizlet JSON, simple JSON/CSV/TSV), study with multiple modes (cards, learn, gamble), and keep learning stats locally in SQLite.

Why Quizzy?
- Local-first: your decks and learning stats stay on your machine.
- Multiple study modes: quick flashcards, an adaptive "learn" quiz mode, and a playful gamble/gauntlet mode.
- Lightweight CLI written in Rust — good for learning and sharing reproducible builds.

Features
- Import from Quizlet JSON (helper included) or load TSV/CSV/JSON text files.
- Save decks and per-card stats locally using SQLite (`rusqlite`).
- Study modes:
  - `learn`: multiple-choice or written quizzes with progress tracking and optional stats commit.
  - `cards`: front/back flashcards with optional shuffle.
  - `gamble` / `gauntlet`: game-like sessions to spice up review.
- Command-driven CLI powered by `clap`.
- Platform-aware storage path via `dirs-next`.
- Detects and offers to recover failed session files on startup.

Quick install
- Download prebuilt binaries from the Releases page:
  https://github.com/NeonD00m/quizzy/releases

- Or build and install locally (requires Rust + cargo):

```quizzy/README.md#L241-255
# Build locally
cargo build --release

# Install locally to your cargo bin
cargo install --path .

# Or run the built binary directly
./target/release/quizzy
```

Quickstart examples
```quizzy/README.md#L256-280
# Create a new deck from a TSV (tab-separated pairs)
quizzy new mydeck examples/tutorial.csv

# Import from a Quizlet link into a saved deck named "My Quizlet" (requires browser)
quizzy import "My Quizlet" "https://quizlet.com/1234567890/some-study-deck-flash-cards/"

# List saved decks (not file-backed decks)
quizzy list

# Study
quizzy learn mydeck            # interactive quiz mode
quizzy cards mydeck --shuffle  # flashcard mode
quizzy gamble mydeck           # play in the "study casino"
```

Tutorial deck
- A small tutorial deck is included in `examples/tutorial.csv`. To load and play it:
```quizzy/README.md#L281-288
quizzy new tutorial examples/tutorial.csv
quizzy learn tutorial
```

Importing from Quizlet
- Quizzy currently supports the Quizlet web JSON format (see `src/core/import.rs`) and the companion `importing.md`.
- If Quizlet’s API changes, the helper will prompt you to save the browser JSON response and import from that file.

Configuration and storage
- Data is stored in an SQLite DB located using `dirs-next` patient conventions (platform-specific).
- The DB schema and storage logic live in `src/core/storage.rs`.
- On startup Quizzy checks for unsaved/failed session files and can attempt to commit them to storage.

Development
- Formatting: `cargo fmt`
- Linting: `cargo clippy`
- Testing: `cargo test`
- Quick full-check (CI parity):
```quizzy/README.md#L289-294
cargo fmt -- --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test --workspace
```

Repository badges
- The CI badge at the top references the Actions workflow at `.github/workflows/ci.yml`.
- The Release badge comes from GitHub releases.
- The License badge reflects the included `LICENSE` (MIT). This project is licensed under the MIT License — see `LICENSE` in the repo root.

Planned/available GitHub Actions
- CI: run `cargo fmt -- --check`, `cargo clippy`, and `cargo test` on push/PR.
- Release: on tag push (e.g. `v0.1.0`) build release binaries for Linux/macOS/Windows and attach zipped artifacts to the GitHub Release.
(See `.github/workflows/ci.yml` and `.github/workflows/release.yml` in the repo for the actual workflows.)

Contributing
- Open issues or PRs - small improvements, importers, or bug fixes are welcome.
- Please run the formatter and clippy before submitting PRs.
- Add small example input files for new importers under `examples/`.

License
- This repository is licensed under the MIT License. See `LICENSE` for details.

Contact
- GitHub: https://github.com/NeonD00m/quizzy
- If you want a feature, open an issue and tag it "feature request".
