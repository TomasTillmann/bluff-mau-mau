//! Held-out full-game evaluation of an immutable trained average policy; no arena writes.
use bluff_mau_mau::{
    engines::{
        Bot,
        baseline::Baseline,
        mccfr::{SampledGame, SampledNode, Trainer},
        new_knowledge,
        solving::{RulesGame, Scenario},
        training_store::RunStore,
    },
    game::new_game,
    rng::PythonRandom,
};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::Instant,
};

const FIRST_SEED: i64 = 3_000_000;
const DEALS: i64 = 50;

// Frozen published top ten and their displayed Elo, docs/arena-ranking-2026-09-13.md.
fn opponents() -> Vec<(Baseline, f64)> {
    [
        (0, 100, 40, 1356.09),
        (0, 100, 30, 1387.65),
        (0, 90, 40, 1365.75),
        (0, 90, 30, 1344.59),
        (10, 90, 40, 1246.40),
        (10, 100, 40, 1342.60),
        (0, 80, 40, 1360.71),
        (10, 90, 30, 1415.33),
        (0, 100, 50, 1405.16),
        (0, 100, 20, 1399.22),
    ]
    .into_iter()
    .map(|(b, n, c, elo)| (Baseline::mixed(b, n, c).unwrap(), elo))
    .collect()
}

fn bot_seed(seed: i64, role: u64) -> i64 {
    let mut value = (seed as u64 ^ role).wrapping_add(0x9e3779b97f4a7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    (value ^ (value >> 31)) as i64
}

#[derive(Serialize)]
struct GameResult {
    opponent: String,
    opponent_elo: f64,
    deal_seed: i64,
    candidate_seat: usize,
    candidate_rng_seed: i64,
    opponent_rng_seed: i64,
    winner: Option<usize>,
    decisions: usize,
    candidate_decisions: usize,
    known_information_set_decisions: usize,
    truncated: bool,
}

#[derive(Default, Serialize)]
struct Counts {
    wins: usize,
    draws: usize,
    losses: usize,
    decisions: usize,
    candidate_decisions: usize,
    known_information_set_decisions: usize,
}
impl Counts {
    fn record(&mut self, result: &GameResult) {
        match result.winner {
            Some(winner) if winner == result.candidate_seat => self.wins += 1,
            Some(_) => self.losses += 1,
            None => self.draws += 1,
        }
        self.decisions += result.decisions;
        self.candidate_decisions += result.candidate_decisions;
        self.known_information_set_decisions += result.known_information_set_decisions;
    }
    fn games(&self) -> usize {
        self.wins + self.draws + self.losses
    }
    fn points(&self) -> f64 {
        self.wins as f64 + 0.5 * self.draws as f64
    }
    fn metrics(&self) -> serde_json::Value {
        json!({"games":self.games(),"wins":self.wins,"draws":self.draws,"losses":self.losses,
            "points":self.points(),"score_rate":self.points()/self.games() as f64,
            "win_rate":self.wins as f64/self.games() as f64,
            "win_loss_ratio":(self.losses>0).then(||self.wins as f64/self.losses as f64),
            "candidate_decisions":self.candidate_decisions,
            "known_information_set_decisions":self.known_information_set_decisions,
            "lookup_coverage":(self.candidate_decisions>0).then(||self.known_information_set_decisions as f64/self.candidate_decisions as f64),
            "decisions":self.decisions,"truncated_games":self.draws})
    }
}

fn play(
    trainer: &Trainer,
    opponent: &Baseline,
    elo: f64,
    seed: i64,
    seat: usize,
) -> Result<GameResult, String> {
    let state = new_game(seed, 0)?;
    let knowledge = new_knowledge(state.top);
    let game = RulesGame::from_scenarios(
        vec![Scenario {
            state,
            knowledge,
            probability: 1.0,
        }],
        1000,
        0.0,
    )?;
    let mut chance = PythonRandom::seed(0); // Fixed scenario retains the core game's own RNG.
    let mut state = game.start(&mut chance)?;
    let candidate_seed = bot_seed(seed, 0xd1b54a32d192ed03);
    let opponent_seed = bot_seed(seed, 0x94d049bb133111eb);
    let mut candidate_rng = PythonRandom::seed_signed(candidate_seed);
    let mut opponent_rng = PythonRandom::seed_signed(opponent_seed);
    let (mut decisions, mut candidate_decisions, mut known) = (0, 0, 0);
    let winner = loop {
        match game.node(&state)? {
            SampledNode::Terminal(value) => {
                break if value > 0.0 {
                    Some(0)
                } else if value < 0.0 {
                    Some(1)
                } else {
                    None
                };
            }
            SampledNode::Decision {
                player,
                key,
                actions,
            } => {
                let action = if player == seat {
                    candidate_decisions += 1;
                    known += usize::from(trainer.has_information_set(player, &key));
                    let policy = trainer.average_strategy(player, &key, actions)?;
                    let draw = candidate_rng.random();
                    let mut cumulative = 0.0;
                    policy
                        .iter()
                        .position(|probability| {
                            cumulative += probability;
                            draw < cumulative
                        })
                        .unwrap_or_else(|| policy.iter().rposition(|p| *p > 0.0).unwrap())
                } else {
                    let selected = opponent.choose(
                        &state.observation(),
                        state.legal_actions(),
                        &mut opponent_rng,
                    )?;
                    state
                        .legal_actions()
                        .iter()
                        .position(|action| *action == selected)
                        .ok_or("Opponent returned an illegal action")?
                };
                game.advance(&mut state, action, &mut chance)?;
                decisions += 1;
            }
        }
    };
    Ok(GameResult {
        opponent: opponent.name(),
        opponent_elo: elo,
        deal_seed: seed,
        candidate_seat: seat,
        candidate_rng_seed: candidate_seed,
        opponent_rng_seed: opponent_seed,
        winner,
        decisions,
        candidate_decisions,
        known_information_set_decisions: known,
        truncated: winner.is_none(),
    })
}

// Fit a single Elo against fixed opponent ratings; match order cannot affect the answer.
fn performance_elo(opponents: &[(f64, usize)], points: f64) -> Option<f64> {
    let games: usize = opponents.iter().map(|(_, n)| n).sum();
    if games == 0 || points <= 0.0 || points >= games as f64 {
        return None;
    }
    let mut lower = opponents
        .iter()
        .map(|(elo, _)| *elo)
        .fold(f64::INFINITY, f64::min)
        - 10000.0;
    let mut upper = opponents
        .iter()
        .map(|(elo, _)| *elo)
        .fold(f64::NEG_INFINITY, f64::max)
        + 10000.0;
    for _ in 0..100 {
        let rating = (lower + upper) * 0.5;
        let expected: f64 = opponents
            .iter()
            .map(|(elo, n)| *n as f64 / (1.0 + 10f64.powf((elo - rating) / 400.0)))
            .sum();
        if expected < points {
            lower = rating;
        } else {
            upper = rating;
        }
    }
    Some((lower + upper) * 0.5)
}

fn new_file(path: &Path) -> Result<File, String> {
    OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| format!("{}: {error}", path.display()))
}
fn emit(file: &mut File, value: &serde_json::Value) -> Result<(), String> {
    writeln!(file, "{value}")
        .and_then(|_| file.flush())
        .and_then(|_| file.sync_data())
        .map_err(|error| error.to_string())
}
fn source_hash() -> String {
    let mut hash = Sha256::new();
    for source in [
        include_str!("evaluate_trained.rs"),
        include_str!("../src/engines/mccfr.rs"),
        include_str!("../src/engines/solving.rs"),
        include_str!("../src/engines/observation.rs"),
        include_str!("../src/engines/baseline/mod.rs"),
        include_str!("../src/engines/baseline/greedy.rs"),
        include_str!("../src/engines/baseline/tactics.rs"),
        include_str!("../src/game.rs"),
        include_str!("../src/rng.rs"),
        include_str!("../Cargo.toml"),
        include_str!("../Cargo.lock"),
    ] {
        hash.update((source.len() as u64).to_le_bytes());
        hash.update(source);
    }
    format!("{:x}", hash.finalize())
}

fn run() -> Result<(), String> {
    let mut checkpoint = PathBuf::from("runs/20260913-mccfr-final-fresh-d32-s42");
    let mut output = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--help" || arg == "-h" {
            println!(
                "evaluate_trained --output DIRECTORY [--run CHECKPOINT_DIRECTORY]\nEvaluates the immutable average policy against the saved top ten, 50 held-out deals in both seats (100 games per opponent). Writes new mccfr-games.jsonl, mccfr-summary.json, and mccfr-results.md; never updates arena ratings."
            );
            return Ok(());
        }
        let value = args
            .next()
            .ok_or_else(|| format!("Missing value for {arg}"))?;
        match arg.as_str() {
            "--run" => checkpoint = PathBuf::from(value),
            "--output" => output = Some(PathBuf::from(value)),
            _ => return Err(format!("Unknown option {arg}")),
        }
    }
    let output = output.ok_or("--output DIRECTORY is required")?;
    let identity =
        fs::read_to_string(checkpoint.join("config.json")).map_err(|error| error.to_string())?;
    let training_model: serde_json::Value =
        serde_json::from_str(&identity).map_err(|error| error.to_string())?;
    let store = RunStore::open(&checkpoint, &identity)?;
    let bytes = store.load()?.ok_or("No trained checkpoint found")?;
    let checkpoint_sha256 = format!("{:x}", Sha256::digest(&bytes));
    let trainer = Trainer::from_bytes(&bytes)?;
    fs::create_dir_all(&output).map_err(|error| error.to_string())?;
    let mut records = new_file(&output.join("mccfr-games.jsonl"))?;
    let config = json!({"type":"config","candidate":"MCCFR average policy","checkpoint":checkpoint,
        "checkpoint_payload_sha256":checkpoint_sha256,"training_model":training_model,
        "training_iterations":trainer.iterations(),"training_information_sets":trainer.information_sets(),
        "evaluation_source_sha256":source_hash(),"runtime":env!("RUSTC_VERSION"),
        "first_seed":FIRST_SEED,"last_seed":FIRST_SEED+DEALS-1,"games_per_opponent":2*DEALS,
        "max_decisions":1000,"policy":"sample immutable average policy; unseen histories use uniform legal actions",
        "opponent_action_order":"RulesGame canonical legal order; same policy distribution, tie RNG mapping can differ from arena",
        "rating_method":"fixed-opponent logistic performance Elo; historical displayed Elo anchors; no persistent Elo updates"});
    emit(&mut records, &config)?;
    let started = Instant::now();
    let mut total = Counts::default();
    let mut summaries = Vec::new();
    let mut rating_opponents = Vec::new();
    for (opponent, elo) in opponents() {
        let mut counts = Counts::default();
        for seed in FIRST_SEED..FIRST_SEED + DEALS {
            for seat in 0..2 {
                let result = play(&trainer, &opponent, elo, seed, seat)?;
                emit(&mut records, &json!({"type":"game","result":result}))?;
                counts.record(&result);
                total.record(&result);
            }
        }
        let mut summary = counts.metrics();
        summary["opponent"] = json!(opponent.name());
        summary["opponent_elo"] = json!(elo);
        summary["performance_elo"] =
            json!(performance_elo(&[(elo, counts.games())], counts.points()));
        rating_opponents.push((elo, counts.games()));
        println!("{summary}");
        summaries.push(summary);
    }
    let mut aggregate = total.metrics();
    aggregate["performance_elo"] = json!(performance_elo(&rating_opponents, total.points()));
    aggregate["performance_elo_boundary"] = json!(if total.points() == 0.0 {
        Some("negative infinity")
    } else if total.points() == total.games() as f64 {
        Some("positive infinity")
    } else {
        None
    });
    let summary = json!({"config":config,"opponents":summaries,"aggregate":aggregate,"wall_seconds":started.elapsed().as_secs_f64()});
    let mut summary_file = new_file(&output.join("mccfr-summary.json"))?;
    emit(&mut summary_file, &summary)?;
    let rating = aggregate["performance_elo"]
        .as_f64()
        .map_or("unbounded".into(), |elo| format!("{elo:.2}"));
    let mut markdown = format!(
        "# Trained MCCFR policy: held-out matches\n\nCheckpoint: `{}`; {} training iterations, {} stored information sets.\n\nPlayed **{} games**, 100 against each saved top-ten baseline, using seeds {}–{} with both seats. Games stop at 1,000 decisions; cutoffs count as draws.\n\n**{} wins / {} draws / {} losses**, win rate **{:.2}%**, score **{:.2}%**, performance Elo **{}**. This Elo fits fixed historical opponent ratings and is diagnostic; persistent arena ratings were not changed.\n\nKnown-table decisions: **{} / {} ({:.4}%)**. Unseen full histories use a uniform legal-action policy. No exploration mixture or baseline tactics are added to the candidate.\n\n| Opponent | Fixed Elo | Wins | Draws | Losses | Win rate | Score | W/L |\n| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n",
        checkpoint.display(),
        trainer.iterations(),
        trainer.information_sets(),
        total.games(),
        FIRST_SEED,
        FIRST_SEED + DEALS - 1,
        total.wins,
        total.draws,
        total.losses,
        100.0 * total.wins as f64 / total.games() as f64,
        100.0 * total.points() / total.games() as f64,
        rating,
        total.known_information_set_decisions,
        total.candidate_decisions,
        100.0 * total.known_information_set_decisions as f64 / total.candidate_decisions as f64
    );
    for row in &summaries {
        let ratio = row["win_loss_ratio"]
            .as_f64()
            .map_or("∞".into(), |value| format!("{value:.4}"));
        markdown.push_str(&format!(
            "| {} | {:.2} | {} | {} | {} | {:.2}% | {:.2}% | {} |\n",
            row["opponent"].as_str().unwrap(),
            row["opponent_elo"].as_f64().unwrap(),
            row["wins"],
            row["draws"],
            row["losses"],
            100.0 * row["win_rate"].as_f64().unwrap(),
            100.0 * row["score_rate"].as_f64().unwrap(),
            ratio
        ));
    }
    let ratio = aggregate["win_loss_ratio"]
        .as_f64()
        .map_or("∞".into(), |value| format!("{value:.4}"));
    markdown.push_str(&format!("\nAggregate W/L ratio: **{ratio}**. Truncated games: **{}**. Opponent legal actions use the shared canonical ordering; this preserves their policy distributions while seeded tie choices can differ from the original arena.\n\n- [All game records](mccfr-games.jsonl)\n- [Complete configuration and metrics](mccfr-summary.json)\n",total.draws));
    let mut report = new_file(&output.join("mccfr-results.md"))?;
    report
        .write_all(markdown.as_bytes())
        .and_then(|_| report.sync_all())
        .map_err(|error| error.to_string())?;
    println!(
        "{}",
        json!({"aggregate":aggregate,"report":output.join("mccfr-results.md")})
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("evaluate_trained: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn performance_rating_and_counts_have_known_answers() {
        assert!((performance_elo(&[(1000.0, 100)], 50.0).unwrap() - 1000.0).abs() < 1e-8);
        assert!(
            (performance_elo(&[(1000.0, 100)], 75.0).unwrap() - (1000.0 + 400.0 * 3f64.log10()))
                .abs()
                < 1e-8
        );
        assert!(performance_elo(&[(1000.0, 100)], 0.0).is_none());
        assert!(performance_elo(&[(1000.0, 100)], 100.0).is_none());
        let a = performance_elo(&[(900.0, 50), (1400.0, 100)], 60.0).unwrap();
        let b = performance_elo(&[(1400.0, 100), (900.0, 50)], 60.0).unwrap();
        assert!((a - b).abs() < 1e-8);
        let mut counts = Counts::default();
        for winner in [Some(1), Some(0), None] {
            counts.record(&GameResult {
                opponent: "test".into(),
                opponent_elo: 1000.0,
                deal_seed: 0,
                candidate_seat: 1,
                candidate_rng_seed: 0,
                opponent_rng_seed: 1,
                winner,
                decisions: 10,
                candidate_decisions: 4,
                known_information_set_decisions: 1,
                truncated: winner.is_none(),
            });
        }
        assert_eq!((counts.wins, counts.draws, counts.losses), (1, 1, 1));
        assert_eq!(counts.points(), 1.5);
        assert_eq!(counts.candidate_decisions, 12);
        assert_eq!(counts.known_information_set_decisions, 3);
    }
}
