# Quizzy

A terminal-first, Quizlet-like flashcard tool written in Rust — import decks (Quizlet / JSON / TSV), review with multiple modes (cards, learn, gamble), and store local progress.

Badges: [build] [release] [license]

Quick install
- Download binary from Releases (Linux/macOS/Windows)
- Or:
```sh
# Build locally
cargo build --release
# Install locally to $CARGO_HOME/bin
cargo install --path .
# Or run the built binary directly
./target/release/quizzy
```

Quickstart
$ quizzy new mydeck example.txt      # create a deck
$ quizzy import "My Quizlet" path/to/quizlet.json
$ quizzy learn mydeck
$ quizzy cards mydeck --shuffle
$ quizzy gamble mydeck
