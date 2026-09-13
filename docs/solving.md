# Toward solving Bluff Mau-Mau

Research and experiments: 13 September 2026. **The full game is not solved.**
The objective is a strategy with low exploitability against any legal opponent,
not a strategy specialized to the existing baseline roster.

All measured game results below predate the correction forbidding queen
declarations on aces, sevens, and K♠, regardless of active effects. They remain
historical evidence for the earlier rule set. Current source uses the corrected
rules, so game-tree sizes, seeded results, trained policies, and performance
ratings must be measured again before being claimed for the current game.

## What comparable research establishes

| Work | Result and relevance | What it does not establish |
| --- | --- | --- |
| [Yi and Kaneko: CFR for Cheat, 2022](https://link.springer.com/chapter/10.1007/978-3-031-11488-5_1) | Very close deception mechanism. Repeated states permit unbounded play; the authors introduce Health Points and smaller abstractions to make training finite. | The accessible publisher abstract reports training experiments, not a solution of unrestricted Cheat. Full text was not available in this research session. |
| [Neller and Hnath: Dudo, 2012](https://cs.gettysburg.edu/~tneller/papers/acg2011.pdf) | Uses a known optimal one-die-versus-one-die strategy as a reference, then approximates the larger bluffing game with CFR and compressed histories. | Its imperfect-recall abstraction has no general equilibrium convergence guarantee; the paper leaves the full-game value and approximation quality open. |
| [Bowling et al.: heads-up limit hold'em, 2015](https://pubmed.ncbi.nlm.nih.gov/25574016/) | A genuine large imperfect-information precedent: a particular two-player poker variant was essentially weakly solved using CFR+. | This is not a claim that every poker variant, or every bluffing game, is solved. |
| [RLCard, 2020](https://arxiv.org/abs/1910.04376) | Includes UNO and neural reinforcement-learning experiments, making it relevant to matching and shedding cards. | An RL benchmark or empirical improvement is not an equilibrium certificate. |

The sources located do not supply a solution for our precise combination of
32 unique cards, false declarations, ace counters, accumulated penalties,
provisional victories, returns, and recycled private card knowledge. This is a
search finding, not proof that no related unpublished work exists.

## Recommended learning architecture

My recommendation is **sampled CFR, then Deep CFR, then optional public-belief
search**. These are development stages; a neural model has not been trained yet.

1. Keep the exact small-tree implementation as a correctness oracle. Scale the
   traversal with Monte Carlo CFR before introducing approximation by a network.
   Self-play updates both players. Baseline-specific opponent models are not part
   of this equilibrium objective. The average mixed strategy is the output,
   rather than just the latest deterministic policy. [CFR paper](https://proceedings.neurips.cc/paper/2007/hash/08d98638c6fcd194a4b1e6992063e944-Abstract.html)
2. Use Deep CFR when the table of information sets becomes too large. A network
   predicts counterfactual advantages for legal actions, generalizing across
   information sets. Sampled traversals supply training targets; reservoir
   memories retain representative past training data. Approximation error can
   leave an exploitability floor, so a trained network is not automatically a
   solution. [Deep CFR](https://proceedings.mlr.press/v97/brown19b.html)
3. Add ReBeL-style search after the information model and learned values are
   reliable. This combines public beliefs, local equilibrium search, and learned
   continuation values. It is the closest established approach to passing
   distributions down a tree and values back up. Its research results include
   poker and Liar's Dice; transferring it here requires adapting the game and
   belief representation. [ReBeL](https://arxiv.org/abs/2007.13544)
4. Train independent adversarial responses and maintain a population of previous
   policies. PSRO provides a principled population approach. A learned adversary
   demonstrates a lower bound on exploitability when it finds a weakness;
   failure to find one does not certify robustness. Use exhaustive best responses
   wherever the game size permits them. [PSRO](https://papers.nips.cc/paper_files/paper/2017/file/3323fe11e9595c09af38fe67567a9394-Paper.pdf)

NFSP is a reasonable alternative: one network learns approximate best responses
and another learns historical average behavior. Its poker experiments approached
equilibrium where ordinary RL comparisons remained exploitable. Plain PPO/DQN
self-play can still supply useful opponents, but its win rate is not the primary
progress measure for this project. [NFSP](https://arxiv.org/abs/1603.01121)

The Rust rules engine remains the simulator. Future model inputs should encode
the player's own cards, public effects, private knowledge, and their observation
and action history. Score the generated legal actions, including actual card,
declared card, and queen suit together. An unrestricted factorization of those
choices could select an illegal combination. A recurrent/history encoder is an
approximation unless it is shown to retain the strategically relevant information.

## Distributions forward, values backward

Let `h` be a possible hidden history and `b(h)` its current probability. After
observing event `o`, the update must include the acting policy's likelihood:

```text
b'(h') ∝ Σ_h b(h) Σ_a π_actor(a | I_actor(h)) P(h', o | h, a)
```

For example, a truth-first opponent's declaration is evidence of which cards they
hold. Uniformly choosing its hidden actual card from the unseen deck ignores that
evidence. Reveals eliminate inconsistent histories. Shuffling destroys positional
certainty; it does not erase all useful information about which cards entered the
draw pool. Each player retains a different private history.

For a fixed policy, continuation values are probability-weighted backward. At an
information set, the player must choose one shared action distribution across all
histories they cannot distinguish. The solver must not maximize independently in
each sampled hidden world and then average those maxima: that would grant access
to hidden information. CFR instead accumulates counterfactual action regrets and
averages its policies. [Information-set search](https://eprints.whiterose.ac.uk/id/eprint/75048/)

For two-player zero-sum utility, the implemented evaluation reports:

```text
BR0 = max_response u0(response, average_policy1)
BR1 = max_response u1(average_policy0, response)
NashConv = BR0 + BR1
exploitability = NashConv / 2
```

Each best response respects information sets. This measures the mean unilateral
improvement available to the two players. Values use the modeled utility scale,
not Elo or an unqualified win-rate percentage.

## Implemented solver and its actual evidence

- [cfr.rs](../src/engines/cfr.rs): simultaneous tabular CFR, own-reach-weighted
  average strategies, perfect-recall/tree validation, and exhaustive best responses.
- [solving.rs](../src/engines/solving.rs): converts supplied weighted hidden worlds
  into a finite tree using every generated legal action and the core
  transitions. Separate private/public histories define information sets. No
  baseline tactic filters the actions.
- [solve.rs](../examples/solve.rs): reproducible endgame experiment and flushed
  JSONL convergence reports, with final root action probabilities.
- [mccfr.rs](../src/engines/mccfr.rs): outcome-sampling self-play with lazy
  information-set storage, average policies, and exact finite-tree evaluation.
- [train.rs](../examples/train.rs) and [training_store.rs](../src/engines/training_store.rs):
  procedural full-history training, source/model-bound checkpoints and crash recovery.
- [Independent tests](../tests/cfr_contract.rs): six original checks were written
  without inspecting the implementations; a source audit added the seventh,
  a private draw-order regression. Kuhn poker converges to its known value of -1/18; reported
  best responses match independent enumeration of all 64 pure responses per
  player to `1e-12`. Other checks cover unequal chance weights, perfect recall,
  hidden actions, reveals, and information sets occurring at different depths.

At the end of that research implementation, all 56 tests passed with
`cargo test --release --all-targets`; formatting and Clippy with warnings denied
passed. The then-current frozen rule-engine reference checks passed. That work
did not change the UI, core rules, baseline policies, or saved arena ratings.

The example defines four equally probable hidden scenarios: player zero has 9H
or 8D, player one has 7H or 9S. The public top is 9C, the deck is empty, and the
other cards are in the pile. Each player sees their own card, and both know this
specified prior. Deck/pile ordering and future shuffle randomness are fixed
within each scenario. These are declared model assumptions, not a posterior
reconstructed from a real match.

Actual wins have utility +1 and losses -1. Unresolved depth cutoffs default to 0.

| Decision depth | Tree nodes | Information sets | CFR iterations | Initial exploitability | Final exploitability | Wall time |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 3 | 18,287 | 1,516 | 2,000 | 0.523563 | 0.000597116 | 0.26 s |
| 4 | 194,025 | 6,918 | 5,000 | 0.597589 | 0.000274775 | 5.12 s |

These are exhaustive best-response evaluations on those finite models, up to
floating-point error. They are not full-game exploitability bounds. Even this
four-world example expands more than tenfold with one additional decision.
The final runs are the `cfr-*-ordered.jsonl` files; private histories retain
observed hand/draw order, which can carry information even when card sets match.

To expose the cutoff assumption, the depth-four experiment was also solved with
every unresolved leaf scored -1 and then +1 for player zero. The conservative
value interval, using `-BR1` from the pessimistic model and `BR0` from the
optimistic model, is approximately **[-0.50037, 1]**. This wide interval shows that
the unresolved continuation matters much more than the small CFR convergence
error. A tiny gap in the draw-cutoff model must not be mistaken for a solution of
its continuation. The interval still concerns only the declared initial prior;
it supplies no bound for omitted hidden worlds.

## Sampled self-play implemented and measured

The Rust trainer now samples one complete trajectory per iteration, then updates
both players' regrets simultaneously. Both players explore with
`q(a) = (1 - epsilon) * policy(a) + epsilon / legal_action_count`; the experiments
use `epsilon = 0.6`. Importance weights correct for this exploration. Chance is
sampled from the environment's own distribution. No baseline opponent model or
"obvious move" filter participates in training. This is outcome-sampling
[Monte Carlo CFR](https://papers.nips.cc/paper_files/paper/2009/file/00411460f7c92d2124a67ea0f4cb5f85-Paper.pdf).

For a visited player-i node, let `q_prefix` include only sampled player actions,
`own_prefix` the target probability of i's earlier actions, and `other_prefix`
the other player's target probability. The sampled action-value estimate uses
the terminal payoff and the remaining target/sampling probability ratios.
Regrets use `other_prefix / q_prefix`; average policies use
`own_prefix / q_prefix`. Sampling true chance cancels it from the regret ratio.
The average estimator has a fixed information-set chance-mass factor, which
cancels when its row is normalized. Exploring both players preserves support.
The independent mathematical review checked these particular estimators.

`RulesGame` runs the core transitions on demand, without expanding a tree. Fixed
scenarios reproduce the exact endgame model. Fresh-deal mode samples the original
deal directly and uses fresh shuffle randomness when the pile is recycled. The
research implementation's only core-file change exposed its existing dealer
within the crate; it did not change the then-current move lists, transitions,
or dealing behavior. Both modes retain ordered private hands, own actual plays,
public declarations/reveals, and the complete sequence
of observations. Recycling clears card-location certainty, but retains history.

All runs below use zero utility for unresolved cutoffs. Each endgame policy was
evaluated with the exact best-response solver, including information sets the
sampled trainer had not visited (whose policy defaults to uniform).

| Model | Seed | Sampled games | Initial exploitability | Final exploitability | Player-0 value | Training/report wall time |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Four-world endgame, depth 3 | 42 | 2,000,000 | 0.523563 | 0.00373173 | 0.00586803 | 15.77 s |
| Four-world endgame, depth 3 | 43 | 2,000,000 | 0.523563 | 0.00327202 | 0.00564580 | 15.76 s |
| Four-world endgame, depth 3 | 44 | 2,000,000 | 0.523563 | 0.00377715 | 0.00596016 | 15.66 s |
| Four-world endgame, depth 4 | 42 | 2,000,000 | 0.597589 | 0.01524936 | 0.00561260 | 19.58 s |

These independent runs ran concurrently. Times include periodic checkpoints and
evaluations, and exclude initial exact-tree construction. The three-seed range
is not a confidence interval. Exact CFR is faster and more accurate on these
small trees; sampling's benefit is avoiding full expansion, not automatically
improving convergence per second in a small game.

The full fresh-deal pilot used a 32-decision horizon and 1,000 sampled games.
It made **11,747 decisions at 11,747 distinct information sets**: zero revisits.
Its checkpoint occupied **92,356,047 bytes (88.1 MiB)** and the run took about
1.05 seconds including four saves. With a previously unseen row, the sampled
average records the initial uniform policy; this pilot therefore supplies no
evidence of useful full-game learning. This is a measured reason to move to
function approximation, not just run the same table longer. Full-game
exploitability remains unavailable. The information-set cap bounds row count,
not total bytes; long histories and large legal-action lists enlarge each row.

The learned policy is exposed through `Trainer::average_strategy` using exact
history keys. It is not registered as a snapshot-only arena `Bot`: that interface
cannot supply its history. A neural policy and a full-game strength claim remain
future work. Long trajectories can also produce excessive importance weights;
the trainer rejects unrepresentable updates without committing that episode,
rather than silently clipping them and changing the estimator.

Five [independent MCCFR tests](../tests/mccfr_contract.rs) were written before
implementation inspection. They cover Kuhn's known value, exact exploitability,
unequal hidden-world probabilities, policy validity, byte-exact resume and
transactional information-budget failure. Four additional
[adapter tests](../tests/sampled_rules.rs) replay all short legal paths through
six rule positions and check hidden identities, reveals, ordered hands,
recycling memory, fresh dealing and canonical action/key agreement.

Checkpoints save the complete regret and average tables, counters and RNG, using
exact floating-point bit patterns. The storage envelope binds model parameters,
source fingerprint, compiler and platform; SHA256 detects corruption. An exclusive
writer lock prevents competing runs. File sync, atomic replacement and directory
sync preserve the last complete checkpoint. Ctrl-C/SIGTERM saves the current
completed iteration. A forced kill can lose the unsaved batch, bounded by
`--checkpoint-every`; it cannot turn a partial file into a valid checkpoint.
The final CLI audit exercised real SIGTERM and SIGKILL processes and verified
that both resumed to byte-identical checkpoints versus uninterrupted runs. Model
changes and corrupt checkpoints are rejected rather than overwritten.

## Held-out top-ten strength check

At the user's request, each candidate played **100 games against each of the
saved top ten baselines: 1,000 games per candidate**. Held-out deal seeds
3,000,000–3,000,049 were each played from both seats. Evaluation used a
1,000-decision cutoff; none of these 3,000 games reached it.

| Candidate | Wins | Draws | Losses | Win rate / score | W/L ratio | Performance Elo |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Trained MCCFR average, fresh-deal checkpoint | 40 | 0 | 960 | 4.0% | 0.0417 | 804.20 |
| Tactical C0 control | 858 | 0 | 142 | 85.8% | 6.0423 | 1678.89 |
| BeliefSearch S8/H40 control | 852 | 0 | 148 | 85.2% | 5.7568 | 1670.43 |

The MCCFR checkpoint had 1,000 fresh-deal training episodes and 11,747 stored
information sets. **Zero of its 6,383 decision histories matched a stored row.**
Every candidate decision therefore used the specified uniform distribution over
all legal actions, without baseline tactics. This is direct evidence that the
current tabular model does not generalize to held-out full games; small endgame
convergence does not make it a strong full-game bot. The evaluation also extends
the training horizon from 32 to 1,000 decisions; histories beyond the trained
horizon necessarily use the same unseen-history fallback.

Wins out of 100 against each opponent (all remaining games were losses):

| Opponent | Fixed Elo | MCCFR | Tactical | Search |
| --- | ---: | ---: | ---: | ---: |
| MixedGreedy[B0-N100-C40] | 1356.09 | 4 | 91 | 86 |
| MixedGreedy[B0-N100-C30] | 1387.65 | 5 | 83 | 77 |
| MixedGreedy[B0-N90-C40] | 1365.75 | 4 | 92 | 88 |
| MixedGreedy[B0-N90-C30] | 1344.59 | 6 | 83 | 81 |
| MixedGreedy[B10-N90-C40] | 1246.40 | 2 | 88 | 93 |
| MixedGreedy[B10-N100-C40] | 1342.60 | 2 | 90 | 93 |
| MixedGreedy[B0-N80-C40] | 1360.71 | 4 | 93 | 91 |
| MixedGreedy[B10-N90-C30] | 1415.33 | 4 | 77 | 92 |
| MixedGreedy[B0-N100-C50] | 1405.16 | 3 | 96 | 83 |
| MixedGreedy[B0-N100-C20] | 1399.22 | 6 | 65 | 68 |

Performance Elo is an order-independent diagnostic: find rating R satisfying
`sum_j games_j / (1 + 10^((opponent_elo_j - R)/400)) = wins + draws/2`.
Opponent ratings are the rounded published values in the saved ranking and
remain fixed. This is not a new persistent rating or a K=20 update, and the small
selected opponent set does not establish a universal strength rating. The
0.6-percentage-point control difference is too small to establish a winner from
this sample.

[evaluate_trained.rs](../examples/evaluate_trained.rs) loads the checkpoint
immutably, maintains a separate private history for each game, and writes every
game with a disk sync. It uses canonical legal ordering for opponents; this
preserves their policy distributions while seeded tie choices can differ from
the original arena/control runner. The original UI, arena DB, Elo and trained
checkpoint were not modified by evaluation. The candidate's history lookup and
all Elo/count calculations are visible in the saved report and records.

Artifacts: `runs/20260913-mccfr-match-evaluation/` contains
`mccfr-results.md`, `mccfr-summary.json`, `mccfr-games.jsonl`,
`tactical-control.jsonl` and `search-control.jsonl`.

## Earlier playing-strength experiments

These are secondary diagnostics, not the equilibrium objective. The original
arena database and its Elo were not opened or updated by the research runner.

[Tactical](../src/engines/tactical.rs) adds direct ace counters, attack tempo,
hand connectivity, and contextual challenges. C0 was selected from development
tests of C0/C10/.../C60 against ten leading baselines and six controls. C is a base
challenge percentage: C0 can still challenge because of obvious moves and the
opponent-hand-size adjustment. It is not equivalent to baseline C0.

On 10,000 fresh deal seeds, each played in both seats against each of 16 opponents:

| Candidate / opponent | Games | Wins | Draws | Losses | Score | Approximate 95% interval |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| Tactical C0 / old champion B0-N100-C40 | 20,000 | 17,350 | 1 | 2,649 | 86.7525% | 86.28–87.22% |
| Tactical C0 / all 16 equally weighted | 320,000 | 267,210 | 15 | 52,775 | 83.5055% | 83.29–83.73% |
| Old champion / same 16 | 320,000 | 179,499 | 1,454 | 139,047 | 56.3206% | 56.05–56.59% |

[BeliefSearch](../src/engines/search.rs) is an experimental root-rollout bot, not
the CFR solver. It samples hidden worlds, applies observation-only rollout
policies, and retains Tactical's move unless paired simulations support an
improvement. Its snapshot prior is approximate, shared opponent knowledge is
incompletely reconstructed, and its rollout opponent is a C30/C40/C50 baseline
mixture. It therefore remains a playing-strength experiment rather than the
recommended full-game solving architecture.

The initial unrestricted four-sample search scored only 13% over 200 games;
32 samples and a longer horizon reached 50.25%, with 47 cutoff draws. Merely
increasing computation did not recover a good policy. Noisy choices repeatedly
bluffed or drew while assuming strong tactical behavior would begin next turn.
The first pilot also predated a subsequently fixed revealed-queen sampling bug.

The conservative S8/H40 revision was evaluated on separate seeds 2,000,000–2,000,999:

| Opponent | Games | Wins | Draws | Losses | Score | Approximate 95% interval |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| Old champion B0-N100-C40 | 2,000 | 1,762 | 0 | 238 | 88.10% | 86.69–89.51% |
| Tactical C0 | 2,000 | 1,145 | 0 | 855 | 57.25% | 55.22–59.28% |

The intervals cluster the two seats by deal seed, and aggregate intervals also
cluster repeated seeds across opponents. They quantify sampling uncertainty for
this selected opponent set, not uncertainty about arbitrary opponents. The
1,000-decision evaluation cutoff counts as a draw. Direct replies can use fewer
API decisions than explicit acceptance followed by play, so the cutoff is not a
neutral definition of game length. Degenerate samples report no normal interval.

## Reproduce and extend

All commands run from the repository root; all implementation and training here
are Rust. Current commands use the corrected rule engine. To reproduce the
historical results exactly, use the original saved executable and its rule set.

```sh
cargo run --release --example solve -- --depth 4 --iterations 5000 --report-every 500
cargo run --release --example solve -- --depth 4 --iterations 5000 --cutoff-utility -1
cargo run --release --example solve -- --depth 4 --iterations 5000 --cutoff-utility 1
cargo run --release --example train -- --run runs/my-mccfr --iterations 100000 --checkpoint-every 10000
# Resume to a larger TOTAL target, preserving the model options:
cargo run --release --example train -- --run runs/my-mccfr --iterations 200000 --checkpoint-every 10000
cargo run --release --example train -- --run runs/my-fresh-mccfr --model fresh --depth 32 --iterations 1000 --checkpoint-every 250 --max-information-sets 40000 --exact-node-cap 0
cargo run --release --example evaluate_trained -- --run runs/my-fresh-mccfr --output runs/my-heldout-evaluation
cargo run --release --example engine_lab -- --bot tactical --opponents all --deals 10000 --seed 1000000 --workers 8
cargo run --release --example engine_lab -- --bot search --opponents tactical --deals 1000 --seed 2000000 --workers 8 --samples 8 --horizon 40
cargo test --release --all-targets
```

Earlier raw JSONL experiments are preserved under `runs/20260913-strong-engines/`.
The final sampled runs are `runs/20260913-mccfr-final-d3-s{42,43,44}/`,
`runs/20260913-mccfr-final-d4-s42/` and
`runs/20260913-mccfr-final-fresh-d32-s42/`. Each contains `config.json`,
`progress.jsonl`, `checkpoint.bin`, and a saved `trainer` executable. Use that
executable with `--run` and the same model options after future source changes;
resume is intended for the same binary/platform. The final process-level
recovery audit is `runs/20260913-030931-final-cli-audit-23995/audit.json`.
All these experiments finished and stopped. `runs/` is ignored by Git; this
document preserves their key results in the repository. The original arena DB
and its ratings remain unchanged. The older matchup runner still saves only
completed opponent summaries, and `solve` itself does not checkpoint exact CFR.

The next implementation target is neural approximation, justified by the fresh
pilot's complete lack of information-set reuse. Retain exact small-game tests
and adversarial evaluation when adding it. A true solution claim additionally
needs a clearly defined payoff for nonterminating
play, control of horizon/abstraction errors, and an exploitability bound in the
target game. The current 1,000-decision arena convention does not supply those.
