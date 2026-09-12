# Rust backend

This crate replaces the Python runtime for the entire backend. Build from the
repository root with `cargo build --release --manifest-path src-rust/Cargo.toml`.
The executables are `src-rust/target/release/{server,arena,baseline}`.
Verified with Rust 1.98.1 on macOS ARM64. Cargo.lock records dependency versions.

## Layout

- `src/game.rs`: cards, state validation, complete move generation and transitions.
- `src/rng.rs`: CPython-compatible integer-seeded MT19937, including shuffles.
- `src/engines/baseline/`: RandomLegal, HonestFirst, MixedGreedy, shared tactics.
- `src/engines/observation.rs`: private observations and discard knowledge.
- `src/engines/matches.rs`: matches, paired round robins, fixed-opponent grid evaluation.
- `src/engines/arena.rs`: parallel scheduling, SQLite ledger, Elo and CSV export.
- `src/server.rs`, `src/move_explain.rs`: existing debug HTTP API and public explanations.
- `tests/reference.rs`, `tests/fixtures/python-reference.json`: independent frozen parity checks.
- `tests/arena_contract.rs`: independent arena contract tests.

No frontend files were changed. The Rust server serves the existing repository
assets, preserving `/api/state`, `/api/new`, `/api/move`, and `/api/debug/max-hand`.
It remains a localhost-only shared debug game, with both hands visible. Game state
is held in memory. The arena separately persists tournament results.

## Core API

```rust
use bluff_mau_mau::game::{new_game, move_generator, play, Move};

let state = new_game(42, 0)?;
let moves = move_generator(&state)?;
let action = moves.iter().find(|m| matches!(m, Move::Play { .. })).unwrap();
let pending = play(&state, action)?;
assert_eq!(&move_generator(&pending)?[..2], &[Move::Accept, Move::Challenge]);
# Ok::<(), String>(())
```

`Card::parse("7H")` constructs cards; `Suit::{H,D,C,S}` and
`Phase::{Turn,Response,Finished}` are enums. `Move::Play` contains `actual_card`,
`declared_card`, and `chosen_suit`. Other moves are `Draw`, `Skip`, `Accept`, and
`Challenge`. JSON card identities use strings such as `"7H"`; moves have a lowercase
`type` tag. The UI's existing `actual`/`declared` field names are preserved by its
separate HTTP projection.

`new_game` takes an `i64` seed and dealer 0 or 1. `new_game_u64` accepts the full
unsigned range; `new_game_decimal` accepts an arbitrarily large signed decimal
integer. The initial shuffle, dealing order, move ordering, random decisions, and
recycling shuffle match Python. Serialized state includes the complete random
state and supports the original version 2/3 random states and Gaussian cache.

`move_generator` validates the state and returns every legal action, including
bluffs known to be false. `play` returns a new state and rejects illegal actions
without modifying its input. State/card collections are owned Rust values; clones
and branches cannot mutate one another. The match runner validates entry state
and the chosen action, then uses a crate-private in-place transition to avoid
repeating whole-state validation and move generation on every decision.

Public numeric state fields use Rust integer types: players and counts use
`usize`, and `draw_penalty` uses `u32`. All reachable game positions fit; the public
transition rejects arithmetic overflow in manually constructed extreme positions.

## Engines

The generic policy interface is `engines::Bot`:

```rust
fn choose(&self, observation: &Observation, legal_moves: &[Move],
          rng: &mut PythonRandom) -> Result<Move, String>;
```

`Baseline::RandomLegal`, `Baseline::HonestFirst`, and
`Baseline::mixed(bluff, no_truth_bluff, challenge)` implement it. The mixed
constructor validates integer percentages 0–100. `mixed_grid()` returns all 1,331
configurations at ten-point intervals, in B/N/C order. Names retain their parameters,
for example `MixedGreedy[B0-N80-C30]`.

B chooses bluff versus truth when both are available. N chooses bluff versus
Draw/Skip when truth is unavailable. C chooses challenge versus acceptance on
uncertain responses. The existing obvious-move rules run inside each baseline
before its fallback strategy; custom engines are not forced to follow them.
Greedy card preferences and tie-breaking use the same ordering and RNG calls as
the Python policies. See [the original behavior specification](../docs/baseline-engines.md).

`Observation` exposes only the acting hand, public state, opponent hand count,
and that player's provable discard identities. Knowledge is a 32-bit card set;
`known_cards(mask)` converts it to cards. Own plays update only their owner's set;
challenges reveal the actual top to both; recycled cards leave both sets. No
shuffled order or unknown opponent card crosses the bot interface.

`run_match([&bot_a, &bot_b], MatchOptions { ..Default::default() })` returns the
final state, private knowledge, decision count, and per-player action counters.
`MatchOptions` supports initial state/knowledge, separate policy seeds, and a
cutoff. A cutoff leaves the game unfinished; only the arena scores it as a draw.
`round_robin`, `evaluate_candidates`, and `evaluate_grid` provide paired evaluations.

```sh
./src-rust/target/release/baseline --deals 100 --seed 0 --bot-seed 10000
./src-rust/target/release/baseline --grid --deals 10 --seed 1000 --bot-seed 20000
```

The grid evaluation tests each candidate against fixed RandomLegal/HonestFirst
opponents. Use the arena for matches between grid configurations.

## Persistence and parallelism

The arena preserves the seeded circle schedule and exchanged-seat pairs from
[the original arena contract](../docs/arena.md). Worker threads execute a bounded
queue; the coordinator commits results in schedule order. Each transaction stores
both games and their W/D/L and Elo updates. SQLite WAL and synchronous FULL protect
committed results. A file lock rejects a second writer while read-only status
remains available. CSV exports are atomically replaced every five seconds and at
exit; the database is authoritative.

Ctrl-C/SIGTERM finishes the current earliest pair and stops. SIGKILL loses only
uncommitted work; restarting uses its original seeds. Worker count can change.
Threads share the process lifetime, so no worker processes remain after a kill.

Rust runs use format 2 and a fingerprint of compiled core/policy/arena sources,
Cargo manifest/lockfile, compiler, and platform. Editing sources cannot relabel an
already-built binary. Rebuilds with changed behavior are rejected on resume.
Old Python runs are readable but cannot be extended with the Rust implementation.

## Verification record

The reference JSON was frozen directly from the original Python implementation
before its author inspected the Rust implementation. It includes source hashes
and records full-state comparisons including RNG. Its SHA-256 is
`afaad3e795b6574387aa8f0853d12b7c195ad01937376a4961efd189359e7b34`.

The nine parity tests passed on their first execution:

- 140 initial deals, including negative and 1,024-bit seeds.
- 2,225 positions and 76,077 transitions, preserving complete ordered legal moves.
- 7,609 trace decisions, including private knowledge after reveals and recycling.
- All 1,333 bots in five public positions: identical chosen move and consumed RNG.
- 45 full matches: identical final state, knowledge, and every action counter.
- Debug-game traces: identical JSON views, move IDs, history, and explanations.
- Paired evaluation statistics and seeded arena schedules.

A separate agent authored eight arena tests from the public contract and original
Python tests, without reading the Rust implementation or existing Rust tests.
Frozen SHA-256 before execution:
`aeac9d09e019a4cde77adf7d9e2a417b3074f1f7087fe3a2b7d37827d09e1ca8`.
All eight passed on their first execution. Subsequent Rust 2024 formatting and
one equivalent Clippy iterator suggestion changed no assertions; the final file
SHA-256 is `539041813bdddae27635cce123ca331d17b670864be773f3686d27a4789bb678`.
This was procedural separation, not an OS-enforced isolation claim.

Real-process tests also compared uninterrupted versus resumed 800-game arenas:
live status, second-writer rejection, SIGTERM, SIGKILL, and changed worker counts
all preserved the exact ledger, ratings, and CSV. The original HTTP boundary tests
also passed against the actual Rust server, without frontend changes.

`examples/benchmark.rs` is a reproducible single-thread workload. In three runs
of the same 120 games (14,874 decisions; result checksum 26,243), median Python
runtime was **6.5288 s**, versus **0.05540 s** for optimized Rust: approximately
**118× faster** on this machine. This measures match execution, not SQLite writes.
The original Python benchmark ran only during migration; the retained benchmark
and tests run entirely in Rust.

The final release also reran the original 50-round, 1,333-bot arena: **66,600 games
in 11.12 seconds with eight workers**, including SQLite persistence and CSV export.
Every ledger row matched the Python run (opponents, seats, all seeds, result, and
decision count). All 1,333 W/D/L records matched. The maximum Elo difference was
2.2737367544323206e-13 from floating-point exponentiation rounding; displayed Elo
and leaderboard order are identical.

Final comparison results:
[`runs/20260913-012802-340092-rust-arena/leaderboard.csv`](../runs/20260913-012802-340092-rust-arena/leaderboard.csv).
The original reference run remains untouched at
`runs/20260913-003315-156654-arena/`.

The final audit found and fixed an overflow in the trusted path for manually
imported extreme penalties. The shared transition now checks arithmetic before
mutation; challenge-history counts use a wider integer. A regression covers both
paths. Final verification: **25 tests pass**, `cargo fmt --check` and
`cargo clippy --all-targets -- -D warnings` pass. Rust arena subprocess tests also
run with a PATH containing no Python or other external runtime.
