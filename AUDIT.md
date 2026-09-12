# Adversarial audit record

The implementation is checked against `RULES.md`. Auditors inspect and probe;
separate agents fix reproduced findings. A clean audit round after the fixes is
required before completion.

## Round 1

| Finding | Evidence | Status |
| --- | --- | --- |
| A1: mutable text aliases violate immutable-state guarantees | `UserString("turn")` is accepted as a phase; changing its `.data` after `Play(state, Draw())` changes the returned phase. The same issue affects card rank/suit and continuing suits. | Fixed by a separate agent: primitive text validation; regression cases added. |
| A2: malformed RNG numerics leak an unexpected exception | A `Decimal("sNaN")` RNG version or legacy internal word raises `decimal.InvalidOperation` rather than the public `ValueError`. | Fixed: validate integer version/words before invoking the RNG; regression cases added. |
| A3: inconsistent terminal player is accepted | Replacing a finished state's `turn` with the other player still makes `MoveGenerator` return `[]`, despite the documented `turn == winner` invariant. | Fixed: reject terminal player mismatch; regressions cover both players and both APIs. |

Independent gameplay auditor: no gameplay findings in 49,152 response cases,
80 accumulated-penalty/revealed-special probes, and multi-turn return/ace traces
for both players. The API auditor reported A1–A3. The independent transition
auditor found no additional bugs across 10,014 legal decisions, 8,352 draw
transitions (including 1,002 partial penalties), and 232 draws under unrestricted
queens. These counts describe independent audit probes, beyond the persistent
test suite.

## Round 2

| Finding | Evidence | Status |
| --- | --- | --- |
| A4: an accepted `Card` subclass has inconsistent identity | An otherwise empty subclass used as effective `7S` removes the legal `KS` counter; using it as a truthful response declaration reverses the challenge outcome. Python dataclass equality distinguishes the concrete classes, while the top-card boundary accepted subclasses. | Fixed by a new agent: consistently require concrete `Card` values at the state boundary; regressions reproduced both errors for both players before the fix. |

The fresh API auditor verified A1–A3 and found A4 after 7,548 ordinary boundary
probes, 69 RNG payload probes, and 53 transitions with immutable tuple wrappers.
The fresh gameplay auditor reported no findings across 42,173 independently
checked transitions, including 322 wins. The transition auditor reported no
findings across 117,882 generated moves, 49,152 response positions, 2,268 draw
positions, and 2,048 both-empty responses. All auditors were read-only.

## Round 3

| Finding | Evidence | Status |
| --- | --- | --- |
| A5: equality-only move admission accepts non-move/non-card wildcards | `Play(state, unittest.mock.ANY)` skips an ordinary turn. An `ANY` actual card removes the whole hand and enters the pile; an `ANY` declaration leaks `AttributeError`. Move and card field types were not checked before equality against generated moves. | Fixed by a new agent: concrete move and card admission before equality; regressions added across decision phases. |
| A6: spoofed state/container types pass `isinstance` | A standard-library `Mock(spec=GameState)` with copied fields yields moves but fails inside `Play`; mocked tuple zones leak `TypeError`. | Fixed: concrete state records and genuine tuple type checks, retaining real tuple subclasses; regressions added. |

The round-3 snapshot is
`bb2431d44389c8c2d3bda197aedca24a26bbbb7bf28713d41303bff58a86523b`.
All 69 persistent methods and the README example pass on this snapshot. A
focused standard-library trace exercises all 219 executable source lines
(excluding the compiler's synthetic line 0); A5 demonstrates that line coverage
alone does not establish correctness. The API auditor otherwise passed 28,288
boundary calls and verified A1–A4. The gameplay auditor reported no findings in
49,210 transitions, 1,280 targeted return-priority resets, and 512 five-ace chains.
The transition auditor reported no findings across 323,310 generated moves,
7,848 draws, 30,841 history transitions, and 352 revealed-queen cases. Every
auditor inspected the same engine hash above without editing files.

## Round 4

Three fresh, read-only auditors examined the frozen engine:
`23e2f42634c7830021ce54e24c17b3d9d0003e8ad735ddf0a60b97173d6fd436`.
All three returned **No findings** and independently reported that exact hash.

| Auditor | Final result | Independent checks |
| --- | --- | --- |
| Gameplay | No findings | 78,772 transitions; 214,796 generated moves across 3,888 positions; both players, every declaration/actual identity, penalty stacks, ace chains, and victory-priority resets. |
| API | No findings | All six regressions verified; 70,348 invalid-input calls, 2,707 legal transitions with genuine tuple subclasses, and 5,298 serialization/replay transitions. |
| Transitions | No findings | 112,477 independently predicted transitions, including 27,250 generated actions, scarce draws, partial penalties, actual-card recycling, queen freedom, and sibling replay. |

The engine was unchanged throughout this round. All six findings from the
earlier rounds are fixed and covered by persistent regressions. No unresolved
findings remain from the final audit round.

## Round 5

| Finding | Evidence | Status |
| --- | --- | --- |
| A7: contradictory victory priority passes validation when both hands are empty | In a response with both hands empty, replacing `provisional_winner` with `1 - turn` is accepted by both public APIs, although the responder necessarily emptied first. Normal game transitions do not produce this state. | Fixed: reject the inconsistent claim with `ValueError` in shared state validation. |

The regression covers both players and final ordinary cards, aces, and sevens.
It reproduced 18 missing-`ValueError` failures before the fix, checks rejection
by `MoveGenerator` and `Play` with either legal response, and preserves valid
acceptance/challenge outcomes. All 74 test methods pass after the fix.

## Verification

Run all persistent checks with `python3 -B -m unittest -v`.

- `test_bluff_mau_mau.py`: rule examples and 2,262 parameterized rule cases,
  including final cards, return priority, ace chains, and scarce penalty draws.
- `test_api_boundaries.py`: malformed public inputs, immutability, replay,
  serialization, terminal refusal, and RNG isolation.
- `test_state_properties.py`: complete normal/effect move sets, 1,408
  actual/declaration/queen-choice response combinations, player symmetry, and
  bounded seeded action sequences.

On the round-4 snapshot, all 73 test methods and 11,873 counted parameterized
subtests pass; additional assertions run inside unwrapped loops. The README
example also passes. A focused standard-library trace across 61 methods covers
all 224 executable source lines. Verification ran on Python 3.14.7 using only
the standard library.
