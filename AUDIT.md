# Backend verification

The backend and its engine/arena tests are Rust; frontend checks use Node.js and
Playwright. The authoritative rules are
[RULES.md](RULES.md), with transition conventions in
[clarification-rules.md](clarification-rules.md). Run the current checks from the
repository root:

```sh
cargo test --release
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo run --release --example benchmark -- 120
```

The migration measurements and parity counts below are historical. They predate
the correction forbidding queen declarations on aces, sevens, and K♠, including
when those cards have no active effect. They do not certify the old queen rule;
current regression tests enforce the corrected [rules](RULES.md).

## Frozen behavioral reference

`tests/fixtures/python-reference.json` is **test data**, retained because Rust
integration tests consume it. It contains frozen outputs from the original
implementation, including source hashes, complete state and RNG values, ordered
moves, bot choices, knowledge, and evaluation results. Its SHA-256 is:

```text
afaad3e795b6574387aa8f0853d12b7c195ad01937376a4961efd189359e7b34
```

The data author generated and froze it before inspecting the Rust implementation.
No Python runtime or source file is needed to execute the reference tests. The
original implementation and its historical audit/test records remain available
in Git at commit `8e92832`; they are not part of the working backend.

The original migration's nine parity tests passed on their first execution:

- 140 initial deals, including negative and 1,024-bit seeds.
- 2,225 positions and 76,077 transitions, comparing complete ordered legal moves.
- 7,609 trace decisions, including private knowledge after reveals and recycling.
- All 1,333 bots in five public positions: identical chosen move and consumed RNG.
- 45 complete matches: identical final state, knowledge, and action counters.
- Debug-game traces: identical JSON views, move IDs, history, and explanations.
- Paired evaluation statistics and seeded arena schedules.

After the queen correction, the unchanged fixture is used only where its
expectations agree with the current rules: 140 initial deals, 1,900 positions,
49,948 transitions, three full 1,333-bot policy grids, and 27 seeded schedules.
Trace and HTTP comparisons retain exact prefixes before the first changed
decision. These exclusions are explicit and their coverage counts are checked;
the old queen behavior is not a current test oracle. Current Rust regressions
cover queen restrictions and legal alternatives. Complete matches are checked
against public `play` replay with independent counters and full knowledge
comparison; paired evaluation totals are checked against separately aggregated
current matches.
No reference data was regenerated and no Python runtime or source was restored.

This is extensive behavioral evidence, not an exhaustive proof over every possible
manually constructed state. Rust-owned values replace Python's object-type checks;
public state validation and illegal-action rejection remain tested.

## Arena test independence

A separate agent authored eight Rust arena contract tests from the public contract
and original arena tests, without reading the Rust implementation or existing Rust
tests. The frozen pre-execution SHA-256 was:

```text
aeac9d09e019a4cde77adf7d9e2a417b3074f1f7087fe3a2b7d37827d09e1ca8
```

All eight passed on first execution. Formatting and an equivalent iterator cleanup
subsequently changed the file without changing assertions. The authors' separation
was procedural, not an OS-enforced filesystem restriction. These are historical
provenance hashes; they are not assertions about later edited test files.

Real-process migration checks compared uninterrupted and resumed 800-game arenas.
Live status, second-writer rejection, SIGTERM, SIGKILL, and changed worker counts
preserved the exact ledger, ratings, and CSV. HTTP boundary checks also exercised
the actual Rust server with the existing UI assets. Subprocess arena tests ran
with a PATH containing no external runtime.

## Migration measurements (2026-09-13)

The audited migration at `8e92832` passed 25 Rust tests, formatting, and Clippy with
warnings denied on Rust 1.98.1 / macOS ARM64. A final audit fixed overflow handling
for extreme manually imported draw penalties before mutation and added regression
coverage for the public and trusted transition paths.

The reproducible single-thread workload in `examples/benchmark.rs` used 120 games,
14,874 decisions, and checksum 26,243. Across three runs on the same machine,
median reference runtime was 6.5288 seconds and optimized Rust was 0.05540 seconds,
approximately **118× faster**. This measures match execution rather than SQLite.
The reference timing was taken during migration; only the Rust benchmark remains.

The full 50-round, 1,333-bot comparison completed **66,600 games in 11.12 seconds**
with eight workers, including SQLite persistence and CSV export. Every ledger row
matched the reference run: opponents, seats, seeds, results, and decision counts.
All W/D/L records matched. Maximum Elo difference was 2.2737367544323206e-13 from
floating-point exponentiation rounding; displayed Elo and ranking were identical.
These are historical measurements on that machine, not a performance guarantee.

## Cleanup verification (2026-09-13)

The Cargo manifest, lockfile, and build script now live at the repository root;
Rust sources are in `src/`, integration tests in `tests/`, and the benchmark in
`examples/`. The old backend and its tests (29 Python files) and the separate
migration directory were removed. Two unused wrappers, `DebugGame::reset` and
`Baseline::from_name`, were deleted after checking all callers. Cargo now uses
its standard target discovery instead of redundant target declarations.

An independent audit read every Rust module, entry point, benchmark, build script,
and integration test. All dependencies and remaining private helpers have callers.
Documented public APIs and the actively consumed reference fixture were retained.
The source/documentation sweep found no obsolete backend launch commands, deleted
source links, or Python source/cache files in the project. Local Markdown links
in all 13 project documents resolve. Operational documentation and comments now
describe the current Rust backend; historical verification is explicitly labeled.

Post-cleanup validation passed:

- All 25 Rust tests, including the unchanged 76,077 reference transitions.
- `cargo clippy --locked --all-targets -- -D warnings` and formatting checks.
- Both existing Node checks and all 10 unchanged Chrome integration tests.
- Server asset resolution from the root Cargo layout, HTTP boundaries, and presets.
- The deterministic benchmark: 120 games, 14,874 decisions, checksum 26,243.
- A fresh 66,600-game arena compared with the previous Rust run: every game row,
  all player records including Elo, and the entire CSV matched exactly.
- The status command reads the completed run successfully; arena tests retain
  resume, changed-worker-count, locking, and invalid-input coverage.

The post-cleanup run is `runs/20260913-013745-060170-rust-arena/`; the previous
comparison run is `runs/20260913-012802-340092-rust-arena/`. Runs are ignored user
data. Source fingerprints intentionally change after source or manifest edits;
existing results remain readable, while resume requires the same compiled build.

The browser test launcher now starts the Rust server and stops it after the suite.
No UI HTML, CSS, JavaScript, fonts, card assets, or browser-test assertions changed.
Only obsolete backend references in UI documentation/test configuration were updated.
