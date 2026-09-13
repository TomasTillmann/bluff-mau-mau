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
- `src/server.rs`, `src/move_explain.rs`: debug HTTP API and public explanations.
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
integer. The initial shuffle, dealing order, ordered legal moves, random bot
choices, and recycling shuffle retain the established deterministic behavior.
Serialized state contains the complete RNG state, including its Gaussian cache.
The Rust `PythonRandom` type preserves the original CPython integer-seeded MT19937
algorithm and version 2/3 state encoding; it does not invoke a Python runtime.

`move_generator` validates the state and returns every legal action, including
bluffs that a player can prove false. `play` rejects illegal actions and returns
a new state without modifying its input. State collections are owned Rust values;
clones and branches cannot mutate one another. Players and counts use `usize`;
`draw_penalty` uses `u32`. Reachable positions fit these types; extreme manually
constructed penalties return an error on arithmetic overflow.

The match runner validates its entry state and each chosen action, then uses a
crate-private in-place transition. This avoids repeating complete state validation
and move generation on every decision. See [baseline engines](baseline-engines.md)
for policy and evaluation APIs.

## HTTP bridge

The localhost server serves the existing repository assets and preserves:

- `GET /api/state`
- `POST /api/new`
- `POST /api/move`
- `POST /api/debug/max-hand`

The HTTP projection retains the UI's `actual`/`declared` move fields, move IDs,
history, and public move explanations. Both hands are intentionally visible in
this shared debug game. State lives in memory; tournament persistence belongs to
the separate [arena](arena.md).
