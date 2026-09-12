//! Restartable, paired-seat baseline arena. One coordinator commits in schedule order.
use super::baseline::{Baseline, mixed_grid};
use super::matches::{MatchOptions, run_match};
use super::observation::Bot;
use crate::rng::PythonRandom;
use chrono::Local;
use fs2::FileExt;
use rusqlite::{Connection, OpenFlags, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, String>;
const FORMAT_VERSION: u32 = 2;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub roster: String,
    pub names: Vec<String>,
    pub seed: i64,
    pub rounds: usize,
    pub max_decisions: usize,
    pub fingerprint: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Standing {
    pub rank: usize,
    pub name: String,
    pub elo: f64,
    pub games: usize,
    pub wins: usize,
    pub draws: usize,
    pub losses: usize,
    pub points: f64,
    pub score_rate: Option<f64>,
}

#[derive(Clone)]
pub struct RunOptions {
    pub workers: usize,
    pub rounds: Option<usize>,
    pub max_pairs: Option<usize>,
    pub top: usize,
    /// Set by the CLI's Ctrl-C/SIGTERM handler; library callers can provide their own.
    pub stop: Arc<AtomicBool>,
    pub quiet: bool,
}
impl Default for RunOptions {
    fn default() -> Self {
        Self {
            workers: std::thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(1)
                .min(8),
            rounds: None,
            max_pairs: None,
            top: 20,
            stop: Arc::new(AtomicBool::new(false)),
            quiet: false,
        }
    }
}

pub fn baseline_roster(kind: &str) -> Result<Vec<Baseline>> {
    let mut bots = vec![Baseline::RandomLegal, Baseline::HonestFirst];
    match kind {
        "all" => bots.extend(mixed_grid()),
        "basic" => bots.push(Baseline::default()),
        _ => return Err("roster must be 'all' or 'basic'".into()),
    }
    Ok(bots)
}

pub fn derived_seed(seed: i64, domain: &str) -> u64 {
    let digest = Sha256::digest(format!("{domain}:{seed}").as_bytes());
    u64::from_be_bytes(digest[..8].try_into().unwrap()) & i64::MAX as u64
}

pub fn round_pairs(
    names: &[String],
    seed: i64,
    round_index: usize,
) -> Result<Vec<(String, String)>> {
    if names.len() < 2
        || names.iter().any(String::is_empty)
        || names.iter().collect::<BTreeSet<_>>().len() != names.len()
    {
        return Err("Provide at least two unique nonempty string names".into());
    }
    let mut sorted = names.to_vec();
    sorted.sort();
    let mut circle: Vec<Option<String>> = sorted.into_iter().map(Some).collect();
    if circle.len() % 2 == 1 {
        circle.push(None);
    }
    let cycle = round_index / (circle.len() - 1);
    let offset = round_index % (circle.len() - 1);
    PythonRandom::seed(derived_seed(seed, &format!("circle:{cycle}"))).shuffle(&mut circle);
    circle[1..].rotate_right(offset);
    let half = circle.len() / 2;
    let mut pairs: Vec<_> = circle[..half]
        .iter()
        .zip(circle[half..].iter().rev())
        .filter_map(|(a, b)| Some((a.as_ref()?.clone(), b.as_ref()?.clone())))
        .collect();
    PythonRandom::seed(derived_seed(seed, &format!("pairs:{round_index}"))).shuffle(&mut pairs);
    Ok(pairs)
}

pub fn elo_update(a: f64, b: f64, score: f64, k: f64) -> Result<(f64, f64)> {
    if ![a, b, score, k].iter().all(|value| value.is_finite())
        || ![0.0, 0.5, 1.0].contains(&score)
        || k <= 0.0
    {
        return Err(
            "Ratings must be finite, score must be 0, 0.5, or 1, and K must be positive".into(),
        );
    }
    let difference = (b - a) / 400.0;
    let power = 10f64.powf(-difference.abs());
    let expected = if difference >= 0.0 {
        power / (1.0 + power)
    } else {
        1.0 / (1.0 + power)
    };
    let delta = k * (score - expected);
    Ok((a + delta, b - delta))
}

fn fingerprint() -> Result<serde_json::Value> {
    // Hash bytes included at compilation, so editing source cannot relabel an old
    // executable and CLI/library callers of the same compiled engine can interoperate.
    let sources: &[(&str, &[u8])] = &[
        ("game", include_bytes!("../game.rs")),
        ("rng", include_bytes!("../rng.rs")),
        ("observation", include_bytes!("observation.rs")),
        ("matches", include_bytes!("matches.rs")),
        ("arena", include_bytes!("arena.rs")),
        ("baseline", include_bytes!("baseline/mod.rs")),
        ("greedy", include_bytes!("baseline/greedy.rs")),
        ("tactics", include_bytes!("baseline/tactics.rs")),
        ("manifest", include_bytes!("../../Cargo.toml")),
        ("lockfile", include_bytes!("../../Cargo.lock")),
    ];
    let hashes: BTreeMap<_, _> = sources
        .iter()
        .map(|(name, bytes)| (*name, format!("{:x}", Sha256::digest(bytes))))
        .collect();
    Ok(
        serde_json::json!({ "format": FORMAT_VERSION, "engine": "rust", "version": env!("CARGO_PKG_VERSION"),
        "platform": [std::env::consts::OS, std::env::consts::ARCH], "compiler": option_env!("RUSTC_VERSION").unwrap_or("unknown"), "sources": hashes }),
    )
}

fn target_pairs(config: &Config) -> Result<usize> {
    config
        .rounds
        .checked_mul(config.names.len() / 2)
        .filter(|pairs| *pairs <= i64::MAX as usize / 2)
        .ok_or_else(|| "Arena target exceeds the ledger integer range".into())
}

fn connect(path: &Path, readonly: bool) -> Result<Connection> {
    let flags = if readonly {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    } else {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    };
    let connection =
        Connection::open_with_flags(path.join("arena.sqlite3"), flags).map_err(|error| {
            format!(
                "Invalid or unavailable arena database at {}: {error}",
                path.display()
            )
        })?;
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    Ok(connection)
}

fn read_config(connection: &Connection) -> Result<Config> {
    let text: String = connection
        .query_row("SELECT value FROM config WHERE key='run'", [], |row| {
            row.get(0)
        })
        .map_err(|e| format!("Invalid arena configuration: {e}"))?;
    let config: Config =
        serde_json::from_str(&text).map_err(|e| format!("Invalid arena configuration: {e}"))?;
    let names: Vec<_> = baseline_roster(&config.roster)?
        .iter()
        .map(Bot::name)
        .collect();
    if config.names != names
        || config.rounds == 0
        || config.max_decisions == 0
        || config.max_decisions > i64::MAX as usize
    {
        return Err("Invalid arena configuration or stored roster".into());
    }
    target_pairs(&config)?;
    Ok(config)
}

pub fn config(path: &Path) -> Result<Config> {
    read_config(&connect(path, true)?)
}

pub fn create_run(
    parent: &Path,
    roster: &str,
    seed: i64,
    rounds: usize,
    max_decisions: usize,
) -> Result<PathBuf> {
    let names = baseline_roster(roster)?.iter().map(Bot::name).collect();
    if rounds == 0 || max_decisions == 0 || max_decisions > i64::MAX as usize {
        return Err("rounds and max_decisions must be positive".into());
    }
    let config = Config {
        roster: roster.into(),
        names,
        seed,
        rounds,
        max_decisions,
        fingerprint: fingerprint()?,
    };
    target_pairs(&config)?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let path = loop {
        let path = parent.join(
            Local::now()
                .format("%Y%m%d-%H%M%S-%6f-rust-arena")
                .to_string(),
        );
        match fs::create_dir(&path) {
            Ok(()) => break path,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        }
    };
    let mut connection = Connection::open(path.join("arena.sqlite3")).map_err(|e| e.to_string())?;
    connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
        CREATE TABLE config (key TEXT PRIMARY KEY, value TEXT NOT NULL);
        CREATE TABLE players (name TEXT PRIMARY KEY, elo REAL NOT NULL DEFAULT 1000,
          games INTEGER NOT NULL DEFAULT 0, wins INTEGER NOT NULL DEFAULT 0,
          draws INTEGER NOT NULL DEFAULT 0, losses INTEGER NOT NULL DEFAULT 0);
        CREATE TABLE games (id INTEGER PRIMARY KEY, round_index INTEGER NOT NULL,
          seat0 TEXT NOT NULL REFERENCES players(name), seat1 TEXT NOT NULL REFERENCES players(name),
          seed INTEGER NOT NULL, bot_seed0 INTEGER NOT NULL, bot_seed1 INTEGER NOT NULL,
          winner INTEGER CHECK(winner IN (0,1)), reason TEXT NOT NULL CHECK(reason IN ('win','max_decisions')),
          decisions INTEGER NOT NULL);").map_err(|e| e.to_string())?;
    let transaction = connection.transaction().map_err(|e| e.to_string())?;
    transaction
        .execute(
            "INSERT INTO config VALUES ('run', ?)",
            [serde_json::to_string(&config).map_err(|e| e.to_string())?],
        )
        .map_err(|e| e.to_string())?;
    for name in &config.names {
        transaction
            .execute("INSERT INTO players(name) VALUES (?)", [name])
            .map_err(|e| e.to_string())?;
    }
    transaction.commit().map_err(|e| e.to_string())?;
    File::create(path.join(".arena.lock")).map_err(|e| e.to_string())?;
    path.canonicalize().map_err(|e| e.to_string())
}

fn sql_usize(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<usize> {
    let value: i64 = row.get(index)?;
    usize::try_from(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Integer,
            Box::new(error),
        )
    })
}

fn read_standings(connection: &Connection) -> Result<Vec<Standing>> {
    let mut statement = connection
        .prepare("SELECT name,elo,games,wins,draws,losses FROM players")
        .map_err(|e| e.to_string())?;
    let mut rows: Vec<Standing> = statement
        .query_map([], |row| {
            let games = sql_usize(row, 2)?;
            let wins = sql_usize(row, 3)?;
            let draws = sql_usize(row, 4)?;
            let points = wins as f64 + draws as f64 / 2.0;
            Ok(Standing {
                rank: 0,
                name: row.get(0)?,
                elo: row.get(1)?,
                games,
                wins,
                draws,
                losses: sql_usize(row, 5)?,
                points,
                score_rate: if games == 0 {
                    None
                } else {
                    Some(points / games as f64)
                },
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<std::result::Result<_, _>>()
        .map_err(|e| e.to_string())?;
    rows.sort_by(|a, b| {
        b.score_rate
            .unwrap_or(-1.0)
            .total_cmp(&a.score_rate.unwrap_or(-1.0))
            .then_with(|| b.games.cmp(&a.games))
            .then_with(|| b.wins.cmp(&a.wins))
            .then_with(|| a.name.cmp(&b.name))
    });
    for (index, row) in rows.iter_mut().enumerate() {
        row.rank = index + 1;
    }
    Ok(rows)
}

pub fn standings(path: &Path) -> Result<Vec<Standing>> {
    read_standings(&connect(path, true)?)
}

fn export(path: &Path, rows: &[Standing]) -> Result<()> {
    let temporary = path.join(format!(".leaderboard-{}.csv", std::process::id()));
    let outcome = (|| {
        let mut output = File::create(&temporary).map_err(|e| e.to_string())?;
        writeln!(
            output,
            "rank,name,elo,games,wins,draws,losses,points,score_rate"
        )
        .map_err(|e| e.to_string())?;
        for row in rows {
            let name = format!("\"{}\"", row.name.replace('"', "\"\""));
            writeln!(
                output,
                "{},{},{},{},{},{},{},{},{}",
                row.rank,
                name,
                row.elo,
                row.games,
                row.wins,
                row.draws,
                row.losses,
                row.points,
                row.score_rate
                    .map(|value| value.to_string())
                    .unwrap_or_default()
            )
            .map_err(|e| e.to_string())?;
        }
        output.sync_all().map_err(|e| e.to_string())?;
        fs::rename(&temporary, path.join("leaderboard.csv")).map_err(|e| e.to_string())?;
        Ok(())
    })();
    let _ = fs::remove_file(temporary);
    outcome
}

pub fn print_standings(path: &Path, config: &Config, rows: &[Standing], top: usize) {
    let games: usize = rows.iter().map(|row| row.games).sum::<usize>() / 2;
    println!(
        "\nRun: {}\nGames: {}/{} | paired rounds target: {}",
        path.display(),
        games,
        config.rounds * (rows.len() / 2) * 2,
        config.rounds
    );
    println!(
        "{:>4}  {:<30} {:>7} {:>8} {:>7} {:>6} {:>6} {:>6} {:>9}",
        "#", "Name", "Score", "Points", "Games", "W", "D", "L", "Elo"
    );
    for row in rows.iter().take(top) {
        let rate = row
            .score_rate
            .map(|value| format!("{:.1}%", value * 100.0))
            .unwrap_or_else(|| "-".into());
        println!(
            "{:>4}  {:<30} {:>7} {:>8.1} {:>7} {:>6} {:>6} {:>6} {:>9.2}",
            row.rank,
            row.name,
            rate,
            row.points,
            row.games,
            row.wins,
            row.draws,
            row.losses,
            row.elo
        );
    }
    println!("Baseline comparison: early scores and differing opponent samples are noisy.");
}

#[derive(Clone)]
struct Job {
    pair_id: usize,
    round_index: usize,
    names: [String; 2],
    bots: [Baseline; 2],
    seed: i64,
    bot_seeds: [i64; 2],
}
type PairResult = [(Option<usize>, usize); 2];

struct Jobs<'a> {
    config: &'a Config,
    next: usize,
    end: usize,
    current_round: Option<usize>,
    pairs: Vec<(String, String)>,
    roster: BTreeMap<String, Baseline>,
    game_offset: u64,
}
impl<'a> Jobs<'a> {
    fn new(config: &'a Config, start: usize, end: usize) -> Result<Self> {
        Ok(Self {
            config,
            next: start,
            end,
            current_round: None,
            pairs: vec![],
            roster: baseline_roster(&config.roster)?
                .into_iter()
                .map(|bot| (bot.name(), bot))
                .collect(),
            game_offset: derived_seed(config.seed, "games"),
        })
    }
    fn next_job(&mut self) -> Result<Option<Job>> {
        if self.next == self.end {
            return Ok(None);
        }
        let pair_id = self.next;
        self.next += 1;
        let per_round = self.config.names.len() / 2;
        let round_index = pair_id / per_round;
        if self.current_round != Some(round_index) {
            self.pairs = round_pairs(&self.config.names, self.config.seed, round_index)?;
            self.current_round = Some(round_index);
        }
        let pair = &self.pairs[pair_id % per_round];
        Ok(Some(Job {
            pair_id,
            round_index,
            names: [pair.0.clone(), pair.1.clone()],
            bots: [self.roster[&pair.0], self.roster[&pair.1]],
            seed: ((self.game_offset + pair_id as u64) & i64::MAX as u64) as i64,
            bot_seeds: [
                derived_seed(self.config.seed, &format!("bot0:{pair_id}")) as i64,
                derived_seed(self.config.seed, &format!("bot1:{pair_id}")) as i64,
            ],
        }))
    }
}

fn play_pair(job: &Job, max_decisions: usize) -> Result<PairResult> {
    let mut results = [(None, 0); 2];
    for (leg, seats) in [[0, 1], [1, 0]].iter().enumerate() {
        let result = run_match(
            [&job.bots[seats[0]], &job.bots[seats[1]]],
            MatchOptions {
                seed: job.seed,
                bot_seeds: job.bot_seeds,
                max_decisions,
                ..Default::default()
            },
        )?;
        results[leg] = (result.winner(), result.decisions);
    }
    Ok(results)
}

fn commit_pair(connection: &mut Connection, job: &Job, results: PairResult) -> Result<()> {
    let transaction = connection.transaction().map_err(|e| e.to_string())?;
    for (leg, (winner, decisions)) in results.into_iter().enumerate() {
        let seats = if leg == 0 {
            [&job.names[0], &job.names[1]]
        } else {
            [&job.names[1], &job.names[0]]
        };
        let a: f64 = transaction
            .query_row("SELECT elo FROM players WHERE name=?", [seats[0]], |row| {
                row.get(0)
            })
            .map_err(|e| e.to_string())?;
        let b: f64 = transaction
            .query_row("SELECT elo FROM players WHERE name=?", [seats[1]], |row| {
                row.get(0)
            })
            .map_err(|e| e.to_string())?;
        let score = winner
            .map(|seat| if seat == 0 { 1.0 } else { 0.0 })
            .unwrap_or(0.5);
        let updated = elo_update(a, b, score, 20.0)?;
        transaction
            .execute(
                "INSERT INTO games VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    (2 * job.pair_id + leg) as i64,
                    job.round_index as i64,
                    seats[0],
                    seats[1],
                    job.seed,
                    job.bot_seeds[0],
                    job.bot_seeds[1],
                    winner.map(|seat| seat as i64),
                    if winner.is_none() {
                        "max_decisions"
                    } else {
                        "win"
                    },
                    decisions as i64
                ],
            )
            .map_err(|e| e.to_string())?;
        for (seat, name) in seats.into_iter().enumerate() {
            transaction.execute("UPDATE players SET elo=?, games=games+1,wins=wins+?,draws=draws+?,losses=losses+? WHERE name=?", params![if seat == 0 { updated.0 } else { updated.1 }, winner == Some(seat), winner.is_none(), winner == Some(1 - seat), name]).map_err(|e| e.to_string())?;
        }
    }
    transaction.commit().map_err(|e| e.to_string())
}

fn validate_resume(
    config: &Config,
    rounds: Option<usize>,
    current_fingerprint: &serde_json::Value,
) -> Result<()> {
    if rounds.is_some_and(|rounds| rounds == 0 || rounds < config.rounds) {
        return Err("rounds must be positive and cannot shrink the stored target".into());
    }
    if &config.fingerprint != current_fingerprint {
        return Err("Arena source/compiler/format fingerprint mismatch; create a new run. Python runs remain readable but cannot be resumed by Rust.".into());
    }
    Ok(())
}

pub fn run_arena(path: &Path, options: RunOptions) -> Result<Vec<Standing>> {
    if options.workers == 0 || options.top == 0 || options.workers.checked_mul(8).is_none() {
        return Err("workers and top must be positive, representable integers".into());
    }
    let current_fingerprint = fingerprint()?;
    validate_resume(&config(path)?, options.rounds, &current_fingerprint)?;
    let lock = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path.join(".arena.lock"))
        .map_err(|e| e.to_string())?;
    lock.try_lock_exclusive().map_err(|error| {
        format!(
            "Another arena writer is already running, or writer lock unavailable at {}: {error}",
            path.display()
        )
    })?;
    let mut connection = connect(path, false)?;
    let mut config = read_config(&connection)?;
    validate_resume(&config, options.rounds, &current_fingerprint)?;
    let (count, first, last): (i64, Option<i64>, Option<i64>) = connection
        .query_row("SELECT count(*),min(id),max(id) FROM games", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .map_err(|e| e.to_string())?;
    if count % 2 != 0
        || count > 0 && (first != Some(0) || last != Some(count - 1))
        || count as usize > target_pairs(&config)? * 2
    {
        return Err("Arena ledger is not a contiguous prefix within its stored target".into());
    }
    let player_names: BTreeSet<String> = read_standings(&connection)?
        .into_iter()
        .map(|row| row.name)
        .collect();
    if player_names != config.names.iter().cloned().collect() {
        return Err("Arena player records do not match stored roster".into());
    }
    connection
        .execute_batch("PRAGMA synchronous=FULL")
        .map_err(|e| e.to_string())?;
    if let Some(rounds) = options.rounds {
        config.rounds = rounds;
        target_pairs(&config)?;
        connection
            .execute(
                "UPDATE config SET value=? WHERE key='run'",
                [serde_json::to_string(&config).map_err(|e| e.to_string())?],
            )
            .map_err(|e| e.to_string())?;
    }
    let start = count as usize / 2;
    let end = options
        .max_pairs
        .map(|limit| {
            start
                .saturating_add(limit)
                .min(target_pairs(&config).unwrap())
        })
        .unwrap_or(target_pairs(&config)?);
    let report = |connection: &Connection| -> Result<Vec<Standing>> {
        let rows = read_standings(connection)?;
        export(path, &rows)?;
        if !options.quiet {
            print_standings(path, &config, &rows, options.top);
        }
        Ok(rows)
    };
    report(&connection)?;
    let mut last_report = Instant::now();
    let mut jobs = Jobs::new(&config, start, end)?;
    let outcome: Result<()> = if options.workers == 1 {
        (|| {
            while !options.stop.load(Ordering::Relaxed) {
                let Some(job) = jobs.next_job()? else { break };
                let results = play_pair(&job, config.max_decisions)?;
                commit_pair(&mut connection, &job, results)?;
                if last_report.elapsed() >= Duration::from_secs(5) {
                    report(&connection)?;
                    last_report = Instant::now();
                }
            }
            Ok(())
        })()
    } else {
        std::thread::scope(|scope| {
            let (sender, receiver) = mpsc::channel::<Job>();
            let receiver = Arc::new(Mutex::new(receiver));
            let (completed_sender, completed_receiver) =
                mpsc::channel::<(usize, Result<PairResult>)>();
            let cancel = Arc::new(AtomicBool::new(false));
            let worker_count = options.workers.min(end.saturating_sub(start));
            let mut handles = Vec::new();
            for _ in 0..worker_count {
                let receiver = Arc::clone(&receiver);
                let completed = completed_sender.clone();
                let cancel = Arc::clone(&cancel);
                let max_decisions = config.max_decisions;
                handles.push(scope.spawn(move || {
                    while !cancel.load(Ordering::Relaxed) {
                        let job = match receiver.lock() {
                            Ok(queue) => match queue.recv() {
                                Ok(job) => job,
                                Err(_) => break,
                            },
                            Err(_) => break,
                        };
                        if cancel.load(Ordering::Relaxed) {
                            break;
                        }
                        let result = std::panic::catch_unwind(|| play_pair(&job, max_decisions))
                            .unwrap_or_else(|_| {
                                Err("Arena worker panicked; pair remains uncommitted".into())
                            });
                        if completed.send((job.pair_id, result)).is_err() {
                            break;
                        }
                    }
                }));
            }
            drop(completed_sender);
            let result =
                (|| {
                    let mut pending = VecDeque::new();
                    let mut ready = BTreeMap::new();
                    loop {
                        while !options.stop.load(Ordering::Relaxed)
                            && pending.len() < 8 * options.workers
                        {
                            let Some(job) = jobs.next_job()? else { break };
                            sender.send(job.clone()).map_err(|e| e.to_string())?;
                            pending.push_back(job);
                        }
                        let Some(front) = pending.front() else { break };
                        let expected = front.pair_id;
                        if let Some(result) = ready.remove(&expected) {
                            let job = pending.pop_front().unwrap();
                            commit_pair(&mut connection, &job, result?)?;
                            if options.stop.load(Ordering::Relaxed) {
                                break;
                            }
                        } else {
                            match completed_receiver.recv_timeout(Duration::from_millis(200)) {
                                Ok((pair_id, result)) => {
                                    if result.is_err() {
                                        result?;
                                    } else {
                                        ready.insert(pair_id, result);
                                    }
                                }
                                Err(mpsc::RecvTimeoutError::Timeout) => {}
                                Err(mpsc::RecvTimeoutError::Disconnected) => return Err(
                                    "Arena workers disconnected; uncommitted pairs remain pending"
                                        .into(),
                                ),
                            }
                        }
                        if last_report.elapsed() >= Duration::from_secs(5) {
                            report(&connection)?;
                            last_report = Instant::now();
                        }
                    }
                    Ok(())
                })();
            cancel.store(true, Ordering::Relaxed);
            drop(sender);
            drop(completed_receiver);
            let mut panicked = false;
            for handle in handles {
                if handle.join().is_err() {
                    panicked = true;
                }
            }
            if panicked && result.is_ok() {
                Err("Arena worker panicked; uncommitted pairs remain pending".into())
            } else {
                result
            }
        })
    };
    let rows = report(&connection)?;
    if options.stop.load(Ordering::Relaxed) && !options.quiet {
        println!("Stopped cleanly; resume this run to continue.");
    }
    outcome.map_err(|error| format!("Arena stopped; uncommitted pairs remain pending: {error}"))?;
    Ok(rows)
}
