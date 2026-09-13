//! Self-play equilibrium experiment on an explicit finite hidden-information game.
use bluff_mau_mau::engines::{
    cfr::{Node, Solver},
    solving::{build_subgame, endgame_scenarios},
};
use serde_json::json;
use std::{
    collections::BTreeSet,
    io::{self, Write},
    time::Instant,
};

fn emit(value: serde_json::Value) -> Result<(), String> {
    let mut out = io::stdout().lock();
    writeln!(out, "{value}")
        .and_then(|_| out.flush())
        .map_err(|e| e.to_string())
}

fn run() -> Result<(), String> {
    let (mut depth, mut iterations, mut node_cap, mut report_every) = (3, 2000, 250_000, 100);
    let mut cutoff_utility = 0.0;
    let mut args = std::env::args().skip(1);
    while let Some(option) = args.next() {
        if option == "--help" || option == "-h" {
            println!(
                "solve [--depth 3] [--iterations 2000] [--node-cap 250000] [--report-every 100] [--cutoff-utility 0]\nSelf-play CFR on four explicitly specified hidden endgame worlds using the real rules. Nonterminal cutoffs default to draws; -1 and +1 give pessimistic/optimistic player-zero continuations. The reported best responses certify only this finite model, NOT the full game. Emits flushed JSONL progress and final root action probabilities."
            );
            return Ok(());
        }
        let raw = args
            .next()
            .ok_or_else(|| format!("Missing value for {option}"))?;
        if option == "--cutoff-utility" {
            cutoff_utility = raw.parse::<f64>().map_err(|_| "Invalid cutoff utility")?;
            if ![-1.0, 0.0, 1.0].contains(&cutoff_utility) {
                return Err("Cutoff utility must be -1, 0, or 1".into());
            }
            continue;
        }
        let value = raw
            .parse::<usize>()
            .map_err(|_| format!("Invalid value for {option}"))?;
        match option.as_str() {
            "--depth" => depth = value,
            "--iterations" => iterations = value,
            "--node-cap" => node_cap = value,
            "--report-every" => report_every = value,
            _ => return Err(format!("Unknown option {option}")),
        }
    }
    if [depth, iterations, node_cap, report_every].contains(&0) {
        return Err("All limits must be positive".into());
    }
    let started = Instant::now();
    let worlds = endgame_scenarios();
    let mut subgame = build_subgame(&worlds, depth, node_cap)?;
    for node in &mut subgame.game.nodes {
        // The adapter's only zero-valued leaves are unresolved cutoffs; actual
        // wins and losses retain their +/-1 utility in every bound experiment.
        if let Node::Terminal(value) = node
            && *value == 0.0
        {
            *value = cutoff_utility;
        }
    }
    let root_information_sets: BTreeSet<_> = match &subgame.game.nodes[subgame.game.root] {
        Node::Chance(children) => children
            .iter()
            .filter_map(|(_, node)| {
                if let Node::Decision {
                    information_set, ..
                } = &subgame.game.nodes[*node]
                {
                    Some(*information_set)
                } else {
                    None
                }
            })
            .collect(),
        _ => return Err("Expected initial hidden-world chance node".into()),
    };
    emit(json!({
        "type": "config", "algorithm": "simultaneous tabular CFR, average strategy",
        "model": "four-world declared Bayesian endgame; fixed deck/RNG in each hidden world",
        "scenario_hands": [["9H", "7H"], ["9H", "9S"], ["8D", "7H"], ["8D", "9S"]],
        "world_probabilities": [0.25,0.25,0.25,0.25], "public_top": "9C", "recycle_seed": 314159,
        "depth": depth, "cutoff_utility": cutoff_utility, "win_utility": 1, "loss_utility": -1,
        "nodes": subgame.game.nodes.len(), "information_sets": subgame.game.information_sets.len(),
        "node_cap": node_cap, "iterations": iterations, "build_seconds": started.elapsed().as_secs_f64(),
        "scope": "certificate applies only to this finite tree; original unrestricted game remains unsolved",
    }))?;
    let mut solver = Solver::new(subgame.game)?;
    let mut completed = 0;
    loop {
        let report = solver.report();
        emit(json!({"type": "convergence", "iterations": completed,
            "value_player0": report.value, "best_response_values": report.best_response_values,
            "nash_conv": report.nash_conv, "exploitability": report.exploitability,
            "wall_seconds": started.elapsed().as_secs_f64()}))?;
        if completed == iterations {
            break;
        }
        let batch = report_every.min(iterations - completed);
        solver.train(batch);
        completed += batch;
    }
    let strategy = solver.average_strategy();
    for information_set in root_information_sets {
        let probabilities: Vec<_> = subgame.action_lists[information_set]
            .iter()
            .zip(&strategy[information_set])
            .map(|(action, probability)| json!({"action": action, "probability": probability}))
            .collect();
        emit(
            json!({"type": "root_strategy", "information_set": information_set,
            "actions": probabilities}),
        )?;
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("solve: {error}");
        std::process::exit(1);
    }
}
