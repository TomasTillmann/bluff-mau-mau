# Bluff Mau-Mau

The backend, move generator, baseline bots, match runner, and persistent arena are
implemented in **Rust**, under [`src-rust/`](src-rust/). The existing UI and card
assets are unchanged. [RULES.md](RULES.md) remains authoritative.

## Build and play

Install Rust, then run from this repository:

```sh
cargo build --release --manifest-path src-rust/Cargo.toml
git submodule update --init free-playing-cards
./src-rust/target/release/server
```

Open <http://127.0.0.1:8767/>. Use `server --port NUMBER` for another local port.
The server serves the original `web/`, `design-system/`, and `free-playing-cards/`
files and preserves their HTTP API. No Python is required to build, test, or run
the Rust backend. A fresh Rust installation may require `source "$HOME/.cargo/env"`
in the current shell.

## Run the bots

```sh
./src-rust/target/release/baseline --deals 100
./src-rust/target/release/arena --rounds 50 --workers 8
```

The arena includes all **1,333** baseline configurations. Fifty rounds produce
**66,600 games**, approximately 100 per bot, against a sampled set of opponents.
A complete everyone-against-everyone cycle takes **1,333 rounds** and gives every
pair two games with exchanged seats. Thirty games per pair requires **19,995
rounds**.

Each run creates `runs/<date-and-time>-rust-arena/` with `arena.sqlite3` and
`leaderboard.csv`. Ratings begin at **1000, K=20**. The leaderboard ranks by
`(wins + draws/2) / games`, followed by games, wins, and name; Elo is shown alongside
W/D/L. The 1,000-decision cutoff counts as an arena draw.

Stop with Ctrl-C, inspect, or continue:

```sh
./src-rust/target/release/arena --status runs/EXACT-DIRECTORY --top 30
./src-rust/target/release/arena --resume runs/EXACT-DIRECTORY --workers 8
./src-rust/target/release/arena --resume runs/EXACT-DIRECTORY --rounds 100
```

The round target is total. Each pair and its ratings commit atomically; restarts
replay only uncommitted work. Old Python results remain readable with `--status`,
but Rust deliberately refuses to continue a Python run or a run made with a
different engine/compiler fingerprint.

## Verification

```sh
cargo test --release --manifest-path src-rust/Cargo.toml
cargo clippy --manifest-path src-rust/Cargo.toml --all-targets -- -D warnings
cargo run --release --manifest-path src-rust/Cargo.toml --example benchmark -- 120
```

Rust tests use frozen outputs from the original Python implementation, including
complete ordered move lists, state transitions, random state, all baseline
configurations, complete matches, and the debug HTTP bridge. The arena has a
separate suite authored without access to its Rust implementation.

See [`src-rust/README.md`](src-rust/README.md) for the Rust API and verification
record. Original Python files remain as the unchanged historical reference;
none are invoked by the Rust backend or Rust tests. The prior behavioral
specifications remain in [`docs/`](docs/).
