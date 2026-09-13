//! Playable snapshot policies with the frozen ratings published in the repository.
use crate::engines::Bot;
use crate::engines::baseline::{Baseline, mixed_grid};
use crate::engines::search::BeliefSearch;
use crate::engines::tactical::Tactical;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;

pub const RATING_DATE: &str = "2026-09-13";

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BotInfo {
    pub id: String,
    pub name: String,
    pub family: String,
    pub elo: f64,
    pub elo_kind: String,
    pub games: u64,
    pub wins: u64,
    pub draws: u64,
    pub losses: u64,
    /// (Wins + half of draws) / games, in the range 0..=1.
    pub score_rate: f64,
    pub description: String,
}

pub struct BotCatalog {
    pub bots: Vec<BotInfo>,
    engines: Vec<Arc<dyn Bot>>,
}
impl BotCatalog {
    pub fn new() -> Result<Self, String> {
        let mut baselines = mixed_grid();
        baselines.extend([Baseline::HonestFirst, Baseline::RandomLegal]);
        let mut by_name: HashMap<_, _> =
            baselines.into_iter().map(|bot| (bot.name(), bot)).collect();
        let mut bots = Vec::with_capacity(by_name.len() + 2);
        let mut engines: Vec<Arc<dyn Bot>> = Vec::with_capacity(by_name.len() + 2);
        // The Markdown is the saved snapshot, not the live arena database.
        for line in include_str!("../docs/arena-ranking-2026-09-13.md").lines() {
            let cells: Vec<_> = line.split('|').map(str::trim).collect();
            if cells.len() != 11 || cells[1].parse::<usize>().is_err() {
                continue;
            }
            let name = cells[2];
            let bot = by_name
                .remove(name)
                .ok_or_else(|| format!("Unknown or duplicate rated bot: {name}"))?;
            let number = |cell: &str| -> Result<u64, String> {
                cell.replace(',', "")
                    .parse()
                    .map_err(|_| format!("Invalid rating count for {name}"))
            };
            let games = number(cells[5])?;
            let wins = number(cells[6])?;
            let draws = number(cells[7])?;
            let losses = number(cells[8])?;
            let elo: f64 = cells[9]
                .parse()
                .map_err(|_| format!("Invalid Elo for {name}"))?;
            if games == 0
                || wins.checked_add(draws).and_then(|n| n.checked_add(losses)) != Some(games)
                || !elo.is_finite()
            {
                return Err(format!("Inconsistent ratings for {name}"));
            }
            let (family, description) = match bot {
                Baseline::MixedGreedy { bluff, no_truth_bluff, challenge } => (
                    "MixedGreedy",
                    format!("After obvious moves: bluff {bluff}% with a truthful play available, bluff {no_truth_bluff}% without one, and challenge uncertain claims {challenge}% of the time."),
                ),
                Baseline::HonestFirst => ("HonestFirst", "Plays truthful cards when possible, otherwise draws; accepts uncertain claims. Shared obvious moves take priority.".into()),
                Baseline::RandomLegal => ("RandomLegal", "Chooses uniformly among legal moves after the shared obvious-move rules.".into()),
            };
            bots.push(BotInfo {
                id: name.into(),
                name: name.into(),
                family: family.into(),
                elo,
                elo_kind: "arena".into(),
                games,
                wins,
                draws,
                losses,
                score_rate: (wins as f64 + 0.5 * draws as f64) / games as f64,
                description,
            });
            engines.push(Arc::new(bot));
        }
        if !by_name.is_empty() {
            return Err("Frozen ranking is missing baseline bots".into());
        }
        // Held-out 1,000-game top-ten benchmark in docs/solving.md. These are
        // fixed-opponent performance estimates, not round-robin arena ratings.
        for (engine, family, elo, wins, losses, description) in [
            (
                Arc::new(Tactical::default()) as Arc<dyn Bot>,
                "Tactical",
                1678.89,
                858,
                142,
                "Uses direct ace counters and tactical card priorities. Performance Elo and results are from the saved benchmark against the arena top ten.",
            ),
            (
                Arc::new(BeliefSearch::new(8, 40)?) as Arc<dyn Bot>,
                "BeliefSearch",
                1670.43,
                852,
                148,
                "Tests tactical choices with eight sampled hidden worlds and 40-decision rollouts. Performance Elo and results are from the saved benchmark against the arena top ten.",
            ),
        ] {
            let name = engine.name();
            bots.push(BotInfo {
                id: name.clone(),
                name,
                family: family.into(),
                elo,
                elo_kind: "performance".into(),
                games: 1000,
                wins,
                draws: 0,
                losses,
                score_rate: wins as f64 / 1000.0,
                description: description.into(),
            });
            engines.push(engine);
        }
        Ok(Self { bots, engines })
    }

    pub fn get(&self, id: &str) -> Option<(BotInfo, Arc<dyn Bot>)> {
        self.bots
            .iter()
            .position(|bot| bot.id == id)
            .map(|index| (self.bots[index].clone(), Arc::clone(&self.engines[index])))
    }
}
