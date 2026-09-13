# Bluff Mau-Mau

A two-player bluffing card game with a Rust backend, playable bots, and a persistent
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
Choose from **1,335 bots**: all 1,333 baseline configurations, `Tactical[C0]`, and
`BeliefSearch[S8-H40-conservative]`. Fuzzy search accepts names, strategies, and
parameters such as `B0 N100 C40`. The human sits below the opponent and always
starts. Opponent cards stay hidden, and bot moves happen automatically. New game
starts another game against the same bot; Choose opponent returns to the chooser.
Reloading the page also opens the chooser.

The chooser distinguishes frozen **arena Elo** from the baseline tournament and
**test Elo** from the stronger bots' separate top-ten benchmark. Ratings, win rates,
and W/D/L records are dated **13 September 2026** and never change through browser
play. See the [saved ranking](docs/arena-ranking-2026-09-13.md) and
[benchmark results](docs/solving.md).

Open <http://127.0.0.1:8767/?debug=1> for the separate debug table, with both hands
visible and both players controlled manually. The server holds one shared local
table per mode in memory; tabs in the same mode share that table. Debug games and
normal games cannot change one another, and neither is saved to disk or browser
storage. The existing table layout and card assets are preserved.

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
baseline configuration, complete matches, and the debug and human-play HTTP APIs.

- [Backend API and layout](docs/backend.md)
- [Baseline behavior and private observations](docs/baseline-engines.md)
- [Saved baseline arena ranking — 7,572,168 games](docs/arena-ranking-2026-09-13.md)
- [Game-solving research, self-play CFR, and experimental engines](docs/solving.md)
- [Verification evidence and fixture provenance](AUDIT.md)
- [UI requirements](PRODUCT.md) and [component conventions](DESIGN.md)
