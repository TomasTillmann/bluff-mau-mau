# Backend API

The repository is a single Cargo crate, `bluff_mau_mau`. `Cargo.lock` fixes dependency
versions. Build and run commands are in the [root README](../README.md).

## Layout

- `src/game.rs`: cards, state validation, complete move generation, and transitions.
- `src/rng.rs`: integer-seeded MT19937 and deterministic shuffle support.
- `src/engines/baseline/`: RandomLegal, HonestFirst, MixedGreedy, and shared tactics.
- `src/engines/observation.rs`: private observations and discard knowledge.
- `src/engines/matches.rs`: matches, paired round robins, and grid evaluation.
- `src/engines/arena.rs`: parallel scheduling, SQLite ledger, Elo, and CSV export.
- `src/engines/tactical.rs`, `search.rs`: experimental stronger playing policies.
- `src/engines/cfr.rs`, `solving.rs`: self-play CFR and finite hidden-information game trees.
- `src/engines/mccfr.rs`, `training_store.rs`: sampled self-play and durable training checkpoints.
- `src/server.rs`, `src/move_explain.rs`: human-play/debug HTTP APIs and public explanations.
- `src/bot_catalog.rs`: runnable snapshot bots and their frozen rating metadata.
- `src/bin/`: `server`, `baseline`, and `arena` command entry points.
- `tests/`: Rust integration tests and their frozen fixtures.
- `examples/benchmark.rs`: reproducible single-thread match benchmark.
- `examples/engine_lab.rs`, `solve.rs`, `train.rs`: isolated match evaluations, exact solving,
  and resumable sampled [solver experiments](solving.md).
- `examples/evaluate_trained.rs`: held-out full-history policy matches and diagnostic performance Elo.

## Game state and moves

```rust
use bluff_mau_mau::game::{new_game, move_generator, play, Move};

fn main() -> Result<(), String> {
    let state = new_game(42, 0)?;
    let moves = move_generator(&state)?;
    let action = moves.iter().find(|m| matches!(m, Move::Play { .. })).unwrap();
    let pending = play(&state, action)?;
    assert_eq!(&move_generator(&pending)?[..2], &[Move::Accept, Move::Challenge]);
    Ok(())
}
```

`Card::parse("7H")` constructs cards. `Suit::{H,D,C,S}` and
`Phase::{Turn,Response,Finished}` are enums. `Move::Play` contains `actual_card`,
`declared_card`, and `chosen_suit`; other moves are `Draw`, `Skip`, `Accept`, and
`Challenge`. JSON cards use strings such as `"7H"`; moves have a lowercase `type` tag.

`new_game` accepts an `i64` seed and dealer 0 or 1. `new_game_u64` accepts the full
unsigned range; `new_game_decimal` accepts an arbitrarily large signed decimal
integer. The initial shuffle, dealing order, and recycling shuffle retain the
established deterministic behavior. Legal moves are ordered deterministically;
rule corrections can change that list and subsequent seeded bot choices.
Serialized state contains the complete RNG state, including its Gaussian cache.
The Rust `PythonRandom` type preserves the original CPython integer-seeded MT19937
algorithm and version 2/3 state encoding; it does not invoke a Python runtime.

`move_generator` validates the state and returns every legal action, including
bluffs that a player can prove false. `play` rejects illegal actions and returns
a new state without modifying its input. State collections are owned Rust values;
clones and branches cannot mutate one another. Players and counts use `usize`;
`draw_penalty` uses `u32`. Reachable positions fit these types; extreme manually
constructed penalties return an error on arithmetic overflow. A declared queen
is forbidden whenever the effective top is an ace, any seven, or K♠, regardless
of pending effects; an actual queen may still bluff a legal non-queen declaration.

The match runner validates its entry state and each chosen action, then uses a
crate-private in-place transition. This avoids repeating complete state validation
and move generation on every decision. See [baseline engines](baseline-engines.md)
for policy and evaluation APIs.

## HTTP bridge

The localhost server serves the existing repository assets. `/` opens the bot
chooser; `/?debug=1` opens the separate debug table. Each mode owns one shared
in-memory table, without per-browser sessions. Browser play does not write games
or ratings to disk; tournament persistence belongs to the [arena](arena.md).

| Method and route | Contract |
| --- | --- |
| `GET /api/bots` | `{bots, rating_date}`; 1,335 runnable bots and frozen records dated `2026-09-13`. |
| `POST /api/play/new` | Requires only `{bot_id}`; deals a new human-versus-bot game with human player 0 moving first. |
| `GET /api/play/state` | Returns the current private human view; 404 until a bot has been selected. |
| `POST /api/play/move` | Requires only `{version, move_id}`; applies the human action and automatic bot actions until the next human decision or winner. |
| `GET /api/state` | Reads the separate debug game, with both hands visible. |
| `POST /api/new` | Starts a debug game; supports the existing optional debug seed. |
| `POST /api/move` | Applies an action to the debug game. |
| `POST /api/debug/max-hand` | Applies a hand-size preset only to the debug game. |

Normal-play views include `mode: "play"`, `human_player: 0`, bot metadata, public
state, history, move explanations, and `hands: [human_hand, []]`. `hand_counts`
provides both counts. Only human legal actions are exposed; play actions retain
the UI's `actual`/`declared` fields and move IDs. The opponent's actual played
card is absent from public history until a challenge reveals it. Deck order,
hidden cards, RNG state, and bot legal actions are never part of this projection.

Move versions increase across actions and new normal games. Stale versions return
409. Unknown bot IDs, client-supplied normal-game seeds, extra fields, and malformed
move payloads are rejected. A failed request commits no cards, knowledge, history,
or random state. Debug operations leave the normal table unchanged.

Catalog rows contain `id`, `name`, `family`, `elo`, `elo_kind`, `games`, `wins`,
`draws`, `losses`, `score_rate`, and `description`. `score_rate` is
`(wins + draws / 2) / games`. The 1,333 baselines use `elo_kind: "arena"` from
[the saved Markdown ranking](arena-ranking-2026-09-13.md). `Tactical[C0]` and
`BeliefSearch[S8-H40-conservative]` use `elo_kind: "performance"`, displayed as
“test Elo”, from their separate 1,000-game top-ten benchmarks. These rating sources
are distinct and remain fixed during browser play. Both predate the queen rule
correction; they are historical records, not evaluations under the current rules.

Only engines implementing the snapshot `Bot` API appear in this catalog.
Full-history trained CFR policies use the [solver/evaluation API](solving.md) and
are not exposed through that snapshot interface or the bot chooser.
