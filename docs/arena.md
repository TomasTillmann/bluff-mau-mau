# Persistent baseline arena

Build with `cargo build --release` from the repository root. The Rust arena uses
worker threads, the existing match runner, and private bot observations.

## Usage and schedule

```sh
./target/release/arena --rounds 50 --workers 8
./target/release/arena --roster basic --rounds 75 --workers 4
./target/release/arena --resume runs/EXACT-DIRECTORY --workers 8
./target/release/arena --resume runs/EXACT-DIRECTORY --rounds 100
./target/release/arena --status runs/EXACT-DIRECTORY --top 20
```

The default `all` roster contains RandomLegal, HonestFirst, and all 1,331
MixedGreedy configurations; `basic` contains the three defaults. Names include
parameters. New runs go under `runs/<local-date-and-time-to-microseconds>-rust-arena/`.
`--runs-dir` changes the parent. Defaults are seed 0, 50 paired rounds, 1,000
decisions per game, Elo 1000, and K=20. `--seed` and `--max-decisions` override
those settings. Workers default to min(8, CPU count) and must be positive.
`--top` controls terminal rows (default 20); CSV exports contain every entrant.

Resume retains the stored configuration; only workers, top, and the total round
target can change. A target may extend but never shrink. `--resume` and `--status`
are mutually exclusive; status accepts `--top` but not workers or rounds.
Invalid options fail before creating or changing a run.

Each paired round gives every entrant one opponent and **two games with exchanged
seats**, except for one bye when the roster size is odd. A seeded, shuffled circle
schedule visits each opponent once per full cycle and shuffles afresh for the next
cycle. Pair order is deterministic. Both legs share the game seed and per-seat
policy seeds; different pairs have distinct game seeds. Scheduling and rating
order do not depend on worker completion timing.

| Roster and target | Games | Coverage |
| --- | ---: | --- |
| 1,333 bots, 50 rounds | 66,600 | 98 or 100 per bot; sampled opponents |
| 1,333 bots, 1,333 rounds | 1,775,556 | Every pair twice |
| 1,333 bots, 19,995 rounds | 26,633,340 | Every pair 30 times |
| 3 basic bots, 75 rounds | 150 | Exactly 100 per bot |

## Ratings and ranking

Each game updates both ratings simultaneously without rounding stored values:

```text
expected_a = 1 / (1 + 10 ^ ((rating_b - rating_a) / 400))
delta = 20 * (score_a - expected_a)
new_a = rating_a + delta
new_b = rating_b - delta
```

`score_a` is 1 for a win, 0 for a loss, and 0.5 for an arena draw. Reaching the
decision cutoff counts as an **arena draw**, stored with `reason="max_decisions"`;
it does not invent a game-rule draw or a finished underlying state. A normal win
has `reason="win"`. A bot error or worker failure aborts without awarding a result.

Rank by `(wins + draws/2) / games` descending, then games descending, wins descending,
and name ascending. Elo does not break ties. Unplayed entrants sort last and have
no score rate. Display points, games, W/D/L, and Elo. Partial schedules compare
different opponent samples, so early rankings are only preliminary evidence.

## Persistence and stopping

`arena.sqlite3` is authoritative for configuration, game ledger, Elo, and W/D/L.
One coordinator commits both legs of a pair and all player updates in one SQLite
transaction, using WAL and synchronous FULL. Committed game IDs form a contiguous
schedule prefix. A file lock rejects a second writer; status remains available.

The worker queue has bounded lookahead. Results commit in schedule order. Ctrl-C
or SIGTERM stops dispatch and finishes the current earliest pair; remaining
uncommitted work is pending. SIGKILL also preserves committed pairs. Resume replays
only unfinished work from the same seeds, at pair granularity rather than from a
saved mid-game position. Threads share the process lifetime, so a killed arena
leaves no worker processes. Worker count may change between invocations.

The coordinator prints progress and atomically replaces the complete sorted
`leaderboard.csv` every five seconds while games complete, and at exit. CSV may
lag after a forced kill; status reads fresh database records.

Runs use format 2 and a fingerprint of the compiled core/policies/runner/arena,
Cargo manifest and lockfile, compiler, and platform. A changed fingerprint is
rejected on resume. Editing sources cannot relabel an already-built executable.
Historical runs from the removed Python implementation can still be inspected
with `--status`, but cannot be resumed by Rust. Results are user data under the
git-ignored `runs/` directory, not executable legacy code.

## Rust API

All fallible operations below return `Result<_, String>` from `engines::arena`:

- `baseline_roster(kind: &str)` returns baseline bots for `"all"` or `"basic"`.
- `round_pairs(names: &[String], seed: i64, round_index: usize)` returns name pairs.
  Names must be unique/nonempty, with at least two entrants; the index is zero-based.
- `elo_update(a: f64, b: f64, score: f64, k: f64)` returns the updated rating pair.
  Ratings must be finite, score must be 0/0.5/1, and K finite and positive.
- `create_run(parent: &Path, roster: &str, seed: i64, rounds: usize,
  max_decisions: usize)` validates settings and returns the new directory path.
- `run_arena(path: &Path, options: RunOptions)` runs/resumes and returns standings.
  Options include worker count, optional total rounds, optional `max_pairs` limiting
  newly committed pairs, terminal row count, quiet output, and a shared atomic stop
  flag. A pair limit does not change the saved round target.
- `standings(path: &Path)` reads authoritative scores without a writer lock.
- `config(path: &Path)` reads the saved configuration.

Each `Standing` contains one-based rank, name, Elo, games, wins, draws, losses,
points, and optional score rate. All roster members appear, even before playing.
The ledger's `games` table stores zero-based `id`, `round_index`, `seat0`, `seat1`,
`seed`, `bot_seed0`, `bot_seed1`, nullable winning seat, reason, and decisions.
CLI errors are reported with a nonzero exit; invalid settings, lock contention,
or fingerprint mismatch never silently reset a run.

[The audit record](../AUDIT.md) describes the independent parity and persistence
checks, including forced termination and resume.
