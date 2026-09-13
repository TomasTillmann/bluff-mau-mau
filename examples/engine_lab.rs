//! Independent experimental matches; stdout is flushed JSONL, never an arena database.
use bluff_mau_mau::engines::{
    Bot,
    baseline::Baseline,
    matches::{MatchOptions, run_match},
    search::BeliefSearch,
    tactical::Tactical,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    io::{self, Write},
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::Instant,
};

#[derive(Clone)]
struct Options {
    bot: String,
    opponents: String,
    deals: usize,
    seed: i64,
    workers: usize,
    samples: usize,
    horizon: usize,
    challenge: u8,
}

impl Options {
    fn parse(args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut options = Self {
            bot: "champion".into(),
            opponents: "top".into(),
            deals: 100,
            seed: 100_000,
            workers: std::thread::available_parallelism().map_or(1, usize::from),
            samples: 32,
            horizon: 80,
            challenge: 0,
        };
        let mut args = args;
        while let Some(arg) = args.next() {
            let value = args
                .next()
                .ok_or_else(|| format!("Missing value for {arg}"))?;
            match arg.as_str() {
                "--bot" => options.bot = value,
                "--opponents" => options.opponents = value,
                "--deals" => options.deals = value.parse().map_err(|_| "Invalid deals")?,
                "--seed" => options.seed = value.parse().map_err(|_| "Invalid seed")?,
                "--workers" => options.workers = value.parse().map_err(|_| "Invalid workers")?,
                "--samples" => options.samples = value.parse().map_err(|_| "Invalid samples")?,
                "--horizon" => options.horizon = value.parse().map_err(|_| "Invalid horizon")?,
                "--challenge" => {
                    options.challenge = value.parse().map_err(|_| "Invalid challenge")?
                }
                _ => return Err(format!("Unknown option {arg}")),
            }
        }
        if !["champion", "tactical", "search"].contains(&options.bot.as_str()) {
            return Err("bot must be champion, tactical, or search".into());
        }
        if !["champion", "tactical", "top", "controls", "all"].contains(&options.opponents.as_str())
        {
            return Err(
                "opponents must be champion, tactical, top, controls, or all (both baseline sets)"
                    .into(),
            );
        }
        if [
            options.deals,
            options.workers,
            options.samples,
            options.horizon,
        ]
        .contains(&0)
        {
            return Err("deals, workers, samples, and horizon must be positive".into());
        }
        if options.challenge > 100 {
            return Err("challenge must be from 0 to 100".into());
        }
        let last_offset = i64::try_from(options.deals - 1).map_err(|_| "Too many deals")?;
        options
            .seed
            .checked_add(last_offset)
            .ok_or("Game seed overflow")?;
        options.deals.checked_mul(32_000).ok_or("Too many deals")?;
        Ok(options)
    }
}

fn roster(kind: &str) -> Vec<Box<dyn Bot>> {
    if kind == "champion" {
        return vec![Box::new(Baseline::mixed(0, 100, 40).unwrap())];
    }
    if kind == "tactical" {
        return vec![Box::new(Tactical::new(0).unwrap())];
    }
    let mut bots = Vec::new();
    if kind != "controls" {
        // The actual saved top ten, in docs/arena-ranking-2026-09-13.md order.
        for (b, n, c) in [
            (0, 100, 40),
            (0, 100, 30),
            (0, 90, 40),
            (0, 90, 30),
            (10, 90, 40),
            (10, 100, 40),
            (0, 80, 40),
            (10, 90, 30),
            (0, 100, 50),
            (0, 100, 20),
        ] {
            bots.push(Baseline::mixed(b, n, c).unwrap());
        }
    }
    if kind != "top" {
        bots.extend([Baseline::HonestFirst, Baseline::RandomLegal]);
        for (b, n, c) in [(0, 0, 0), (0, 100, 0), (0, 100, 100), (100, 100, 40)] {
            bots.push(Baseline::mixed(b, n, c).unwrap());
        }
    }
    bots.into_iter()
        .map(|bot| Box::new(bot) as Box<dyn Bot>)
        .collect()
}

// SplitMix64 derives independent role seeds; swapping seats preserves each bot's seed.
fn bot_seed(seed: i64, role: u64) -> i64 {
    let mut value = (seed as u64 ^ role).wrapping_add(0x9e3779b97f4a7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    (value ^ (value >> 31)) as i64
}

#[derive(Default, Debug, PartialEq, Eq)]
struct Counts {
    wins: usize,
    draws: usize,
    losses: usize,
    decisions: usize,
}
impl Counts {
    fn add(&mut self, other: &Self) {
        self.wins += other.wins;
        self.draws += other.draws;
        self.losses += other.losses;
        self.decisions += other.decisions;
    }
    fn games(&self) -> usize {
        self.wins + self.draws + self.losses
    }
    fn score(&self) -> f64 {
        (self.wins as f64 + self.draws as f64 * 0.5) / self.games() as f64
    }
}

fn seat_pair(candidate: &dyn Bot, opponent: &dyn Bot, seed: i64) -> Result<Counts, String> {
    let role_seeds = [
        bot_seed(seed, 0xd1b54a32d192ed03),
        bot_seed(seed, 0x94d049bb133111eb),
    ];
    let mut counts = Counts::default();
    for seat in 0..2 {
        let (bots, bot_seeds) = if seat == 0 {
            ([candidate, opponent], role_seeds)
        } else {
            ([opponent, candidate], [role_seeds[1], role_seeds[0]])
        };
        let result = run_match(
            bots,
            MatchOptions {
                seed,
                bot_seeds,
                max_decisions: 1000,
                ..Default::default()
            },
        )?;
        match result.winner() {
            Some(winner) if winner == seat => counts.wins += 1,
            Some(_) => counts.losses += 1,
            None => counts.draws += 1,
        }
        counts.decisions += result.decisions;
    }
    Ok(counts)
}

fn matchup(
    candidate: &dyn Bot,
    opponent: &dyn Bot,
    options: &Options,
) -> Result<(Counts, Vec<f64>), String> {
    let next = AtomicUsize::new(0);
    let mut totals = Counts::default();
    let mut pair_scores = vec![0.0; options.deals];
    std::thread::scope(|scope| -> Result<(), String> {
        let (sender, receiver) = mpsc::channel();
        for _ in 0..options.workers.min(options.deals) {
            let sender = sender.clone();
            let next = &next;
            scope.spawn(move || {
                loop {
                    let deal = next.fetch_add(1, Ordering::Relaxed);
                    if deal >= options.deals {
                        break;
                    }
                    let result = seat_pair(candidate, opponent, options.seed + deal as i64);
                    let failed = result.is_err();
                    if sender.send((deal, result)).is_err() || failed {
                        break;
                    }
                }
            });
        }
        drop(sender);
        let mut completed = 0;
        for (deal, result) in receiver {
            let counts = result.map_err(|error| {
                format!(
                    "{}; deal seed {}: {error}",
                    opponent.name(),
                    options.seed + deal as i64
                )
            })?;
            pair_scores[deal] = counts.score();
            totals.add(&counts);
            completed += 1;
        }
        if completed != options.deals {
            return Err("Not all requested deals completed".into());
        }
        Ok(())
    })?;
    Ok((totals, pair_scores))
}

fn interval(cluster_scores: &[f64]) -> Option<[f64; 2]> {
    if cluster_scores.len() < 2
        || cluster_scores
            .iter()
            .all(|score| *score == cluster_scores[0])
    {
        return None;
    }
    let n = cluster_scores.len() as f64;
    let mean = cluster_scores.iter().sum::<f64>() / n;
    let variance = cluster_scores
        .iter()
        .map(|score| (score - mean).powi(2))
        .sum::<f64>()
        / (n - 1.0);
    let margin = 1.96 * (variance / n).sqrt();
    Some([(mean - margin).max(0.0), (mean + margin).min(1.0)])
}

// Embed the evaluated implementation, so source edits after building cannot change provenance.
fn source_sha256() -> String {
    let mut hash = Sha256::new();
    for source in [
        include_str!("../Cargo.toml"),
        include_str!("../Cargo.lock"),
        include_str!("../build.rs"),
        include_str!("../src/game.rs"),
        include_str!("../src/rng.rs"),
        include_str!("../src/engines/observation.rs"),
        include_str!("../src/engines/matches.rs"),
        include_str!("../src/engines/baseline/mod.rs"),
        include_str!("../src/engines/baseline/greedy.rs"),
        include_str!("../src/engines/baseline/tactics.rs"),
        include_str!("../src/engines/tactical.rs"),
        include_str!("../src/engines/search.rs"),
        include_str!("engine_lab.rs"),
    ] {
        hash.update((source.len() as u64).to_le_bytes());
        hash.update(source.as_bytes());
    }
    format!("{:x}", hash.finalize())
}

fn write_json(value: serde_json::Value) -> Result<(), String> {
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{value}")
        .and_then(|_| stdout.flush())
        .map_err(|error| error.to_string())
}

fn report(
    kind: &str,
    opponent: &str,
    counts: &Counts,
    scores: &[f64],
    seconds: f64,
) -> serde_json::Value {
    let ci = interval(scores);
    let ci_method = if scores.len() < 2 {
        "normal CI unavailable: fewer than two seed clusters"
    } else if ci.is_none() {
        "normal CI unavailable for degenerate samples"
    } else {
        "normal approximation, seed-cluster standard error"
    };
    json!({
        "type": kind, "opponent": opponent, "games": counts.games(),
        "wins": counts.wins, "draws": counts.draws, "losses": counts.losses,
        "score_rate": counts.score(), "win_rate": counts.wins as f64 / counts.games() as f64,
        "score_ci95": ci, "ci_method": ci_method,
        "seed_clusters": scores.len(), "decisions": counts.decisions,
        "mean_decisions": counts.decisions as f64 / counts.games() as f64,
        "wall_seconds": seconds, "wall_ms_per_game": 1000.0 * seconds / counts.games() as f64,
        "truncated_games": counts.draws
    })
}

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!(
            "engine_lab [--bot champion|tactical|search] [--opponents champion|tactical|top|controls|all] [--deals 100] [--seed 100000] [--workers N] [--samples 32] [--horizon 80] [--challenge 0]\nEach deal plays both seats. 'all' combines top ten and six controls. JSONL stdout is flushed after every opponent; this does not open or update arena results. Games reaching 1000 decisions count as draws. CI clusters both seats, and all opponents in the aggregate, by deal seed."
        );
        return Ok(());
    }
    let options = Options::parse(args.into_iter())?;
    let candidate: Box<dyn Bot> = match options.bot.as_str() {
        "champion" => Box::new(Baseline::mixed(0, 100, 40)?),
        "tactical" => Box::new(Tactical::new(options.challenge)?),
        "search" => Box::new(BeliefSearch::new(options.samples, options.horizon)?),
        _ => unreachable!(),
    };
    let opponents = roster(&options.opponents);
    let names: Vec<_> = opponents.iter().map(|bot| bot.name()).collect();
    write_json(json!({
        "type": "config", "candidate": candidate.name(), "opponents": names,
        "deals": options.deals, "games_per_opponent": options.deals * 2,
        "first_seed": options.seed, "last_seed": options.seed + (options.deals - 1) as i64,
        "workers": options.workers.min(options.deals), "samples": options.samples,
        "horizon": options.horizon, "challenge": options.challenge, "max_decisions": 1000,
        "seed_scheme": "v1: consecutive game seeds; SplitMix64 role seeds; exchange seats and preserve role seeds",
        "source_sha256": source_sha256(), "rustc": env!("RUSTC_VERSION"), "arena_results_modified": false
    }))?;
    let started = Instant::now();
    let mut totals = Counts::default();
    let mut aggregate_scores = vec![0.0; options.deals];
    for opponent in &opponents {
        let matchup_started = Instant::now();
        let (counts, scores) = matchup(candidate.as_ref(), opponent.as_ref(), &options)?;
        for (aggregate, score) in aggregate_scores.iter_mut().zip(&scores) {
            *aggregate += score / opponents.len() as f64;
        }
        totals.add(&counts);
        let mut row = report(
            "matchup",
            &opponent.name(),
            &counts,
            &scores,
            matchup_started.elapsed().as_secs_f64(),
        );
        row["candidate"] = json!(candidate.name());
        row["first_seed"] = json!(options.seed);
        row["last_seed"] = json!(options.seed + (options.deals - 1) as i64);
        write_json(row)?;
    }
    let mut row = report(
        "aggregate",
        "equal-weight opponent set",
        &totals,
        &aggregate_scores,
        started.elapsed().as_secs_f64(),
    );
    row["candidate"] = json!(candidate.name());
    row["ci_scope"] = json!(
        "Shared deal seeds clustered across both seats and all opponents; opponents are a fixed selected set, not a random population"
    );
    write_json(row)
}

fn main() {
    if let Err(error) = run() {
        eprintln!("engine_lab: error: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paired_results_do_not_depend_on_workers() {
        let mut options = Options::parse(
            ["--deals", "8", "--workers", "1"]
                .map(str::to_owned)
                .into_iter(),
        )
        .unwrap();
        let candidate = Baseline::mixed(0, 100, 40).unwrap();
        let opponent = Baseline::mixed(0, 100, 30).unwrap();
        let serial = matchup(&candidate, &opponent, &options).unwrap();
        options.workers = 3;
        let parallel = matchup(&candidate, &opponent, &options).unwrap();
        assert_eq!(serial, parallel);
        assert_eq!(serial.0.games(), 16);
        assert_eq!(interval(&[0.0, 1.0]), Some([0.0, 1.0]));
        assert_eq!(interval(&[0.5]), None);
        assert_eq!(interval(&[1.0, 1.0]), None);
        assert_eq!(interval(&[0.0, 0.0]), None);
        assert_eq!(interval(&[0.5, 0.5]), None);
        let names: std::collections::BTreeSet<_> =
            roster("all").iter().map(|bot| bot.name()).collect();
        assert_eq!(names.len(), 16);
        assert_eq!(roster("champion")[0].name(), candidate.name());
        assert_eq!(
            roster("tactical")[0].name(),
            Tactical::new(0).unwrap().name()
        );
        assert_eq!(source_sha256().len(), 64);
        for args in [
            vec!["--deals", "0"],
            vec!["--workers", "0"],
            vec!["--bot", "unknown"],
            vec!["--challenge", "101"],
            vec!["--seed", "9223372036854775807", "--deals", "2"],
        ] {
            assert!(Options::parse(args.into_iter().map(str::to_owned)).is_err());
        }
    }
}
