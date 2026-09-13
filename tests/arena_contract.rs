//! Independent arena contract tests. Authored without reading Rust implementation/tests.
//! Public contract: docs/arena.md. Independent authorship provenance: AUDIT.md.
use bluff_mau_mau::engines::arena::{self, RunOptions};
use rusqlite::{Connection, OpenFlags, types::ValueRef};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static SERIAL: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "blind-rust-arena-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn create(&self, roster: &str, seed: i64, rounds: usize, decisions: usize) -> PathBuf {
        arena::create_run(&self.0, roster, seed, rounds, decisions).unwrap()
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run(path: &Path, workers: usize, rounds: Option<usize>, max_pairs: Option<usize>) {
    arena::run_arena(
        path,
        RunOptions {
            workers,
            rounds,
            max_pairs,
            quiet: true,
            ..Default::default()
        },
    )
    .unwrap();
}
fn standings(path: &Path) -> Vec<Value> {
    arena::standings(path)
        .unwrap()
        .into_iter()
        .map(|row| {
            json!({
                "rank": row.rank, "name": row.name, "elo": row.elo, "games": row.games,
                "wins": row.wins, "draws": row.draws, "losses": row.losses,
                "points": row.points, "score_rate": row.score_rate
            })
        })
        .collect()
}
fn query(path: &Path, sql: &str) -> Vec<Value> {
    let db =
        Connection::open_with_flags(path.join("arena.sqlite3"), OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let mut stmt = db.prepare(sql).unwrap();
    let names: Vec<String> = stmt
        .column_names()
        .iter()
        .map(|name| name.to_string())
        .collect();
    stmt.query_map([], |row| {
        let mut object = serde_json::Map::new();
        for (index, name) in names.iter().enumerate() {
            let value = match row.get_ref(index)? {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(value) => json!(value),
                ValueRef::Real(value) => json!(value),
                ValueRef::Text(value) => json!(std::str::from_utf8(value).unwrap()),
                ValueRef::Blob(_) => panic!("unexpected binary arena data"),
            };
            object.insert(name.clone(), value);
        }
        Ok(Value::Object(object))
    })
    .unwrap()
    .map(Result::unwrap)
    .collect()
}
fn games(path: &Path) -> Vec<Value> {
    query(
        path,
        "SELECT id,round_index,seat0,seat1,seed,bot_seed0,bot_seed1,winner,reason,decisions FROM games ORDER BY id",
    )
}
fn database_snapshot(path: &Path) -> Value {
    let schema = query(
        path,
        "SELECT type,name,tbl_name,sql FROM sqlite_master ORDER BY type,name",
    );
    let mut tables = BTreeMap::new();
    for row in &schema {
        if row["type"] == "table" {
            let name = row["name"].as_str().unwrap();
            let mut records = query(
                path,
                &format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")),
            );
            records.sort_by_key(Value::to_string);
            tables.insert(name.to_owned(), records);
        }
    }
    json!({"schema": schema, "tables": tables})
}
fn number(row: &Value, key: &str) -> f64 {
    row[key].as_f64().unwrap()
}
fn count(row: &Value, key: &str) -> usize {
    row[key].as_u64().unwrap() as usize
}
fn near(actual: f64, expected: f64) {
    assert!((actual - expected).abs() <= 1e-9, "{actual} != {expected}");
}
fn cli(args: &[&str]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_arena"))
        .args(args)
        .env("PATH", "/no-python-or-other-runtime-here")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let output = child.wait_with_output().unwrap();
            panic!(
                "arena CLI timed out: {args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        thread::sleep(Duration::from_millis(10));
    }
    child.wait_with_output().unwrap()
}
fn succeeds(output: Output) -> String {
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
fn rejects(output: Output) {
    assert!(!output.status.success(), "invalid input succeeded");
    assert!(
        !output.stdout.is_empty() || !output.stderr.is_empty(),
        "error needs an explanation"
    );
}

#[derive(Clone)]
struct Totals {
    elo: f64,
    games: usize,
    wins: usize,
    draws: usize,
    losses: usize,
}
fn audit(path: &Path) -> (Vec<Value>, Vec<Value>) {
    let rows = standings(path);
    let ledger = games(path);
    let mut totals: BTreeMap<String, Totals> = rows
        .iter()
        .map(|row| {
            (
                row["name"].as_str().unwrap().to_owned(),
                Totals {
                    elo: 1000.0,
                    games: 0,
                    wins: 0,
                    draws: 0,
                    losses: 0,
                },
            )
        })
        .collect();
    assert_eq!(totals.len(), rows.len());
    assert_eq!(ledger.len() % 2, 0);
    let mut seeds = BTreeSet::new();
    let mut rounds: BTreeMap<usize, BTreeSet<String>> = BTreeMap::new();
    for pair in ledger.as_chunks::<2>().0 {
        let (a, b) = (&pair[0], &pair[1]);
        assert_eq!(a["round_index"], b["round_index"]);
        assert_eq!(a["seat0"], b["seat1"]);
        assert_eq!(a["seat1"], b["seat0"]);
        for field in ["seed", "bot_seed0", "bot_seed1"] {
            assert_eq!(a[field], b[field]);
        }
        assert!(
            seeds.insert(a["seed"].to_string()),
            "each pair needs a distinct game seed"
        );
        let present = rounds.entry(count(a, "round_index")).or_default();
        for seat in ["seat0", "seat1"] {
            assert!(present.insert(a[seat].as_str().unwrap().to_owned()));
        }
    }
    for (id, game) in ledger.iter().enumerate() {
        assert_eq!(count(game, "id"), id);
        assert!(count(game, "decisions") > 0);
        let a = game["seat0"].as_str().unwrap();
        let b = game["seat1"].as_str().unwrap();
        assert_ne!(a, b);
        let score = match game["reason"].as_str().unwrap() {
            "max_decisions" => {
                assert!(game["winner"].is_null());
                0.5
            }
            "win" => match game["winner"].as_u64().unwrap() {
                0 => 1.0,
                1 => 0.0,
                seat => panic!("invalid winning seat {seat}"),
            },
            reason => panic!("unknown game result {reason}"),
        };
        // Replay the mathematical contract directly, never the production Elo helper.
        let delta =
            20.0 * (score - 1.0 / (1.0 + 10.0_f64.powf((totals[b].elo - totals[a].elo) / 400.0)));
        for (name, result, change) in [(a, score, delta), (b, 1.0 - score, -delta)] {
            let player = totals.get_mut(name).unwrap();
            player.elo += change;
            player.games += 1;
            if result == 1.0 {
                player.wins += 1;
            } else if result == 0.5 {
                player.draws += 1;
            } else {
                player.losses += 1;
            }
        }
    }
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(count(row, "rank"), index + 1);
        let expected = &totals[row["name"].as_str().unwrap()];
        for (field, value) in [
            ("games", expected.games),
            ("wins", expected.wins),
            ("draws", expected.draws),
            ("losses", expected.losses),
        ] {
            assert_eq!(count(row, field), value, "{} {field}", row["name"]);
        }
        near(number(row, "elo"), expected.elo);
        let points = expected.wins as f64 + expected.draws as f64 / 2.0;
        near(number(row, "points"), points);
        if expected.games == 0 {
            assert!(row["score_rate"].is_null());
        } else {
            near(number(row, "score_rate"), points / expected.games as f64);
        }
    }
    let mut expected_order = rows.clone();
    expected_order.sort_by(|a, b| {
        (count(a, "games") == 0)
            .cmp(&(count(b, "games") == 0))
            .then_with(|| {
                b["score_rate"]
                    .as_f64()
                    .unwrap_or(0.0)
                    .total_cmp(&a["score_rate"].as_f64().unwrap_or(0.0))
            })
            .then_with(|| count(b, "games").cmp(&count(a, "games")))
            .then_with(|| count(b, "wins").cmp(&count(a, "wins")))
            .then_with(|| a["name"].as_str().cmp(&b["name"].as_str()))
    });
    assert_eq!(
        rows, expected_order,
        "rank must use score rate, games, wins, then name"
    );
    near(
        rows.iter().map(|row| number(row, "elo")).sum(),
        1000.0 * rows.len() as f64,
    );
    assert_eq!(
        rows.iter().map(|row| count(row, "games")).sum::<usize>(),
        ledger.len() * 2
    );
    let csv = fs::read_to_string(path.join("leaderboard.csv")).unwrap();
    let mut lines = csv.lines();
    let headers: Vec<_> = lines
        .next()
        .unwrap()
        .split(',')
        .map(|s| s.trim_matches('"'))
        .collect();
    let exported: Vec<_> = lines.collect();
    assert_eq!(exported.len(), rows.len());
    for (line, row) in exported.iter().zip(&rows) {
        let fields: BTreeMap<_, _> = headers
            .iter()
            .copied()
            .zip(line.split(',').map(|s| s.trim_matches('"')))
            .collect();
        assert_eq!(fields["name"], row["name"].as_str().unwrap());
        for key in ["rank", "games", "wins", "draws", "losses"] {
            assert_eq!(fields[key].parse::<usize>().unwrap(), count(row, key));
        }
        for key in ["elo", "points", "score_rate"] {
            if row[key].is_null() {
                assert!(fields[key].is_empty());
            } else {
                assert!(
                    (fields[key].parse::<f64>().unwrap() - number(row, key)).abs() <= 0.01,
                    "CSV {key} disagrees with SQLite"
                );
            }
        }
    }
    (rows, ledger)
}

#[test]
fn elo_math_extremes_symmetry_and_validation() {
    for (score, expected) in [
        (0.0, (990.0, 1010.0)),
        (0.5, (1000.0, 1000.0)),
        (1.0, (1010.0, 990.0)),
    ] {
        assert_eq!(
            arena::elo_update(1000.0, 1000.0, score, 20.0).unwrap(),
            expected
        );
    }
    let (a, b) = arena::elo_update(1200.0, 1000.0, 1.0, 20.0).unwrap();
    near(a, 1204.8050614670408);
    near(b, 995.1949385329592);
    assert_eq!(
        arena::elo_update(1000.0, 1000.0, 1.0, 40.0).unwrap(),
        (1020.0, 980.0)
    );
    for (a, b) in [(-500.0, 900.0), (1333.3, 1234.5), (1e6, -1e6)] {
        for score in [0.0, 0.5, 1.0] {
            let (new_a, new_b) = arena::elo_update(a, b, score, 20.0).unwrap();
            let reverse = arena::elo_update(b, a, 1.0 - score, 20.0).unwrap();
            near(new_a, reverse.1);
            near(new_b, reverse.0);
            near(new_a + new_b, a + b);
        }
    }
    assert_eq!(
        arena::elo_update(1e6, -1e6, 0.0, 20.0).unwrap(),
        (999980.0, -999980.0)
    );
    for (a, b) in [(f64::MAX, -f64::MAX), (-f64::MAX, f64::MAX)] {
        for score in [0.0, 0.5, 1.0] {
            assert_eq!(arena::elo_update(a, b, score, 20.0).unwrap(), (a, b));
        }
    }
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(arena::elo_update(bad, 1000.0, 1.0, 20.0).is_err());
        assert!(arena::elo_update(1000.0, bad, 1.0, 20.0).is_err());
    }
    for bad in [-1.0, 0.1, 2.0, f64::NAN, f64::INFINITY] {
        assert!(arena::elo_update(1000.0, 1000.0, bad, 20.0).is_err());
    }
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(arena::elo_update(1000.0, 1000.0, 1.0, bad).is_err());
    }
}

#[test]
fn circle_schedule_exhaustively_covers_opponents_and_byes() {
    for n in 2..=19 {
        let names: Vec<String> = (0..n).map(|i| format!("entrant-{i}")).collect();
        let original = names.clone();
        let cycle_len = if n % 2 == 0 { n - 1 } else { n };
        let expected: BTreeSet<_> = (0..n)
            .flat_map(|a| (a + 1..n).map(move |b| (a, b)))
            .collect();
        for seed in [-917, 0, 37] {
            for cycle in 0..3 {
                let mut seen = BTreeSet::new();
                let mut byes = vec![0; n];
                for index in cycle * cycle_len..(cycle + 1) * cycle_len {
                    let pairs = arena::round_pairs(&names, seed, index).unwrap();
                    assert_eq!(pairs, arena::round_pairs(&names, seed, index).unwrap());
                    assert_eq!(pairs.len(), n / 2);
                    let mut present = BTreeSet::new();
                    for (a, b) in pairs {
                        let a = names.iter().position(|name| name == &a).unwrap();
                        let b = names.iter().position(|name| name == &b).unwrap();
                        assert!(present.insert(a));
                        assert!(present.insert(b));
                        assert!(
                            seen.insert((a.min(b), a.max(b))),
                            "repeated opponents before cycle ends"
                        );
                    }
                    for (i, bye) in byes.iter_mut().enumerate() {
                        if !present.contains(&i) {
                            *bye += 1;
                        }
                    }
                }
                assert_eq!(seen, expected);
                assert_eq!(byes, vec![n % 2; n]);
            }
        }
        assert_eq!(names, original);
    }
    for bad in [vec![], vec!["A"], vec!["A", "A"], vec!["A", ""]] {
        assert!(
            arena::round_pairs(
                &bad.into_iter().map(str::to_owned).collect::<Vec<_>>(),
                0,
                0
            )
            .is_err()
        );
    }
    let names = ["δ", "A", "B", "C", "D"].map(str::to_owned);
    let variants: BTreeSet<_> = (0..6)
        .map(|seed| {
            (0..5)
                .map(|i| arena::round_pairs(&names, seed, i).unwrap())
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(variants.len() > 1, "run seed must influence pairing");
}

#[test]
fn full_roster_has_all_parameters_and_fifty_round_fairness() {
    let temp = Temp::new();
    let path = temp.create("all", 0, 50, 1);
    let initial = standings(&path);
    let mut expected = BTreeSet::from([
        "RandomLegal[uniform]".to_owned(),
        "HonestFirst[B0-N0-C0]".to_owned(),
    ]);
    for b in (0..=100).step_by(10) {
        for n in (0..=100).step_by(10) {
            for c in (0..=100).step_by(10) {
                expected.insert(format!("MixedGreedy[B{b}-N{n}-C{c}]"));
            }
        }
    }
    let names: Vec<String> = initial
        .iter()
        .map(|row| row["name"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(names, expected.into_iter().collect::<Vec<_>>());
    assert_eq!(names.len(), 1333);
    for row in &initial {
        assert_eq!(count(row, "games"), 0);
        assert_eq!(number(row, "elo"), 1000.0);
        assert!(row["score_rate"].is_null());
    }
    let mut played: BTreeMap<_, usize> = names.iter().cloned().map(|name| (name, 0)).collect();
    let mut opponents = BTreeSet::new();
    for index in 0..50 {
        let pairs = arena::round_pairs(&names, 0, index).unwrap();
        assert_eq!(pairs.len(), 666);
        let mut present = BTreeSet::new();
        for (a, b) in pairs {
            assert!(present.insert(a.clone()));
            assert!(present.insert(b.clone()));
            let key = if a < b {
                (a.clone(), b.clone())
            } else {
                (b.clone(), a.clone())
            };
            assert!(opponents.insert(key));
            *played.get_mut(&a).unwrap() += 2;
            *played.get_mut(&b).unwrap() += 2;
        }
    }
    assert_eq!(played.values().sum::<usize>() / 2, 66600);
    assert_eq!(played.values().filter(|&&games| games == 98).count(), 50);
    assert_eq!(played.values().filter(|&&games| games == 100).count(), 1283);
    run(&path, 2, None, Some(1));
    let (rows, ledger) = audit(&path);
    assert_eq!(ledger.len(), 2);
    assert_eq!(
        rows.iter()
            .map(|row| count(row, "games"))
            .collect::<Vec<_>>(),
        [vec![2, 2], vec![0; 1331]].concat()
    );
}

#[test]
fn seventy_five_basic_rounds_balance_seats_and_draws() {
    let temp = Temp::new();
    let path = temp.create("basic", 173, 75, 1);
    run(&path, 3, None, None);
    let (rows, ledger) = audit(&path);
    assert_eq!(rows.len(), 3);
    assert_eq!(ledger.len(), 150);
    for row in &rows {
        assert_eq!(count(row, "games"), 100);
        assert_eq!(count(row, "draws"), 100);
        assert_eq!(number(row, "points"), 50.0);
        assert_eq!(number(row, "elo"), 1000.0);
        for seat in ["seat0", "seat1"] {
            assert_eq!(
                ledger
                    .iter()
                    .filter(|game| game[seat] == row["name"])
                    .count(),
                50
            );
        }
    }
    for game in &ledger {
        assert_eq!(count(game, "decisions"), 1);
        assert_eq!(game["reason"], "max_decisions");
    }
    let before = database_snapshot(&path);
    run(&path, 1, None, None);
    assert_eq!(
        database_snapshot(&path),
        before,
        "completed resume must not credit games again"
    );
}

#[test]
fn bounded_resume_and_extension_match_uninterrupted_real_games() {
    let temp = Temp::new();
    let reference = temp.create("basic", 20260913, 12, 1000);
    let resumed = temp.create("basic", 20260913, 6, 1000);
    run(&reference, 1, None, None);
    let (expected_rows, expected_ledger) = audit(&reference);
    assert!(expected_ledger.iter().any(|game| game["reason"] == "win"));
    run(&resumed, 2, None, Some(0));
    assert!(games(&resumed).is_empty());
    run(&resumed, 3, None, Some(2));
    assert_eq!(games(&resumed), expected_ledger[..4]);
    run(&resumed, 1, None, None);
    assert_eq!(games(&resumed), expected_ledger[..12]);
    run(&resumed, 4, Some(12), None);
    let (rows, ledger) = audit(&resumed);
    assert_eq!(ledger.len(), 24);
    assert_eq!(ledger, expected_ledger);
    assert_eq!(rows, expected_rows);
}

#[test]
fn invalid_api_settings_leave_filesystem_and_database_unchanged() {
    let temp = Temp::new();
    for (index, (roster, rounds, decisions)) in [("bad", 3, 1), ("basic", 0, 1), ("basic", 3, 0)]
        .into_iter()
        .enumerate()
    {
        let parent = temp.0.join(format!("bad-{index}"));
        assert!(arena::create_run(&parent, roster, 0, rounds, decisions).is_err());
        assert!(!parent.exists());
    }
    let path = temp.create("basic", 173, 3, 1);
    let before = database_snapshot(&path);
    for (workers, rounds) in [(0, None), (1, Some(0)), (1, Some(2))] {
        assert!(
            arena::run_arena(
                &path,
                RunOptions {
                    workers,
                    rounds,
                    quiet: true,
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert_eq!(database_snapshot(&path), before);
    }
    let missing = temp.0.join("missing");
    assert!(arena::run_arena(&missing, RunOptions::default()).is_err());
    assert!(!missing.exists());
}

#[test]
fn cli_needs_no_python_and_status_ignores_stale_csv() {
    let temp = Temp::new();
    succeeds(cli(&[
        "--runs-dir",
        temp.0.to_str().unwrap(),
        "--roster",
        "basic",
        "--rounds",
        "3",
        "--max-decisions",
        "1",
        "--seed",
        "-173",
        "--workers",
        "2",
    ]));
    let paths: Vec<_> = fs::read_dir(&temp.0)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(paths.len(), 1);
    let path = &paths[0];
    let (rows, ledger) = audit(path);
    assert_eq!(ledger.len(), 6);
    let before = database_snapshot(path);
    fs::write(path.join("leaderboard.csv"), "stale export\n").unwrap();
    let output = succeeds(cli(&["--status", path.to_str().unwrap(), "--top", "1"]));
    assert!(output.contains(rows[0]["name"].as_str().unwrap()));
    assert_eq!(standings(path), rows);
    assert_eq!(database_snapshot(path), before);
    assert_eq!(
        fs::read_to_string(path.join("leaderboard.csv")).unwrap(),
        "stale export\n"
    );
}

#[test]
fn invalid_cli_options_fail_before_any_mutation() {
    let temp = Temp::new();
    for (index, args) in [
        vec!["--workers", "0"],
        vec!["--workers", "-1"],
        vec!["--workers", "1.5"],
        vec!["--rounds", "0"],
        vec!["--rounds", "-1"],
        vec!["--max-decisions", "0"],
        vec!["--roster", "unknown"],
        vec!["--seed", "1.5"],
        vec!["--unknown-option"],
    ]
    .into_iter()
    .enumerate()
    {
        let parent = temp.0.join(format!("invalid-cli-{index}"));
        let mut full = vec!["--runs-dir", parent.to_str().unwrap()];
        full.extend(args);
        rejects(cli(&full));
        assert!(!parent.exists());
    }
    let path = temp.create("basic", 173, 3, 1);
    let path_text = path.to_str().unwrap();
    let before = database_snapshot(&path);
    for args in [
        vec!["--resume", path_text, "--status", path_text],
        vec!["--resume", path_text, "--seed", "19"],
        vec!["--resume", path_text, "--max-decisions", "2"],
        vec!["--resume", path_text, "--roster", "all"],
        vec!["--resume", path_text, "--rounds", "2"],
        vec!["--resume", path_text, "--workers", "0"],
    ] {
        rejects(cli(&args));
        assert_eq!(database_snapshot(&path), before);
    }
}
