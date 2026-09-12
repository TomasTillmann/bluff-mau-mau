# Bluff Mau-Mau engine

Python 3.10+, standard library only. The game follows [RULES.md](RULES.md).

```python
from bluff_mau_mau import Accept, Challenge, MoveGenerator, NewGame, Play, PlayCard

state = NewGame(seed=42, dealer=0)
moves = MoveGenerator(state)
move = next(move for move in moves if isinstance(move, PlayCard))
pending = Play(state, move)
assert MoveGenerator(pending) == [Accept(), Challenge()]
next_state = Play(pending, Accept())
assert state == NewGame(seed=42, dealer=0)  # The input was not modified.
```

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
| `Accept()` | Accept the latest declaration. |
| `Challenge()` | Reveal and challenge the latest declaration. |
| `Draw()` | Take the pending draw penalty, or one card when there is none, and end the turn. |
| `Skip()` | Consume an ace's skip effect and end the turn. |

After `PlayCard`, the opponent acts in `"response"` phase and chooses only
`Accept()` or `Challenge()`. Acceptance generally leaves that same opponent
in `"turn"` phase. Forced effects on an empty hand are resolved as part of
acceptance; see [clarification-rules.md](clarification-rules.md).

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
