# Persistent baseline arena

Python 3.10+, standard library only, macOS/Linux. Reuse the existing match runner
and private observations unchanged. This public contract precedes implementation
and independent tests.

## Usage

```sh
python3 -B -m src.engines.arena --rounds 50 --workers 8
python3 -B -m src.engines.arena --roster basic --rounds 75 --workers 4
python3 -B -m src.engines.arena --resume runs/EXACT-RUN-DIRECTORY --workers 8
python3 -B -m src.engines.arena --resume runs/EXACT-RUN-DIRECTORY --rounds 100
python3 -B -m src.engines.arena --status runs/EXACT-RUN-DIRECTORY --top 20
```

The default `all` roster contains RandomLegal, HonestFirst, and every one of the
1,331 MixedGreedy configurations. `basic` contains the three default bots.
Names retain their parameters. New runs go under `runs/<local-date-and-time-to-
microseconds>-arena/`. `--runs-dir` changes the parent directory. New-run defaults:
seed 0, 50 paired rounds, 1,000 decisions per game, Elo 1000, K=20. `--seed` and
`--max-decisions` override those two settings. Workers default to min(8, CPU count)
and must be positive. `--top` controls terminal rows (default 20); exports contain
all entrants. Resume uses the stored configuration; only workers, top, and the
target round count can change. A new round target may extend but never shrink the
stored target. `--resume` and `--status` are mutually exclusive. Invalid options
must fail before creating a run or changing an existing run.

Every paired round gives each entrant one opponent and two games with exchanged
seats, except one bye when the entrant count is odd. Shuffle a circle schedule
from the run seed; each opponent appears once per full cycle. Shuffle afresh for
the next cycle. Pair order is also deterministic. Both legs use the same game
seed and same per-seat bot seeds; different pairs have distinct game seeds. Seeds
do not depend on Python's process-randomized hash or worker completion order.
For 1,333 entrants, 50 rounds = 66,600 games, 98 or 100 per entrant. A complete
cycle takes 1,333 rounds. For the three basic bots, 75 rounds = 150 games and
exactly 100 per entrant. `rounds` always means total target, not additional rounds.

## Results and persistence

Every entrant starts at Elo 1000. Each game updates both players simultaneously:
E_A = 1 / (1 + 10 ** ((R_B - R_A) / 400)); delta = 20 * (S_A - E_A).
New ratings are R_A + delta and R_B - delta, without rounding persisted values.
S_A is 1 for a win, 0 for a loss, 0.5 for an arena draw. Rating updates follow
schedule order, independent of process speed and interruption/resume.

The game has no natural draw result. Reaching max_decisions is an **arena draw**,
worth half a point each; preserve `reason="max_decisions"` in the game ledger.
This does not modify the rules or pretend the underlying game finished. A normal
win has `reason="win"`. A bot exception/worker failure aborts the run with an error,
never awards a win or draw, and leaves uncommitted games pending for resume.

Rank by score_rate = (wins + draws/2) / games, descending; then by games descending,
wins descending, and name ascending. Unplayed entrants sort after played entrants
and have score_rate null. Elo never breaks ranking ties. Also display points,
games, wins, draws, losses and Elo. Early scores and different opponent samples
are noisy; this is a baseline comparison, not a statistical strength guarantee.

`arena.sqlite3` is authoritative: configuration, full game ledger, and all Elo /
W-D-L updates. A single coordinator commits both legs of each pair and all four
player updates in one SQLite transaction. Unique game IDs prevent duplicate
credit; committed results form a contiguous schedule prefix. No second writer may
run the same directory concurrently; read-only status remains available. A crash
or forced kill preserves committed pairs; an unfinished/uncommitted pair replays
from its original seeds. Ctrl-C and SIGTERM stop dispatching and exit cleanly
after bounded in-flight baseline work. Restart is at pair granularity, not from a
saved mid-game position. There is no pickled executable state.

Print progress periodically (at least every 5 seconds while games complete) and
at exit. Export the full sorted `leaderboard.csv` using atomic replacement at
progress/exit. CSV is derived and can lag after a forced kill; `--status` reads
fresh SQLite data. Store the source fingerprint of the actual game/policy/runner
code, arena format version and Python version; refuse to mix changed code/runtime
into an old run. Changing worker count is safe. Rating and schedule state must
survive abrupt process exit. Ignore `runs/` in git.

## Python API (src.engines.arena)

- `baseline_roster(kind="all") -> dict[str, Bot]`; accepts `all` or `basic`.
- `round_pairs(names, seed, round_index) -> tuple[tuple[str, str], ...]`.
  Zero-based round index. At least two unique nonempty string names; integer seed
  (not bool); nonnegative integer round_index (not bool). No mutation/global RNG.
- `elo_update(rating_a, rating_b, score_a, k=20) -> tuple[float, float]`.
  Finite numeric ratings (not bool), score in {0, 0.5, 1}, finite positive numeric K.
  Reject invalid input with ValueError; support large finite rating differences.
- `create_run(parent="runs", *, roster="all", seed=0, rounds=50,
  max_decisions=1000) -> pathlib.Path`. Validate before filesystem mutation.
- `run_arena(path, *, workers=None, rounds=None, max_pairs=None) -> list[dict]`.
  Resume or continue the created run; `max_pairs` optionally limits newly committed
  pairs during this call (nonnegative int, useful for bounded batches); it does not
  change the target. Only positive integer workers/rounds accepted (not bool).
  Changing a target below the stored target is rejected even before any games.
- `standings(path) -> list[dict]` reads authoritative results without a writer lock.
  Each row contains at least `rank` (one-based), `name`, `elo`, `games`, `wins`,
  `draws`, `losses`, `points`, `score_rate`. All roster members appear even at zero.

Persistence table `games` exposes at least: `id` (zero-based contiguous),
`round_index`, `seat0`, `seat1`, `seed`, `bot_seed0`, `bot_seed1`, `winner` (seat or
NULL), `reason`, `decisions`. Tests may read these with sqlite3. Config layout and
other tables are implementation details. API errors for invalid settings/config,
concurrent run, and runtime/source mismatch should be clear ValueError/RuntimeError
rather than silently resetting state. CLI prints errors and returns nonzero.

## Verification (2026-09-13)

`python3 -B -m unittest -q` passed all 197 tests on Python 3.14.7. The 16 new
arena tests were authored by a fresh agent using only this public contract and
the rules, without reading/importing/running the implementation or existing tests.
They passed unchanged on first execution. A later housekeeping edit closed the
test helpers' SQLite connections; assertions and fixtures stayed unchanged.
Final `tests/arena/test_arena.py` SHA256:
`66728689f3009bca17783062012a69afd4946540d7a8d1fa8cfd34ae4800316a`.
Blindness was enforced by agent instructions and separated context, not an OS
filesystem restriction.

An independent auditor then exercised real subprocess failures against the final
implementation: live read-only status, concurrent-writer rejection, SIGTERM,
coordinator-only SIGKILL with worker cleanup, and a killed worker. Each interrupted
run resumed with a different worker count and exactly matched an uninterrupted
240-game reference ledger and ratings. SIGTERM exited in 0.22 seconds and committed
only its current pair; queued work remained pending. Independent ledger replay
verified W/D/L, Elo, contiguous IDs, paired seats/seeds, and CSV consistency.
Changed source/Python/format fingerprints were rejected without score mutation.
The final audited `src/engines/arena.py` SHA256 is
`3e4af03ea817f43a7974e843d9e4efef639a9562caac83f838cf2f7a5b776272`.

A full-grid pilot exposed idle workers behind unusually long games. A bounded
lookahead of eight pairs per worker now keeps workers occupied; rating commits
remain in schedule order. The full test suite and subprocess failure audit were
repeated after this change. No game engine, bot policy, or web implementation was
changed for the arena.
