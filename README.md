# Bluff Mau-Mau engine

Python 3.10+, standard library only. The game follows [RULES.md](RULES.md).

```python
from bluff_mau_mau import Accept, Challenge, MoveGenerator, NewGame, Play, PlayCard

state = NewGame(seed=42, dealer=0)
moves = MoveGenerator(state)
move = next(move for move in moves if isinstance(move, PlayCard))
pending = Play(state, move)
assert MoveGenerator(pending)[:2] == [Accept(), Challenge()]
next_state = Play(pending, Accept())
assert state == NewGame(seed=42, dealer=0)  # The input was not modified.
```

## Playable debug table

```sh
git submodule update --init free-playing-cards
python3 -B server.py
```

Open <http://127.0.0.1:8767/> in Chrome. Use `--port NUMBER` to choose another
local port. The server uses only Python's standard library and holds one shared
game only in memory. Every page load or reload replaces it with a fresh deal,
clearing moves, selections, and history. No game data is saved to browser storage
or disk; reloading any tab resets the shared debug game.

Both hands stay visible in fixed seats. Make the active player's decisions,
including playing any actual card as a legal declaration, selecting a queen's
continuing suit, drawing, passing, and challenging. Declaration tiles show rank
and suit; unavailable declarations are dimmed and disabled. Play and Play as
itself send only an exact move offered by the engine. Empty hands stay provisional until the engine
confirms a winner. New game deals a fresh game with Player 1 (the bottom seat) starting. This iteration is a local debug
table; hidden-hand play and multiplayer are not included.

Click the draw stack to draw, or the facedown discard to challenge while a
response is pending. Selecting cards, declarations, or suits leaves that response
open. Submitting Play or Play as itself, or drawing from the stack, implicitly
accepts the previous claim in one engine move. When an ace or empty hand prevents
drawing, the draw stack resolves the pass or acceptance instead; its tooltip
describes the action. An accepted ace skips automatically, without drawing.

MoveExplain keeps the last public result directly below Player 1’s hand, always
using Player 1’s perspective ("you"), even when controlling Player 2. Draw counts
come from actual hand-count changes, including shortages and recycled cards.
The result survives selections and failed moves; a reload, new game, or debug
preset clears it. Hidden card identities are never used in its explanations.

Integration checks cover 30 playable cards and the 31-card finished maximum.
A discard must always remain, so 32 cards cannot fit in one hand.
The 31-card fixture uses a real engine draw that confirms the empty opponent’s win.

The shared visual components remain in `design-system/`; open
<http://127.0.0.1:8767/design-system/index.html> for the component study. The supplied
card deck and its CC0 license are preserved in `free-playing-cards/`.
Run `python3 -B -m unittest test_move_explain test_server` for explanation,
bridge, and HTTP boundary checks. The blind MoveExplain test author worked from
the public contract, rules, and engine without access to the implementation.
Run `node test_truthful_play.cjs` for the truthful-play shortcut checks.
For the small Chrome sanity suite, run `npm --prefix tests/integration ci` once,
then `npm --prefix tests/integration test`. It starts an isolated local server
and checks pile hover/clicks, bluff calls for both players, stable panel positions,
and maximum-hand access. See [tests/integration/AGENTS.md](tests/integration/AGENTS.md)
for the deliberately lightweight testing scope.

## API

- `MoveGenerator(state) -> list[Move]`: all legal actions for `state.turn`, in
  stable order. No duplicates, ranking, or bluff pruning. Returns `[]` after a win.
- `Play(state, move) -> GameState`: applies one decision and returns an immutable
  state. Invalid states or unavailable moves raise `ValueError`.
- `NewGame(seed=0, dealer=0) -> GameState`: shuffles the 32 cards, deals five
  alternately to each player starting with the non-dealer, reveals the next
  card, and applies its starting effect. Supply another seed for another deal.

Use the provided concrete `GameState` and move classes. Unsupported or spoofed
record types are rejected before comparing legal moves. Genuine immutable tuple
subclasses, including named tuples, are accepted as card containers.

Players are `0` and `1`. Construct cards with `Card(rank, suit)`: ranks are
`"7", "8", "9", "10", "J", "Q", "K", "A"`; suits are
`"H"` (hearts), `"D"` (diamonds), `"C"` (clubs), `"S"` (spades).
Use concrete `Card` instances for card values; subclasses are unsupported.

The move types are frozen dataclasses:

| Move | Meaning |
| --- | --- |
| `PlayCard(actual_card, declared_card, chosen_suit=None)` | Play a card from the acting hand under a legal declaration. A declared queen requires one of the four suit codes, even when bluffing; other declarations require `None`. |
| `Accept()` | Accept the latest declaration, immediately consuming an ace's skip. |
| `Challenge()` | Reveal and challenge the latest declaration. |
| `Draw()` | Take the pending draw penalty, or one card when there is none, and end the turn. |
| `Skip()` | Consume an ace's skip effect and end the turn. |

After `PlayCard`, the opponent acts in `"response"` phase. `Accept()` and
`Challenge()` remain first in the legal list. A nonempty responder can also play
or draw directly to implicitly accept the prior claim. Against an ace, only an
ace or queen may be played; explicit acceptance consumes the skip immediately.
Queens cancel any pending skip or draw penalty. Forced effects on an empty hand
are resolved during acceptance; see [clarification-rules.md](clarification-rules.md).

## State and replay

`GameState` contains these fields:

| Fields | Meaning |
| --- | --- |
| `deck`, `pile`, `hands` | Tuples of actual cards. `deck[0]` is drawn next; `pile[-1]` is the newest discard. `hands` is a pair of tuples. Together they must contain each of the 32 cards exactly once. |
| `turn`, `phase` | Player making the next decision, and `"turn"`, `"response"`, or `"finished"`. In a finished state, `turn` identifies the winner. |
| `top`, `chosen_suit` | Effective declared/revealed top identity and a queen's continuing suit. A queen with `None` permits unrestricted declarations until the next play. |
| `draw_penalty`, `skip_pending` | Accumulated draw count and ace effect. During a response these already record the latest declaration's contribution, but no effect is applied until resolution. |
| `provisional_winner`, `winner` | Player with the earliest still-empty hand, and confirmed winner; otherwise `None`. Terminal states clear the provisional claim and pending effects. |
| `rng_state` | Immutable state of a local `random.Random`, used and advanced when recycling actual discards. Defaults to the state from seed `0` for manually constructed positions. |

All state needed for a transition lives in `GameState`. No global random state is
used or changed. Replaying a state and move on the same Python version produces
the same result. Cards and moves are immutable too. Full engine states contain
hidden card identities; future player observations should expose only what that
player is allowed to see.

## Verification

Run from this directory:

```sh
python3 -B -m unittest -v
```

The three test modules cover the rule examples and thousands of parameterized
cases, complete move enumeration, turn and victory transitions, challenges,
recycling, deterministic replay, invalid inputs, and card conservation. Seeded
action sequences exercise transitions without providing a gameplay bot.

Independent adversarial audit findings and their resolution are recorded in
[AUDIT.md](AUDIT.md).

## Baseline engines

The bots live in `src/engines/baseline/`; shared player observations and match
facilities live in `src/engines/`. The trusted rules engine remains unchanged.
All imports work from the repository root without installing dependencies.

```python
from random import Random
from bluff_mau_mau import MoveGenerator, NewGame, Play
from src.engines import new_knowledge, observe, advance_knowledge
from src.engines.baseline import HonestFirst, MixedGreedy, RandomLegal, mixed_grid

state = NewGame(seed=42)
knowledge = new_knowledge(state.top)
bot = MixedGreedy(bluff=10, no_truth_bluff=40, challenge=20)
moves = tuple(MoveGenerator(state))
move = bot(observe(state, knowledge), moves, Random(10000))
next_state = Play(state, move)
knowledge = advance_knowledge(state, move, next_state, knowledge)
```

Every bot instance has the same decision interface:
`bot(observation, legal_moves, rng) -> Move`. Configuration is bound at
construction. The common interface does not impose a strategy on other engines;
the obvious-move checks live inside these baselines.

| Name | Fallback after obvious-move checks |
| --- | --- |
| `RandomLegal[uniform]` | Uniform over the remaining legal choices. |
| `HonestFirst[B0-N0-C0]` | Play truthfully if possible; otherwise draw/skip, or bluff when required to continue. Accept uncertain declarations. |
| `MixedGreedy[B10-N40-C20]` | B% bluff when truth is available; N% bluff when truth is unavailable; C% challenge on uncertain responses. |

MixedGreedy never voluntarily draws/skips when a truthful surviving play exists.
When truth is unavailable, drawing/skipping is the alternative to bluffing.
Forced wins, last chances, provable bluffs, and return-attempt restrictions precede
these probabilities. The two greedy policies share simple card preferences and
random tie breaking. A provable bluff can use either the bot's own hand or its
personal knowledge of cards still in the discard pile.

Each player knows their own actual discards and cards revealed to both players,
including the starting card. Acceptance never proves a declaration. Recycling
shuffles older discards into the draw pile and removes them from both known-pile
sets; the top stays known only to players who already knew it. The observation
never exposes hidden opponent cards, unknown discards, deck order, or RNG state.

`mixed_grid()` enumerates all **1,331** configurations with B, N, C independently
at 0, 10, ..., 100 percent, with each configuration encoded in its `.name`.
Direct construction also permits integer percentages between the grid points.
The full behavior/API specification is in [docs/baseline-engines.md](docs/baseline-engines.md).

### Matches and grid evaluation

```sh
python3 -B -m src.engines.baseline --deals 100 --seed 0 --bot-seed 10000
python3 -B -m src.engines.baseline --grid --deals 10 --seed 1000 --bot-seed 20000
```

The first command compares the three defaults in paired seats. The second
compares every MixedGreedy configuration against the fixed RandomLegal and
HonestFirst opponents in paired seats; it does not run every grid candidate
against every other candidate. Grid evaluation can take substantially longer.
JSON results use the full parameterized names and report wins, losses,
truncations, mean decision count, bluff/challenge frequency, and challenge success.
A cutoff is unfinished, never an in-game draw. Means include cutoffs; unobserved
rates are `null`.

For custom comparisons, import `run_match`, `round_robin`, and `evaluate_grid`
from `src.engines.matches`. The runner follows `state.turn`, maintains each
player's pile knowledge, and separates game randomness from each policy's RNG.
A resumed `initial_state` may provide `initial_knowledge`; omitted knowledge is
conservatively empty because reveal history cannot be reconstructed from a
snapshot. Reuse deal seeds and bot seeds to reproduce a run. Keep final evaluation
deals separate from tuning deals. Grid enumeration/evaluation does not select an
optimal configuration automatically.

### Independent tests

New baseline tests live in `tests/baseline/` and run with the existing command:

```sh
python3 -B -m unittest discover -s tests -v
python3 -B -m unittest -v
```

The baseline test authors received only the public contract, rules, and trusted
card-game engine in isolated directories. They authored and froze their tests
without reading or importing the old or new bot implementation. The integration
run checks those tests against the code afterward; original game and web tests
remain separate. Test provenance and audit evidence are recorded in
[docs/baseline-verification.md](docs/baseline-verification.md).
