# Bluff Mau-Mau

A two-player bluffing card game with a Rust backend, baseline bots, and a persistent
parallel arena. [RULES.md](RULES.md) defines the game; [rule clarifications](clarification-rules.md)
define the transition conventions. The browser UI lives in `web/`, with shared
components in `design-system/`.

## Build and play

Install Rust, then run from the repository root:

```sh
cargo build --release
git submodule update --init free-playing-cards
./target/release/server
```

Open <http://127.0.0.1:8767/>. `server --port NUMBER` chooses another local port.
The server serves the existing UI and card assets. This is a local shared debug
game: both hands are visible, and game state is held only in memory.

## Run the bots

```sh
./target/release/baseline --deals 100
./target/release/arena --rounds 50 --workers 8
```

The arena includes all **1,333** baseline configurations. Fifty rounds produce
**66,600 games**, 98 or 100 per bot, against a sampled set of opponents.
A complete everyone-against-everyone cycle takes **1,333 rounds** and gives every
pair two games with exchanged seats. Thirty games per pair requires **19,995 rounds**.

Each run creates `runs/<date-and-time>-rust-arena/` with `arena.sqlite3` and
`leaderboard.csv`. Ratings start at **1000, K=20**. The leaderboard ranks by
`(wins + draws/2) / games`, then games, wins, and name; Elo appears alongside W/D/L.
The default 1,000-decision cutoff counts as an arena draw.

Stop with Ctrl-C, inspect, or continue:

```sh
./target/release/arena --status runs/EXACT-DIRECTORY --top 30
./target/release/arena --resume runs/EXACT-DIRECTORY --workers 8
./target/release/arena --resume runs/EXACT-DIRECTORY --rounds 100
```

The round target is total. Each pair and its ratings commit atomically; restarts
replay only uncommitted work. Resume requires the same compiled engine fingerprint.
See [the arena contract](docs/arena.md) for scheduling, persistence, and compatibility.

## Verify

```sh
cargo test --release
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo run --release --example benchmark -- 120
```

The backend, tests, and benchmark run entirely in Rust. Frozen reference data
checks complete ordered move lists, state transitions and random state, every
baseline configuration, complete matches, and the debug HTTP bridge.

- [Backend API and layout](docs/backend.md)
- [Baseline behavior and private observations](docs/baseline-engines.md)
- [Saved baseline arena ranking — 7,572,168 games](docs/arena-ranking-2026-09-13.md)
- [Verification evidence and fixture provenance](AUDIT.md)
- [UI requirements](PRODUCT.md) and [component conventions](DESIGN.md)
