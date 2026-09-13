# Baseline engines

[RULES.md](../RULES.md) is authoritative. These policies are intentionally small:
shared obvious decisions followed by a simple fallback, with no game-tree search.

## API and grid

`engines::Bot` requires `Send + Sync` and these methods:

```rust
fn name(&self) -> String;
fn choose(&self, observation: &Observation, legal_moves: &[Move],
          rng: &mut PythonRandom) -> Result<Move, String>;
```

The supplied list contains every generated legal action for the actor. Policies
return one of those actions; an empty list is an error. Decisions use only the
provided observation, legal moves, and RNG. The generic interface and match runner
do not impose baseline tactics on custom engines.

The legal list excludes declared queens whenever the effective top is an ace,
any seven, or K♠, including after its effect has cleared. This applies to every
bot; a physical queen may still bluff a legal non-queen declaration.

`Baseline::RandomLegal`, `Baseline::HonestFirst`, and
`Baseline::mixed(bluff, no_truth_bluff, challenge)` implement the interface.
The mixed constructor accepts integer percentages from 0 through 100; its three
arguments are `u8`, and values over 100 return an error. Direct enum construction
is checked again when choosing a move. `Baseline::default()` is B10-N10-C20.

Names include parameters and are stable:

- `RandomLegal[uniform]`
- `HonestFirst[B0-N0-C0]`
- `MixedGreedy[B10-N10-C20]`, substituting the configured percentages.

`mixed_grid()` returns all **1,331**
unique mixed configurations with B, N, and C in `0,10,...,100`, ordered with B
outermost and C innermost. Calling it again produces the same order.

## Personalized observations

`Observation` owns the acting player's hand and exposes `player`, `opponent_count`,
`top` (the effective declaration), `chosen_suit`, `phase`, `draw_penalty`,
`skip_pending`, `provisional_winner`, `deck_count`, `pile_count`,
`known_pile_cards`, and `opening_card`. No opponent hand, hidden pile identities,
draw order, game seed, or game RNG state crosses the policy boundary.

`known_pile_cards` is a 32-bit identity set; `known_cards(mask)` returns its cards.
`PileKnowledge` is `[u32; 2]`, one set per player; `EMPTY_KNOWLEDGE` is `[0, 0]`.
`new_knowledge(starting_card)` records the revealed starting identity for both.
`advance_knowledge(before, action, after, knowledge)` updates a genuine transition:

- An actual played card becomes known only to its player.
- Accepting never proves an opponent's declaration truthful.
- Challenging reveals the latest actual card to both players, truthful or false.
- Recycling removes all older discards from both known sets. Their new order is
  shuffled and unavailable. The retained top stays known only to whoever knew it.
- If a challenge reveals and recycles in the same transition, retain its revealed
  top and forget every older recycled card.
- An opponent's later hidden play or declaration cannot restore forgotten knowledge.

`observe(state, knowledge)` projects only the next actor's set. It expects a valid
state and coherent event-based knowledge, without reconstructing history from the
physical pile. `opening_card` remains true across draws until the first card is
played: an opening special has no active effect, but a starting seven/K♠ contributes
when that first play starts a draw penalty.

## Shared obvious decisions

Every baseline applies the following order before its fallback. Recognized wins
outrank last chances, which outrank a provable bluff. This is a small tactical list,
not a claim that every selected move is globally optimal.

1. **Take a recognized guaranteed win.** Accept when already empty, with zero draw
   penalty and either no pending ace or an empty opponent. Play a truthful legal
   final seven/K♠ against an empty opponent, or a truthful final ace against an
   opponent with exactly one card. Accept a response if it permits either final
   play on the following turn; against a pending ace, counter with the final ace
   directly because acceptance would consume the skip.
2. **Challenge as the last chance.** Do so when both hands are empty and a response
   has a draw penalty, or when the opponent is empty and we hold one card against
   their pending ace. Acceptance would lose; a final ace cannot return them.
3. **Challenge a provable bluff.** A declaration matching a card in our hand or our
   known-pile set cannot be the opponent's actual card. Take this challenge unless
   the guaranteed-win rule above applies.
4. **Preserve a return route.** On a turn against an empty opponent, if any legal
   route exists, retain only penalty declarations and aces leaving at least one
   card in hand. The fallback chooses among them. If no route exists, keep legal
   actions available instead of failing.

Bluff declarations count as return routes; lacking an actual seven proves nothing.
A partial draw still returns an empty player if they receive even one card.

## Fallback policies

**RandomLegal** chooses uniformly among actions remaining after the tactical layer.
This includes drawing when it remains legal.

**HonestFirst** chooses a truthful surviving play. If none exists, it draws/skips
when allowed, otherwise bluffs. It accepts uncertain responses.

**MixedGreedy** uses three independent percentages after tactical overrides:

| Parameter | Decision |
| --- | --- |
| B | When truthful and bluff plays both exist, bluff with B%; otherwise play truthfully. |
| N | Without a truthful play, bluff with N%; otherwise draw/skip when available. |
| C | On an uncertain response, challenge with C%; otherwise accept. |

When truth is available, it never voluntarily draws/skips. When a bluff is the
only action family, it bluffs regardless of N; when only truth exists, it chooses
truth regardless of B. There is no separate draw/skip parameter. Probability
checks use `rng.random() < percentage / 100` with a strict boundary. In an
uncertain response, even C=0 and C=100 consume that random draw.

Both greedy policies rank moves within the selected truth/bluff family using the
same coarse preferences, in order: when bluffing, prefer a declared identity still
held; against at most two opponent cards prefer sevens/K♠/aces, otherwise ordinary
declarations; then prefer discarding an ordinary actual card; for a declared queen,
maximize remaining cards of the chosen suit. Ties use the supplied RNG. Ordinary
cards exclude queens, aces, all sevens, and K♠. These are fallback preferences,
not additional forced tactics.

## Matches and evaluation

`run_match([&bot_a, &bot_b], MatchOptions { ..Default::default() })` returns a
`MatchResult`. Options are `seed`, `bot_seeds`, `max_decisions`, `initial_state`,
and `initial_knowledge`; defaults use game seed 0, policy seeds `[0, 1]`, and a
1,000-decision limit. Seeds are `i64`; the cutoff must be positive.

The result contains `final_state`, `knowledge`, `decisions`, and per-seat arrays
`plays`, `bluffs`, `responses`, `challenges`, and `correct_challenges`.
`winner()` reports the final state's winner; `truncated()` means no winner yet.
A cutoff leaves the game unfinished. Only the arena scores it as a draw.

New games give both players knowledge of the revealed starting card. Resuming an
arbitrary initial state without explicit knowledge starts conservatively with
empty sets. The runner acts according to `state.turn`, including empty-hand
responses, and rejects illegal policy output. Game/recycling RNG and each policy's
RNG are independent. A terminal state dispatches no decision; resumed counters
cover only the resumed invocation.

`round_robin` plays each unordered pair over common seeds with seats exchanged.
`evaluate_candidates` tests candidates against a fixed roster; `evaluate_grid`
uses the full mixed grid against RandomLegal and HonestFirst. Both seats receive
the same per-seat policy seeds on each paired deal. Invalid/duplicate names,
empty required rosters or deals, and invalid limits return errors.

`Stats` reports games, wins, losses, truncated games, total/mean decision counts,
plays, bluffs, responses, challenges, correct challenges, bluff rate, challenge
rate, and challenge success rate. Means include cutoffs; rate denominators are
plays, responses, and challenges respectively, and a zero denominator yields `None`.

```sh
./target/release/baseline --deals 100 --seed 0 --bot-seed 10000
./target/release/baseline --grid --deals 10 --seed 1000 --bot-seed 20000
```

The CLI prints JSON statistics. Grid evaluation uses fixed opponents; use the
[arena](arena.md) for matches between grid configurations. Tests validate behavior,
not optimal play or the strength of any parameter choice.
