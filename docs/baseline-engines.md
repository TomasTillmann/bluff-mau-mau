# Baseline engine contract

This is a behavior specification agreed before the new implementation and blind
unit tests. Python 3.10+, standard library only. RULES.md remains authoritative.
Do not modify the trusted game engine or the unrelated web app.

## Layout and public API

Implementation lives in src/engines/baseline; shared observation and match
facilities live in src/engines. Root imports work without installation:

- from src.engines import Observation, Bot, PileKnowledge, new_knowledge, advance_knowledge, observe
- from src.engines.baseline import RandomLegal, HonestFirst, MixedGreedy, mixed_grid
- from src.engines.matches import run_match, round_robin, evaluate_grid

RandomLegal() and HonestFirst() create callable bots.
MixedGreedy(bluff=10, no_truth_bluff=10, challenge=20) creates a callable bot.
All instances have the same decision signature:
(observation: Observation, legal_moves: tuple[Move, ...], rng: Random) -> Move.
They return an action from the supplied COMPLETE generated tuple. Empty moves
raise ValueError. Inputs are immutable and decisions use only the supplied RNG.
The interface imposes no tactics on other custom engines: baseline tactics are
inside these baseline policies, never imposed by the runner.

Each MixedGreedy argument is an integer percentage 0..100 (bool, noninteger,
negative, and >100 are rejected with ValueError). Values need not be multiples
of ten for direct construction; the grid uses only 0,10,...,100.
Public configuration attributes are bluff, no_truth_bluff, challenge.
Names are exact strings:
RandomLegal[uniform]
HonestFirst[B0-N0-C0]
MixedGreedy[B10-N10-C20] (substituting decimal integer percentages).
Names are exposed by .name, stable and deterministic.
mixed_grid() returns an iterable of all 1331 unique MixedGreedy configurations,
ordered lexicographically by (bluff, no_truth_bluff, challenge); B outermost,
C innermost. Calling the grid again is repeatable.

## Personalized observation and lifecycle

Observation is an immutable record with fields player:int, hand:tuple[Card,...],
opponent_count:int, top:Card, chosen_suit:str|None, phase:str,
draw_penalty:int, skip_pending:bool, provisional_winner:int|None,
deck_count:int, pile_count:int, known_pile_cards:frozenset[Card], opening_card:bool.
`opening_card` identifies the untouched starting discard, including after draws;
its special effect is inactive, but an opening seven/K♠ contributes if the first
play starts a draw penalty.
No opponent hand, actual hidden pile, deck order, seed, or RNG state is exposed.
The move list itself is generated for state.turn and exposes no hidden data.

PileKnowledge is a pair of frozensets, one per player.
new_knowledge(starting_card:Card) returns knowledge containing that revealed
starting identity for both players.
advance_knowledge(before:GameState, move:Move, after:GameState,
                  knowledge:PileKnowledge) -> PileKnowledge
updates knowledge for one genuine Play transition, without mutating inputs:
- An actual played card is added only to the acting player's knowledge.
- Accepting never proves an opponent declaration truthful.
- Challenge reveals the latest actual card to both players, truthful or false.
- Recycled discards leave BOTH known-pile sets. Their order is shuffled and not
  exposed. The top discard stays and remains known only to players who knew it.
- A challenge can reveal and recycle within one transition: retain the revealed
  top, remove every recycled older card.
- No knowledge of a recycled card is reacquired from an opponent's later hidden
  play or from a declaration. Only new own play/reveal events add knowledge.

observe(state:GameState, knowledge:PileKnowledge=(frozenset(),frozenset()))
returns the next actor's Observation, including ONLY that actor's known set.
The adapter handles valid engine states and coherent knowledge from the above
lifecycle. It must not infer previously exposed cards from the current physical
pile. Starting/revealed/public identities are events, not facts derivable from
an arbitrary snapshot. Constructors normalize known_pile_cards to a frozenset
if necessary; exposed nested containers remain immutable.

## Shared small tactical layer (inside EVERY baseline)

No search or growing catalogue of heuristics. First take a recognized guaranteed
win, then a last chance, then a certain bluff, and otherwise use policy.
Recognized guarantees:
1. Accept if already empty and acceptance wins immediately: zero draw penalty
   and either no pending ace or opponent also empty.
2. Play a truthful legal last-card penalty (seven/KS) against an empty opponent.
3. Play a truthful legal last ace against an opponent with exactly one card.
4. Accept a response if it permits either guaranteed final play in 2 or 3 on
   our following turn. Against a pending ace, play the final ace directly because
   accepting would consume the skip. This outranks catching a provable bluff.
Last chances:
5. If both hands are empty and a response has a draw penalty, Challenge.
6. If opponent is empty and we have one card against a pending response ace,
   Challenge. Accepting skips immediately, and our final ace or queen cannot return them.
Certain bluff:
7. Challenge an identity in our own hand or known_pile_cards, unless a guaranteed
   win above applies. These are certainly successful calls, not a claim of
   globally optimal strategic play.
Return filter:
8. On a turn against an empty opponent, if any return route exists, remove
   Draw/Skip and declarations other than penalties or aces leaving >=1 card.
   Strategy still chooses among survivors. If no survivor exists (an already
   lost one-card/ace turn), return some legal move instead of failing.

A lack of ACTUAL sevens never proves no return route: bluff declarations count.
Partial draws do not prevent returns: drawing even one card returns the player,
and a played card makes older discard material recyclable.

## Fallback policies

RandomLegal is uniform over remaining legal moves after the tactical layer.
It may draw normally: uniform randomness is its baseline fallback definition.

HonestFirst prefers a truthful surviving play. Without one, it draws/skips if
that remains allowed, otherwise bluffs. Uncertain responses are accepted.

MixedGreedy, after tactical overrides/filter:
- If truthful and bluff plays both exist, bluff with B%, else play truthfully.
  Never voluntarily Draw/Skip when a truthful surviving play exists.
- If no truthful play exists, bluff with N%, otherwise Draw/Skip if available.
- If bluff is the only way to continue, bluff regardless of N.
- If no bluff exists but truth does, choose truth regardless of B.
- On uncertain responses, Challenge with C%, else Accept.
- Threshold convention is rng.random() < percentage / 100 (strict boundary).
- B and N are separate: one does not stand in for the other.

Both greedy policies keep the same coarse card preferences after selecting a
truth/bluff family: when bluffing prefer a declared identity still held;
against <=2 opponent cards prefer sevens/KS/aces, otherwise ordinary declarations;
then prefer discarding an ordinary actual card; for declared queens maximize
the count of remaining cards of the chosen suit. Ties use the supplied RNG.
Ordinary cards exclude queens, aces, all sevens, and KS.
These are fixed preferences, not hard tactical rules. No history model/search.

## Match and evaluation API

run_match(bots:tuple[Bot,Bot], *, seed=0, bot_seeds=(0,1),
          max_decisions=1000, initial_state=None, initial_knowledge=None)
returns immutable MatchResult with final_state, knowledge, decisions and
per-seat tuples plays, bluffs, responses, challenges, correct_challenges;
.winner is final_state.winner and .truncated is winner is None.
New games start with both players knowing the revealed starting card. Resuming
an arbitrary initial_state without initial_knowledge starts with empty knowledge
(conservative, never reverse-engineer history). A provided pair is carried on.
Use state.turn for every decision, including empty-hand responses. Build a safe
observation, call the policy, validate its output through Play, and advance
personal knowledge across the real transition. Game/recycling RNG, two policy
RNGs, and global random state are independent. Do not enforce baseline tactical
rules in this generic runner. Illegal outputs raise ValueError via the engine.
Game seed and bot seeds are integers (not bool); two callable bots and two bot
seeds are required; max_decisions is a positive integer (not bool).
No decision is dispatched in an already terminal state. A cutoff is unfinished,
not an invented draw/winner. A resumed run counts only its own actions.

round_robin(bots:dict[str,Bot], seeds, *, bot_seeds=(0,1), max_decisions=1000)
replays every unordered pair over the same deals with seats exchanged. Requires
>=2 named callable bots and >=1 integer game seed. Returns per-name rows with
games,wins,losses,truncated,total_game_decisions,plays,bluffs,responses,challenges,
correct_challenges,mean_game_length,bluff_rate,challenge_rate,
challenge_success_rate. Means include cutoffs; rates use plays/responses/
challenges respectively; zero denominators -> None.

evaluate_grid(seeds, *, candidates=None, opponents=None, bot_seeds=(0,1),
              max_decisions=1000) evaluates each candidate against a fixed
opponent roster in both seats, never all 1331 candidates against each other.
Defaults: mixed_grid candidates and RandomLegal/HonestFirst opponents. Optional
candidates is an iterable of named bot instances; opponents is a mapping of
labels to callable bots. Returns per-candidate-name statistics with the same
row keys above, aggregating ONLY the candidate's games. Names must be unique;
require nonempty candidates/opponents/seeds, validate seeds and settings.

CLI: python3 -B -m src.engines.baseline --deals 1 --max-decisions 200
runs the three named defaults and prints a JSON mapping {bot_name: stats_row}. --grid evaluates the grid
against fixed default opponents. --seed and --bot-seed control repeatability.
No full-scale grid tournament must run as part of normal unit tests.
