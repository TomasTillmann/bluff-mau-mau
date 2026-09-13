//! Outcome-sampling MCCFR: one fully supported trajectory, then simultaneous player updates.
//! Based on Lanctot et al. (2009), Monte Carlo Sampling for Regret Minimization in Extensive Games:
//! https://proceedings.neurips.cc/paper/2009/hash/00411460f7c92d2124a67ea0f4cb5f85-Abstract.html
//!
//! The environment must supply finite, two-player zero-sum, perfect-recall play and stable
//! action indices. Chance is sampled by the environment from its true distribution. Exact
//! information-set keys must include all information the player remembers; this module cannot
//! validate that contract across unvisited histories. No trajectory or action is pruned.
use super::cfr::{Game, Node, Report, Solver};
use crate::rng::{PythonRandom, RngState};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub trait SampledGame {
    type State;
    fn start(&self, rng: &mut PythonRandom) -> Result<Self::State, String>;
    fn node(&self, state: &Self::State) -> Result<SampledNode, String>;
    fn advance(
        &self,
        state: &mut Self::State,
        action: usize,
        rng: &mut PythonRandom,
    ) -> Result<(), String>;
}

#[derive(Clone, Debug)]
pub enum SampledNode {
    /// Utility for player zero; player one's utility is its negative.
    Terminal(f64),
    Decision {
        player: usize,
        key: Vec<u8>,
        actions: usize,
    },
}

type Key = (usize, Vec<u8>);
#[derive(Clone)]
struct Row {
    regrets: Vec<f64>,
    average: Vec<f64>,
}

pub struct Trainer {
    rng: PythonRandom,
    exploration: f64,
    max_information_sets: usize,
    iterations: usize,
    decision_visits: u64,
    rows: HashMap<Key, Row>,
}

// Floating-point bits avoid dependence on JSON decimal parsing precision during resume.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedRow {
    player: usize,
    key: Vec<u8>,
    regret_bits: Vec<u64>,
    average_bits: Vec<u64>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Checkpoint {
    algorithm: String,
    rng: RngState,
    exploration_bits: u64,
    max_information_sets: usize,
    iterations: usize,
    decision_visits: u64,
    rows: Vec<SavedRow>,
}
const ALGORITHM: &str = "outcome-sampling-mccfr-v1";

struct Visit {
    key: Key,
    strategy: Vec<f64>,
    action: usize,
    log_sample_action: f64,
    log_sample_prefix: f64,
    log_policy_prefix: [f64; 2],
}

impl Trainer {
    pub fn new(seed: u64, exploration: f64, max_information_sets: usize) -> Result<Self, String> {
        if !exploration.is_finite()
            || exploration <= 0.0
            || exploration > 1.0
            || max_information_sets == 0
        {
            return Err(
                "Exploration must be finite in (0, 1] and the information-set limit positive"
                    .into(),
            );
        }
        Ok(Self {
            rng: PythonRandom::seed(seed),
            exploration,
            max_information_sets,
            iterations: 0,
            decision_visits: 0,
            rows: HashMap::new(),
        })
    }

    pub fn iterations(&self) -> usize {
        self.iterations
    }
    pub fn decision_visits(&self) -> u64 {
        self.decision_visits
    }
    pub fn information_sets(&self) -> usize {
        self.rows.len()
    }

    pub fn has_information_set(&self, player: usize, key: &[u8]) -> bool {
        self.rows.contains_key(&(player, key.to_vec()))
    }

    pub fn average_strategy(
        &self,
        player: usize,
        key: &[u8],
        actions: usize,
    ) -> Result<Vec<f64>, String> {
        check_decision(player, actions)?;
        match self.rows.get(&(player, key.to_vec())) {
            Some(row) if row.average.len() != actions => {
                Err("Information-set action count changed".into())
            }
            Some(row) => Ok(normalized(&row.average)),
            None => Ok(vec![1.0 / actions as f64; actions]),
        }
    }

    /// Each successful episode commits atomically. On error, earlier completed episodes remain;
    /// the failing episode changes neither table, counters, nor RNG state. Environment-owned
    /// external side effects are outside this transaction and should not be used by adapters.
    pub fn train(&mut self, game: &impl SampledGame, iterations: usize) -> Result<(), String> {
        self.iterations
            .checked_add(iterations)
            .ok_or("Iteration counter overflow")?;
        for _ in 0..iterations {
            let mut rng = self.rng.clone();
            let mut state = game.start(&mut rng)?;
            let mut visits = Vec::new();
            let mut seen = HashSet::new();
            let mut new_rows = 0;
            let mut log_policy_prefix = [0.0; 2];
            let mut log_sample_prefix = 0.0;
            let utility = loop {
                match game.node(&state)? {
                    SampledNode::Terminal(value) => {
                        if !value.is_finite() {
                            return Err("Terminal utilities must be finite".into());
                        }
                        break value;
                    }
                    SampledNode::Decision {
                        player,
                        key,
                        actions,
                    } => {
                        check_decision(player, actions)?;
                        let key = (player, key);
                        if !seen.insert(key.clone()) {
                            return Err("An information set repeated in one episode; perfect recall is required".into());
                        }
                        let strategy = match self.rows.get(&key) {
                            Some(row) if row.regrets.len() != actions => {
                                return Err("Information-set action count changed".into());
                            }
                            Some(row) => normalized(&row.regrets),
                            None => {
                                new_rows += 1;
                                if new_rows > self.max_information_sets - self.rows.len() {
                                    return Err("Information-set budget reached; failing episode was not committed".into());
                                }
                                vec![1.0 / actions as f64; actions]
                            }
                        };
                        let sampling: Vec<_> = strategy
                            .iter()
                            .map(|p| {
                                (1.0 - self.exploration) * p + self.exploration / actions as f64
                            })
                            .collect();
                        if sampling.iter().any(|q| !q.is_finite() || *q <= 0.0) {
                            return Err(
                                "Exploration probabilities lost finite positive support".into()
                            );
                        }
                        let action = sample(&sampling, &mut rng);
                        let log_sample_action = sampling[action].ln();
                        visits.push(Visit {
                            key,
                            strategy: strategy.clone(),
                            action,
                            log_sample_action,
                            log_sample_prefix,
                            log_policy_prefix,
                        });
                        log_policy_prefix[player] += strategy[action].ln();
                        log_sample_prefix += log_sample_action;
                        if !log_sample_prefix.is_finite()
                            || log_policy_prefix
                                .iter()
                                .any(|x| x.is_nan() || *x == f64::INFINITY)
                        {
                            return Err(
                                "Nonfinite sampling prefix; failing episode was not committed"
                                    .into(),
                            );
                        }
                        game.advance(&mut state, action, &mut rng)?;
                    }
                }
            };
            let next_visits = self
                .decision_visits
                .checked_add(u64::try_from(visits.len()).map_err(|_| "Decision counter overflow")?)
                .ok_or("Decision counter overflow")?;
            let mut updates = Vec::with_capacity(visits.len());
            let mut log_suffix_ratio = 0.0;
            for visit in visits.into_iter().rev() {
                let player = visit.key.0;
                let chosen_probability = visit.strategy[visit.action];
                let mut row = self.rows.get(&visit.key).cloned().unwrap_or_else(|| Row {
                    regrets: vec![0.0; visit.strategy.len()],
                    average: vec![0.0; visit.strategy.len()],
                });
                // Sampling chance from the true distribution cancels it from importance
                // ratios. Its correct counterfactual mass remains in the expectation.
                let log_regret_weight = visit.log_policy_prefix[1 - player]
                    - visit.log_sample_prefix
                    - visit.log_sample_action
                    + log_suffix_ratio;
                let player_utility = if player == 0 { utility } else { -utility };
                // Expected average increments equal standard own-reach increments times the
                // fixed sum of chance reaches over this infoset; row normalization cancels it.
                let log_average_weight = visit.log_policy_prefix[player] - visit.log_sample_prefix;
                for (action, &probability) in visit.strategy.iter().enumerate() {
                    let coefficient = if action == visit.action {
                        1.0 - chosen_probability
                    } else {
                        -chosen_probability
                    };
                    let delta = weighted(player_utility, coefficient, log_regret_weight)?;
                    let average = weighted(probability, 1.0, log_average_weight)?;
                    row.regrets[action] += delta;
                    row.average[action] += average;
                    if !row.regrets[action].is_finite() || !row.average[action].is_finite() {
                        return Err(
                            "Nonfinite accumulator; failing episode was not committed".into()
                        );
                    }
                }
                updates.push((visit.key, row));
                log_suffix_ratio += chosen_probability.ln() - visit.log_sample_action;
            }
            for (key, row) in updates {
                self.rows.insert(key, row);
            }
            self.rng = rng;
            self.decision_visits = next_visits;
            self.iterations += 1;
        }
        Ok(())
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        let mut rows: Vec<_> = self
            .rows
            .iter()
            .map(|((player, key), row)| SavedRow {
                player: *player,
                key: key.clone(),
                regret_bits: row.regrets.iter().map(|v| v.to_bits()).collect(),
                average_bits: row.average.iter().map(|v| v.to_bits()).collect(),
            })
            .collect();
        rows.sort_by(|a, b| (a.player, &a.key).cmp(&(b.player, &b.key)));
        serde_json::to_vec(&Checkpoint {
            algorithm: ALGORITHM.into(),
            rng: self.rng.state(),
            exploration_bits: self.exploration.to_bits(),
            max_information_sets: self.max_information_sets,
            iterations: self.iterations,
            decision_visits: self.decision_visits,
            rows,
        })
        .map_err(|error| error.to_string())
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let checkpoint: Checkpoint =
            serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        if checkpoint.algorithm != ALGORITHM {
            return Err("Unsupported MCCFR checkpoint algorithm/version".into());
        }
        let mut trainer = Self::new(
            0,
            f64::from_bits(checkpoint.exploration_bits),
            checkpoint.max_information_sets,
        )?;
        if checkpoint.rows.len() > checkpoint.max_information_sets
            || (checkpoint.iterations == 0 && !checkpoint.rows.is_empty())
        {
            return Err("Invalid checkpoint information-set count".into());
        }
        trainer.rng = PythonRandom::from_state(&checkpoint.rng);
        if trainer.rng.state() != checkpoint.rng {
            return Err("Checkpoint RNG state must be canonical".into());
        }
        let rng_json = serde_json::to_value(&checkpoint.rng).map_err(|error| error.to_string())?;
        if rng_json[1].as_array().unwrap()[..624]
            .iter()
            .all(|word| word.as_u64() == Some(0))
        {
            return Err("Checkpoint RNG cannot contain an all-zero state".into());
        }
        if checkpoint.decision_visits < checkpoint.rows.len() as u64
            || (checkpoint.iterations == 0 && checkpoint.decision_visits != 0)
        {
            return Err("Invalid checkpoint decision count".into());
        }
        trainer.iterations = checkpoint.iterations;
        trainer.decision_visits = checkpoint.decision_visits;
        for saved in checkpoint.rows {
            check_decision(saved.player, saved.regret_bits.len())?;
            let row = Row {
                regrets: saved.regret_bits.into_iter().map(f64::from_bits).collect(),
                average: saved.average_bits.into_iter().map(f64::from_bits).collect(),
            };
            if row.average.len() != row.regrets.len()
                || row.regrets.iter().any(|v| !v.is_finite())
                || row.average.iter().any(|v| !v.is_finite() || *v < 0.0)
            {
                return Err("Invalid checkpoint regret/average row".into());
            }
            if trainer
                .rows
                .insert((saved.player, saved.key), row)
                .is_some()
            {
                return Err("Duplicate checkpoint information set".into());
            }
        }
        Ok(trainer)
    }
}

fn check_decision(player: usize, actions: usize) -> Result<(), String> {
    if player > 1 || actions == 0 {
        return Err("Decisions require player 0 or 1 and at least one action".into());
    }
    Ok(())
}

fn normalized(weights: &[f64]) -> Vec<f64> {
    let maximum = weights.iter().copied().fold(0.0, f64::max);
    if maximum <= 0.0 {
        return vec![1.0 / weights.len() as f64; weights.len()];
    }
    let sum: f64 = weights.iter().map(|v| v.max(0.0) / maximum).sum();
    weights
        .iter()
        .map(|v| (v.max(0.0) / maximum) / sum)
        .collect()
}

fn weighted(value: f64, coefficient: f64, log_weight: f64) -> Result<f64, String> {
    if log_weight.is_nan() || log_weight == f64::INFINITY {
        return Err("Nonfinite importance weight; failing episode was not committed".into());
    }
    if value == 0.0 || coefficient == 0.0 || log_weight == f64::NEG_INFINITY {
        return Ok(0.0);
    }
    let magnitude = (value.abs().ln() + coefficient.abs().ln() + log_weight).exp();
    if !magnitude.is_finite() || magnitude == 0.0 {
        return Err(
            "Importance-weight arithmetic overflow/underflow; failing episode was not committed"
                .into(),
        );
    }
    Ok(value.signum() * coefficient.signum() * magnitude)
}

fn sample(probabilities: &[f64], rng: &mut PythonRandom) -> usize {
    let needle = rng.random();
    let mut total = 0.0;
    for (action, &probability) in probabilities.iter().enumerate() {
        total += probability;
        if needle < total {
            return action;
        }
    }
    probabilities.iter().rposition(|p| *p > 0.0).unwrap()
}

/// An independently validated literal tree, useful for exact exploitability checks on small games.
pub struct TreeGame {
    game: Game,
    evaluator: Solver,
}
impl TreeGame {
    pub fn new(game: Game) -> Result<Self, String> {
        let evaluator = Solver::new(game.clone())?;
        Ok(Self { game, evaluator })
    }

    pub fn report(&self, trainer: &Trainer) -> Result<Report, String> {
        let strategy = self
            .game
            .information_sets
            .iter()
            .enumerate()
            .map(|(index, info)| {
                trainer.average_strategy(info.player, &(index as u64).to_le_bytes(), info.actions)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut report = self.evaluator.evaluate_strategy(&strategy)?;
        report.iterations = trainer.iterations();
        Ok(report)
    }

    fn through_chance(&self, mut node: usize, rng: &mut PythonRandom) -> usize {
        while let Node::Chance(children) = &self.game.nodes[node] {
            let probabilities: Vec<_> = children.iter().map(|(p, _)| *p).collect();
            node = children[sample(&probabilities, rng)].1;
        }
        node
    }
}
impl SampledGame for TreeGame {
    type State = usize;
    fn start(&self, rng: &mut PythonRandom) -> Result<usize, String> {
        Ok(self.through_chance(self.game.root, rng))
    }
    fn node(&self, state: &usize) -> Result<SampledNode, String> {
        match self.game.nodes.get(*state).ok_or("Unknown tree node")? {
            Node::Terminal(value) => Ok(SampledNode::Terminal(*value)),
            Node::Decision {
                information_set,
                children,
            } => Ok(SampledNode::Decision {
                player: self.game.information_sets[*information_set].player,
                key: (*information_set as u64).to_le_bytes().to_vec(),
                actions: children.len(),
            }),
            Node::Chance(_) => Err("Tree state must be advanced through chance".into()),
        }
    }
    fn advance(
        &self,
        state: &mut usize,
        action: usize,
        rng: &mut PythonRandom,
    ) -> Result<(), String> {
        let Some(Node::Decision { children, .. }) = self.game.nodes.get(*state) else {
            return Err("Only decision nodes can advance".into());
        };
        *state = self.through_chance(*children.get(action).ok_or("Invalid action index")?, rng);
        Ok(())
    }
}
