//! Reproducible matches: game RNG and both seat RNGs are independent.
use super::baseline::{Baseline, mixed_grid};
use super::observation::{
    Bot, EMPTY_KNOWLEDGE, PileKnowledge, advance_known, new_knowledge, observe,
};
use crate::game::{GameState, Move, Phase, apply_generated, generated_moves, new_game, validate};
use crate::rng::PythonRandom;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub struct MatchOptions {
    pub seed: i64,
    pub bot_seeds: [i64; 2],
    pub max_decisions: usize,
    pub initial_state: Option<GameState>,
    pub initial_knowledge: Option<PileKnowledge>,
}
impl Default for MatchOptions {
    fn default() -> Self {
        Self {
            seed: 0,
            bot_seeds: [0, 1],
            max_decisions: 1000,
            initial_state: None,
            initial_knowledge: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MatchResult {
    pub final_state: GameState,
    pub knowledge: PileKnowledge,
    pub decisions: usize,
    pub plays: [usize; 2],
    pub bluffs: [usize; 2],
    pub responses: [usize; 2],
    pub challenges: [usize; 2],
    pub correct_challenges: [usize; 2],
}
impl MatchResult {
    pub fn winner(&self) -> Option<usize> {
        self.final_state.winner
    }
    pub fn truncated(&self) -> bool {
        self.winner().is_none()
    }
}

pub fn run_match(bots: [&dyn Bot; 2], options: MatchOptions) -> Result<MatchResult, String> {
    if options.max_decisions == 0 {
        return Err("The decision limit must be positive".into());
    }
    let resumed = options.initial_state.is_some();
    let mut state = match options.initial_state {
        Some(state) => {
            validate(&state)?;
            state
        }
        None => new_game(options.seed, 0)?,
    };
    let mut knowledge = options.initial_knowledge.unwrap_or_else(|| {
        if resumed {
            EMPTY_KNOWLEDGE
        } else {
            new_knowledge(state.top)
        }
    });
    let mut rngs = options.bot_seeds.map(PythonRandom::seed_signed);
    let (
        mut decisions,
        mut plays,
        mut bluffs,
        mut responses,
        mut challenges,
        mut correct_challenges,
    ) = (0, [0; 2], [0; 2], [0; 2], [0; 2], [0; 2]);
    while state.winner.is_none() && decisions < options.max_decisions {
        let player = state.turn;
        let moves = generated_moves(&state);
        let action = bots[player].choose(&observe(&state, knowledge), &moves, &mut rngs[player])?;
        // Validate the policy output against the one generated list. State invariants were
        // checked at entry and the core's trusted transition preserves them thereafter.
        if !moves.contains(&action) {
            return Err(format!(
                "Bot {} returned an illegal move",
                bots[player].name()
            ));
        }
        let old_actual = *state.pile.last().unwrap();
        if state.phase == Phase::Response {
            responses[player] += 1;
        }
        match action {
            Move::Play {
                actual_card,
                declared_card,
                ..
            } => {
                plays[player] += 1;
                bluffs[player] += usize::from(actual_card != declared_card);
            }
            Move::Challenge => {
                challenges[player] += 1;
                correct_challenges[player] += usize::from(old_actual != state.top);
            }
            _ => {}
        }
        apply_generated(&mut state, &action)?;
        knowledge = advance_known(player, old_actual, &action, &state.pile, knowledge);
        decisions += 1;
    }
    Ok(MatchResult {
        final_state: state,
        knowledge,
        decisions,
        plays,
        bluffs,
        responses,
        challenges,
        correct_challenges,
    })
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Stats {
    pub games: usize,
    pub wins: usize,
    pub losses: usize,
    pub truncated: usize,
    pub total_game_decisions: usize,
    pub plays: usize,
    pub bluffs: usize,
    pub responses: usize,
    pub challenges: usize,
    pub correct_challenges: usize,
    pub mean_game_length: f64,
    pub bluff_rate: Option<f64>,
    pub challenge_rate: Option<f64>,
    pub challenge_success_rate: Option<f64>,
}
impl Stats {
    fn record(&mut self, result: &MatchResult, player: usize) {
        self.games += 1;
        self.wins += usize::from(result.winner() == Some(player));
        self.losses += usize::from(result.winner() == Some(1 - player));
        self.truncated += usize::from(result.truncated());
        self.total_game_decisions += result.decisions;
        self.plays += result.plays[player];
        self.bluffs += result.bluffs[player];
        self.responses += result.responses[player];
        self.challenges += result.challenges[player];
        self.correct_challenges += result.correct_challenges[player];
    }
    fn rates(&mut self) {
        self.mean_game_length = self.total_game_decisions as f64 / self.games as f64;
        let rate = |n: usize, d: usize| {
            if d == 0 {
                None
            } else {
                Some(n as f64 / d as f64)
            }
        };
        self.bluff_rate = rate(self.bluffs, self.plays);
        self.challenge_rate = rate(self.challenges, self.responses);
        self.challenge_success_rate = rate(self.correct_challenges, self.challenges);
    }
}

fn validate_roster(bots: &[(String, &dyn Bot)], minimum: usize) -> Result<(), String> {
    if bots.len() < minimum
        || bots.iter().any(|(name, _)| name.is_empty())
        || bots
            .iter()
            .map(|(name, _)| name)
            .collect::<BTreeSet<_>>()
            .len()
            != bots.len()
    {
        return Err(format!("Provide at least {minimum} uniquely named bots"));
    }
    Ok(())
}
fn validate_settings(seeds: &[i64], max_decisions: usize) -> Result<(), String> {
    if seeds.is_empty() || max_decisions == 0 {
        return Err("Provide game seeds and a positive decision limit".into());
    }
    Ok(())
}

pub fn round_robin(
    bots: &[(String, &dyn Bot)],
    seeds: &[i64],
    bot_seeds: [i64; 2],
    max_decisions: usize,
) -> Result<BTreeMap<String, Stats>, String> {
    validate_roster(bots, 2)?;
    validate_settings(seeds, max_decisions)?;
    let mut stats: BTreeMap<String, Stats> = bots
        .iter()
        .map(|(name, _)| (name.clone(), Stats::default()))
        .collect();
    for a in 0..bots.len() {
        for b in a + 1..bots.len() {
            for &seed in seeds {
                for seats in [[a, b], [b, a]] {
                    let result = run_match(
                        [bots[seats[0]].1, bots[seats[1]].1],
                        MatchOptions {
                            seed,
                            bot_seeds,
                            max_decisions,
                            ..Default::default()
                        },
                    )?;
                    for player in 0..2 {
                        stats
                            .get_mut(&bots[seats[player]].0)
                            .unwrap()
                            .record(&result, player);
                    }
                }
            }
        }
    }
    for row in stats.values_mut() {
        row.rates();
    }
    Ok(stats)
}

pub fn evaluate_candidates(
    candidates: &[&dyn Bot],
    opponents: &[(String, &dyn Bot)],
    seeds: &[i64],
    bot_seeds: [i64; 2],
    max_decisions: usize,
) -> Result<BTreeMap<String, Stats>, String> {
    validate_roster(opponents, 1)?;
    validate_settings(seeds, max_decisions)?;
    let named: Vec<_> = candidates.iter().map(|bot| (bot.name(), *bot)).collect();
    validate_roster(&named, 1)?;
    let mut stats = BTreeMap::new();
    for (name, candidate) in named {
        let mut row = Stats::default();
        for (_, opponent) in opponents {
            for &seed in seeds {
                for seat in 0..2 {
                    let bots = if seat == 0 {
                        [candidate, *opponent]
                    } else {
                        [*opponent, candidate]
                    };
                    let result = run_match(
                        bots,
                        MatchOptions {
                            seed,
                            bot_seeds,
                            max_decisions,
                            ..Default::default()
                        },
                    )?;
                    row.record(&result, seat);
                }
            }
        }
        row.rates();
        stats.insert(name, row);
    }
    Ok(stats)
}

pub fn evaluate_grid(
    seeds: &[i64],
    bot_seeds: [i64; 2],
    max_decisions: usize,
) -> Result<BTreeMap<String, Stats>, String> {
    let candidates = mixed_grid();
    let references: Vec<&dyn Bot> = candidates.iter().map(|bot| bot as &dyn Bot).collect();
    let opponents = [Baseline::RandomLegal, Baseline::HonestFirst];
    let named: Vec<_> = opponents
        .iter()
        .map(|bot| (bot.name(), bot as &dyn Bot))
        .collect();
    evaluate_candidates(&references, &named, seeds, bot_seeds, max_decisions)
}
