//! Resumable outcome-sampling self-play; no arena ratings or baseline opponents.
use bluff_mau_mau::engines::{
    cfr::{Node, Solver},
    mccfr::Trainer,
    solving::{RulesGame, build_subgame, endgame_scenarios},
    training_store::RunStore,
};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

#[derive(Serialize)]
struct Model {
    source_sha256: String,
    rules_sha256: String,
    runtime: String,
    model: String,
    depth: usize,
    cutoff_utility: f64,
    seed: u64,
    exploration: f64,
    max_information_sets: usize,
}

fn source_hash() -> String {
    let mut hash = Sha256::new();
    for source in [
        include_str!("../src/game.rs"),
        include_str!("../src/rng.rs"),
        include_str!("../src/engines/observation.rs"),
        include_str!("../src/engines/solving.rs"),
        include_str!("../src/engines/mccfr.rs"),
        include_str!("../src/engines/cfr.rs"),
        include_str!("../src/engines/training_store.rs"),
        include_str!("train.rs"),
        include_str!("../Cargo.lock"),
        include_str!("../Cargo.toml"),
        include_str!("../build.rs"),
    ] {
        hash.update((source.len() as u64).to_le_bytes());
        hash.update(source);
    }
    format!("{:x}", hash.finalize())
}

fn emit(log: &mut File, value: serde_json::Value) -> Result<(), String> {
    writeln!(log, "{value}")
        .and_then(|_| log.flush())
        .map_err(|e| e.to_string())?;
    let mut stdout = std::io::stdout().lock();
    writeln!(stdout, "{value}")
        .and_then(|_| stdout.flush())
        .map_err(|e| e.to_string())
}

struct Evaluation {
    solver: Solver,
    keys: Vec<Vec<u8>>,
    players: Vec<usize>,
    actions: Vec<usize>,
}

impl Evaluation {
    fn report(&self, trainer: &Trainer) -> Result<serde_json::Value, String> {
        let strategy: Result<Vec<_>, _> = self
            .keys
            .iter()
            .enumerate()
            .map(|(i, key)| trainer.average_strategy(self.players[i], key, self.actions[i]))
            .collect();
        let mut report = self.solver.evaluate_strategy(&strategy?)?;
        report.iterations = trainer.iterations();
        serde_json::to_value(report).map_err(|e| e.to_string())
    }
}

fn run() -> Result<(), String> {
    let mut model = Model {
        source_sha256: source_hash(),
        rules_sha256: format!("{:x}", Sha256::digest(include_bytes!("../src/game.rs"))),
        runtime: format!(
            "{};{};{};debug={}",
            env!("RUSTC_VERSION"),
            std::env::consts::OS,
            std::env::consts::ARCH,
            cfg!(debug_assertions)
        ),
        model: "endgame".into(),
        depth: 3,
        cutoff_utility: 0.0,
        seed: 42,
        exploration: 0.6,
        max_information_sets: 100_000,
    };
    let (mut target, mut checkpoint_every, mut exact_node_cap) = (100_000, 10_000, 250_000);
    let mut directory = None;
    let mut args = std::env::args().skip(1);
    while let Some(option) = args.next() {
        if matches!(option.as_str(), "--help" | "-h") {
            println!(
                "train [--run DIRECTORY] [--model endgame|fresh] [--depth 3] [--iterations 100000] [--seed 42] [--exploration 0.6] [--max-information-sets 100000] [--checkpoint-every 10000] [--exact-node-cap 250000] [--cutoff-utility 0]\nIterations is a TOTAL target. Repeat the same command and directory to resume; only target/checkpoint interval/exact node cap may change. Ctrl-C/SIGTERM saves the completed iteration. SIGKILL can lose at most the unsaved batch. Exact evaluation covers only the declared endgame/horizon; set node cap 0 to skip it. Fresh deals use all legal actions and complete private histories, with no full-game exploitability certificate."
            );
            return Ok(());
        }
        let value = args
            .next()
            .ok_or_else(|| format!("Missing value for {option}"))?;
        let invalid = || format!("Invalid value for {option}");
        match option.as_str() {
            "--run" => directory = Some(PathBuf::from(value)),
            "--model" => model.model = value,
            "--depth" => model.depth = value.parse().map_err(|_| invalid())?,
            "--iterations" => target = value.parse().map_err(|_| invalid())?,
            "--checkpoint-every" => checkpoint_every = value.parse().map_err(|_| invalid())?,
            "--exact-node-cap" => exact_node_cap = value.parse().map_err(|_| invalid())?,
            "--max-information-sets" => {
                model.max_information_sets = value.parse().map_err(|_| invalid())?
            }
            "--seed" => model.seed = value.parse().map_err(|_| invalid())?,
            "--exploration" => model.exploration = value.parse().map_err(|_| invalid())?,
            "--cutoff-utility" => model.cutoff_utility = value.parse().map_err(|_| invalid())?,
            _ => return Err(format!("Unknown option {option}")),
        }
    }
    if target == 0 || checkpoint_every == 0 {
        return Err("Iteration target and checkpoint interval must be positive".into());
    }
    let game = match model.model.as_str() {
        "endgame" => {
            RulesGame::from_scenarios(endgame_scenarios(), model.depth, model.cutoff_utility)?
        }
        "fresh" => RulesGame::fresh_deals(model.depth, model.cutoff_utility)?,
        _ => return Err("Model must be endgame or fresh".into()),
    };
    let mut trainer = Trainer::new(model.seed, model.exploration, model.max_information_sets)?;
    let directory = directory.unwrap_or_else(|| {
        PathBuf::from(format!(
            "runs/{}-mccfr",
            chrono::Local::now().format("%Y%m%d-%H%M%S-%f")
        ))
    });
    let identity = serde_json::to_string(&model).map_err(|e| e.to_string())?;
    let mut store = RunStore::open(&directory, &identity)?;
    if let Some(bytes) = store.load()? {
        trainer = Trainer::from_bytes(&bytes)?;
    } else {
        // Evaluation checks the rules fingerprint before opening the checkpoint;
        // the checksummed checkpoint binds this exact configuration and source.
        fs::write(directory.join("config.json"), &identity).map_err(|e| e.to_string())?;
        store.save(&trainer.to_bytes()?)?;
    }
    let mut log = OpenOptions::new()
        .create(true)
        .read(true)
        .append(true)
        .open(directory.join("progress.jsonl"))
        .map_err(|e| e.to_string())?;
    // A crash can interrupt a diagnostic line. Preserve it and separate the
    // next record; checkpoint.bin, not this log, is the resume authority.
    if log.metadata().map_err(|e| e.to_string())?.len() > 0 {
        log.seek(SeekFrom::End(-1)).map_err(|e| e.to_string())?;
        let mut byte = [0];
        log.read_exact(&mut byte).map_err(|e| e.to_string())?;
        if byte[0] != b'\n' {
            log.write_all(b"\n").map_err(|e| e.to_string())?;
        }
    }
    let stopped = Arc::new(AtomicBool::new(false));
    let signal = stopped.clone();
    ctrlc::set_handler(move || signal.store(true, Ordering::Relaxed)).map_err(|e| e.to_string())?;
    let evaluation = if model.model == "endgame" && exact_node_cap > 0 {
        let mut tree = build_subgame(&endgame_scenarios(), model.depth, exact_node_cap)?;
        for node in &mut tree.game.nodes {
            if let Node::Terminal(value) = node
                && *value == 0.0
            {
                *value = model.cutoff_utility;
            }
        }
        Some(Evaluation {
            keys: tree.information_keys,
            players: tree
                .game
                .information_sets
                .iter()
                .map(|i| i.player)
                .collect(),
            actions: tree
                .game
                .information_sets
                .iter()
                .map(|i| i.actions)
                .collect(),
            solver: Solver::new(tree.game)?,
        })
    } else {
        None
    };
    let started = Instant::now();
    let initial_iterations = trainer.iterations();
    emit(
        &mut log,
        json!({"type":"run", "directory":directory, "model":model,
        "resumed_iterations":initial_iterations, "target_iterations":target,
        "checkpoint_every":checkpoint_every, "exact_evaluation":evaluation.is_some()}),
    )?;
    let mut failure = None;
    loop {
        let report = evaluation
            .as_ref()
            .map(|e| e.report(&trainer))
            .transpose()?;
        emit(
            &mut log,
            json!({"type":"checkpoint", "iterations":trainer.iterations(),
            "information_sets":trainer.information_sets(), "decision_visits":trainer.decision_visits(),
            "wall_seconds":started.elapsed().as_secs_f64(), "exact_finite_model":report}),
        )?;
        if trainer.iterations() >= target || stopped.load(Ordering::Relaxed) || failure.is_some() {
            break;
        }
        let batch_end = target.min(trainer.iterations().saturating_add(checkpoint_every));
        while trainer.iterations() < batch_end && !stopped.load(Ordering::Relaxed) {
            if let Err(error) = trainer.train(&game, 1) {
                failure = Some(error);
                break;
            }
        }
        // Save before diagnostics: even a broken stdout pipe leaves the newest
        // complete batch recoverable. Failed episodes leave trainer state intact.
        store.save(&trainer.to_bytes()?)?;
    }
    emit(
        &mut log,
        json!({"type":"stopped", "iterations":trainer.iterations(),
        "reason":failure.as_deref().unwrap_or(if stopped.load(Ordering::Relaxed) { "signal" } else { "target" }),
        "checkpoint":directory.join("checkpoint.bin")}),
    )?;
    if let Some(error) = failure {
        return Err(error);
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("train: {error}");
        std::process::exit(1);
    }
}
