//! Literal finite Bayesian subgames of the real rules.
//!
//! Chance selects only the supplied initial scenarios. Each scenario fixes its
//! hidden deck and RNG, including future recycling shuffles. This is exact for
//! that finite model, not for every possible hidden state or the unbounded game.
//! The decision cutoff explicitly defines a draw with utility zero.
use super::{
    cfr::{Game, InformationSet, Node},
    mccfr::{SampledGame, SampledNode},
    observation::{
        Observation, PileKnowledge, advance_knowledge, card_mask, new_knowledge, observe,
    },
};
use crate::{
    game::{CARDS, Card, GameState, Move, Phase, apply_generated, deal, generated_moves, validate},
    rng::PythonRandom,
};
use std::collections::{HashMap, hash_map::Entry};

#[derive(Clone, Debug)]
pub struct Scenario {
    pub state: GameState,
    pub knowledge: PileKnowledge,
    pub probability: f64,
}

pub struct Subgame {
    pub game: Game,
    /// Canonical legal actions, indexed by information-set ID.
    pub action_lists: Vec<Vec<Move>>,
    /// Exact private-history keys shared with the procedural sampled environment.
    pub information_keys: Vec<Vec<u8>>,
}

fn public_facts(state: &GameState) -> Result<Vec<u8>, String> {
    serde_json::to_vec(&(
        state.turn,
        state.top,
        state.chosen_suit,
        state.phase,
        state.draw_penalty,
        state.skip_pending,
        state.provisional_winner,
        state.winner,
        state.deck.len(),
        state.pile.len(),
        state.opening_card,
        state.hands.each_ref().map(Vec::len),
    ))
    .map_err(|error| error.to_string())
}

fn private_view(
    state: &GameState,
    knowledge: PileKnowledge,
    player: usize,
) -> Result<Vec<u8>, String> {
    // Draw order is observable in the hand vector and can distinguish hidden
    // histories even when their current card sets are identical.
    serde_json::to_vec(&(
        public_facts(state)?,
        &state.hands[player],
        knowledge[player],
    ))
    .map_err(|error| error.to_string())
}

fn observed_event(before: &GameState, action: Move, viewer: usize) -> Result<Vec<u8>, String> {
    let (kind, actual, declared, chosen_suit, revealed) = match action {
        Move::Play {
            actual_card,
            declared_card,
            chosen_suit,
        } => (
            "play",
            (viewer == before.turn).then_some(actual_card),
            Some(declared_card),
            chosen_suit,
            None,
        ),
        Move::Challenge => ("challenge", None, None, None, before.pile.last().copied()),
        Move::Accept => ("accept", None, None, None, None),
        Move::Draw => ("draw", None, None, None, None),
        Move::Skip => ("skip", None, None, None, None),
    };
    serde_json::to_vec(&(before.turn, kind, actual, declared, chosen_suit, revealed))
        .map_err(|error| error.to_string())
}

fn action_order(action: &Move) -> (u8, u8, u8, u8) {
    match *action {
        Move::Accept => (0, 0, 0, 0),
        Move::Challenge => (1, 0, 0, 0),
        Move::Draw => (2, 0, 0, 0),
        Move::Skip => (3, 0, 0, 0),
        Move::Play {
            actual_card,
            declared_card,
            chosen_suit,
        } => (
            4,
            actual_card.0,
            declared_card.0,
            chosen_suit.map_or(4, |suit| suit as u8),
        ),
    }
}

fn history_key(history: &[Vec<u8>]) -> Vec<u8> {
    let mut key = Vec::new();
    for event in history {
        key.extend_from_slice(&(event.len() as u64).to_le_bytes());
        key.extend_from_slice(event);
    }
    key
}

fn initial_history(
    state: &GameState,
    knowledge: PileKnowledge,
) -> Result<[Vec<Vec<u8>>; 2], String> {
    Ok([
        vec![private_view(state, knowledge, 0)?],
        vec![private_view(state, knowledge, 1)?],
    ])
}

fn advance_history(
    history: &mut [Vec<Vec<u8>>; 2],
    before: &GameState,
    action: Move,
    after: &GameState,
    knowledge: PileKnowledge,
) -> Result<(), String> {
    for (viewer, events) in history.iter_mut().enumerate() {
        events.push(observed_event(before, action, viewer)?);
        events.push(private_view(after, knowledge, viewer)?);
    }
    Ok(())
}

struct Pending {
    node: usize,
    state: GameState,
    knowledge: PileKnowledge,
    history: [Vec<Vec<u8>>; 2],
    remaining: usize,
}

/// Build all legal branches, with perfect recall of each player's observations.
/// Node count includes the initial chance node; no partial tree is returned when
/// the budget is exceeded. Probabilities must be positive and sum to one (1e-12).
pub fn build_subgame(
    scenarios: &[Scenario],
    max_decisions: usize,
    max_nodes: usize,
) -> Result<Subgame, String> {
    if scenarios.is_empty() || max_decisions == 0 || max_nodes == 0 {
        return Err("Provide scenarios and positive decision/node limits".into());
    }
    validate_scenarios(scenarios)?;
    if scenarios.len() >= max_nodes {
        return Err(format!("Subgame exceeds node budget {max_nodes}"));
    }

    let mut nodes = vec![Node::Terminal(0.0)];
    let mut roots = Vec::with_capacity(scenarios.len());
    let mut pending = Vec::with_capacity(scenarios.len());
    for scenario in scenarios {
        let node = nodes.len();
        nodes.push(Node::Terminal(0.0));
        roots.push((scenario.probability, node));
        pending.push(Pending {
            node,
            state: scenario.state.clone(),
            knowledge: scenario.knowledge,
            history: initial_history(&scenario.state, scenario.knowledge)?,
            remaining: max_decisions,
        });
    }
    nodes[0] = Node::Chance(roots);
    pending.reverse();
    let mut information_sets = Vec::new();
    let mut information_keys = Vec::new();
    let mut action_lists: Vec<Vec<Move>> = Vec::new();
    let mut information_index = HashMap::new();

    // ponytail: full finite-tree expansion; the explicit node cap limits memory.
    // An iterative stack also keeps a large requested horizon off the call stack.
    while let Some(frame) = pending.pop() {
        if let Some(winner) = frame.state.winner {
            nodes[frame.node] = Node::Terminal(if winner == 0 { 1.0 } else { -1.0 });
            continue;
        }
        if frame.remaining == 0 {
            continue;
        }
        let player = frame.state.turn;
        let mut actions = generated_moves(&frame.state);
        actions.sort_unstable_by_key(action_order);
        if actions.is_empty() {
            return Err("Nonterminal game state has no legal moves".into());
        }
        if actions.len() > max_nodes - nodes.len() {
            return Err(format!("Subgame exceeds node budget {max_nodes}"));
        }
        let key = history_key(&frame.history[player]);
        let information_set = match information_index.entry((player, key.clone())) {
            Entry::Occupied(entry) => {
                let id = *entry.get();
                if action_lists[id] != actions {
                    return Err("One information set has inconsistent legal actions".into());
                }
                id
            }
            Entry::Vacant(entry) => {
                let id = information_sets.len();
                entry.insert(id);
                information_keys.push(key);
                information_sets.push(InformationSet {
                    player,
                    actions: actions.len(),
                });
                action_lists.push(actions.clone());
                id
            }
        };
        let children: Vec<usize> = (0..actions.len())
            .map(|_| {
                let id = nodes.len();
                nodes.push(Node::Terminal(0.0));
                id
            })
            .collect();
        nodes[frame.node] = Node::Decision {
            information_set,
            children: children.clone(),
        };
        for (&action, &node) in actions.iter().zip(&children).rev() {
            let mut after = frame.state.clone();
            apply_generated(&mut after, &action)?;
            let knowledge = advance_knowledge(&frame.state, &action, &after, frame.knowledge);
            let mut history = frame.history.clone();
            advance_history(&mut history, &frame.state, action, &after, knowledge)?;
            pending.push(Pending {
                node,
                state: after,
                knowledge,
                history,
                remaining: frame.remaining - 1,
            });
        }
    }
    Ok(Subgame {
        game: Game {
            nodes,
            information_sets,
            root: 0,
        },
        action_lists,
        information_keys,
    })
}

fn validate_scenarios(scenarios: &[Scenario]) -> Result<(), String> {
    if scenarios.is_empty() {
        return Err("Provide at least one scenario".into());
    }
    let mut total_probability = 0.0;
    let mut common_public = None;
    for scenario in scenarios {
        validate(&scenario.state)?;
        if !scenario.probability.is_finite() || scenario.probability <= 0.0 {
            return Err("Scenario probabilities must be positive and finite".into());
        }
        total_probability += scenario.probability;
        let pile = scenario
            .state
            .pile
            .iter()
            .fold(0, |mask, card| mask | card_mask(*card));
        if scenario.knowledge.iter().any(|mask| mask & !pile != 0) {
            return Err("Scenario knowledge must be a subset of the actual pile".into());
        }
        let public = public_facts(&scenario.state)?;
        if common_public
            .as_ref()
            .is_some_and(|common| *common != public)
        {
            return Err("Scenarios must have identical initial public facts".into());
        }
        common_public = Some(public);
    }
    if !total_probability.is_finite() || (total_probability - 1.0).abs() > 1e-12 {
        return Err("Scenario probabilities must sum to one".into());
    }
    Ok(())
}

/// On-demand play with the same keys and action ordering as `build_subgame`.
/// Fixed scenarios retain their hidden RNG; fresh deals sample new chance events
/// from the training RNG at every transition, including recycling shuffles.
pub struct RulesGame {
    scenarios: Option<Vec<Scenario>>,
    max_decisions: usize,
    cutoff_utility: f64,
}

pub struct RulesHistory {
    state: GameState,
    knowledge: PileKnowledge,
    history: [Vec<Vec<u8>>; 2],
    decisions: usize,
    actions: Vec<Move>,
}

impl RulesHistory {
    pub fn legal_actions(&self) -> &[Move] {
        &self.actions
    }

    /// Only the acting player's view crosses into an opponent policy.
    pub fn observation(&self) -> Observation {
        observe(&self.state, self.knowledge)
    }

    fn new(state: GameState, knowledge: PileKnowledge) -> Result<Self, String> {
        let history = initial_history(&state, knowledge)?;
        let mut actions = generated_moves(&state);
        actions.sort_unstable_by_key(action_order);
        Ok(Self {
            state,
            knowledge,
            history,
            decisions: 0,
            actions,
        })
    }
}

impl RulesGame {
    pub fn from_scenarios(
        scenarios: Vec<Scenario>,
        max_decisions: usize,
        cutoff_utility: f64,
    ) -> Result<Self, String> {
        validate_scenarios(&scenarios)?;
        let mut game = Self::fresh_deals(max_decisions, cutoff_utility)?;
        game.scenarios = Some(scenarios);
        Ok(game)
    }

    pub fn fresh_deals(max_decisions: usize, cutoff_utility: f64) -> Result<Self, String> {
        if max_decisions == 0
            || !cutoff_utility.is_finite()
            || !(-1.0..=1.0).contains(&cutoff_utility)
        {
            return Err(
                "Positive decision limit and finite cutoff utility in [-1,1] required".into(),
            );
        }
        Ok(Self {
            scenarios: None,
            max_decisions,
            cutoff_utility,
        })
    }
}

impl SampledGame for RulesGame {
    type State = RulesHistory;

    fn start(&self, rng: &mut PythonRandom) -> Result<RulesHistory, String> {
        if let Some(scenarios) = &self.scenarios {
            let mut draw = rng.random();
            let mut selected = scenarios.last().unwrap();
            for scenario in scenarios {
                draw -= scenario.probability;
                if draw < 0.0 {
                    selected = scenario;
                    break;
                }
            }
            RulesHistory::new(selected.state.clone(), selected.knowledge)
        } else {
            let dealer = rng.randbelow(2);
            // Reuse the core dealer directly: do not restrict the initial
            // shuffle to a small finite list of 64-bit deal seeds.
            let state = deal(rng.clone(), dealer)?;
            *rng = PythonRandom::from_state(&state.rng_state);
            let knowledge = new_knowledge(state.top);
            RulesHistory::new(state, knowledge)
        }
    }

    fn node(&self, state: &RulesHistory) -> Result<SampledNode, String> {
        if let Some(winner) = state.state.winner {
            return Ok(SampledNode::Terminal(if winner == 0 { 1.0 } else { -1.0 }));
        }
        if state.decisions >= self.max_decisions {
            return Ok(SampledNode::Terminal(self.cutoff_utility));
        }
        Ok(SampledNode::Decision {
            player: state.state.turn,
            key: history_key(&state.history[state.state.turn]),
            actions: state.actions.len(),
        })
    }

    fn advance(
        &self,
        state: &mut RulesHistory,
        action: usize,
        rng: &mut PythonRandom,
    ) -> Result<(), String> {
        if state.state.winner.is_some() || state.decisions >= self.max_decisions {
            return Err("Cannot advance a terminal training history".into());
        }
        let action = *state
            .actions
            .get(action)
            .ok_or("Action index outside legal actions")?;
        let mut after = state.state.clone();
        if self.scenarios.is_none() {
            after.rng_state = rng.state();
        }
        apply_generated(&mut after, &action)?;
        if self.scenarios.is_none() {
            *rng = PythonRandom::from_state(&after.rng_state);
        }
        let knowledge = advance_knowledge(&state.state, &action, &after, state.knowledge);
        advance_history(&mut state.history, &state.state, action, &after, knowledge)?;
        state.state = after;
        state.knowledge = knowledge;
        state.decisions += 1;
        state.actions = generated_moves(&state.state);
        state.actions.sort_unstable_by_key(action_order);
        Ok(())
    }
}

/// Common declared four-world endgame used by both exact and sampled experiments.
pub fn endgame_scenarios() -> Vec<Scenario> {
    let top = Card::parse("9C").unwrap();
    let mut result = Vec::new();
    for a in ["9H", "8D"] {
        for b in ["7H", "9S"] {
            let hands = [vec![Card::parse(a).unwrap()], vec![Card::parse(b).unwrap()]];
            let mut pile: Vec<_> = CARDS
                .into_iter()
                .filter(|card| *card != top && !hands.iter().any(|hand| hand.contains(card)))
                .collect();
            pile.push(top);
            result.push(Scenario {
                state: GameState {
                    deck: Vec::new(),
                    pile,
                    hands,
                    turn: 0,
                    top,
                    chosen_suit: None,
                    draw_penalty: 0,
                    skip_pending: false,
                    phase: Phase::Turn,
                    provisional_winner: None,
                    winner: None,
                    rng_state: PythonRandom::seed(314159).state(),
                    opening_card: false,
                },
                knowledge: new_knowledge(top),
                probability: 0.25,
            });
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engines::new_knowledge;

    #[test]
    fn hidden_initial_cards_do_not_split_an_information_set() {
        let state = crate::game::new_game(7, 0).unwrap();
        let knowledge = new_knowledge(state.top);
        let mut alternative = state.clone();
        let other = 1 - state.turn;
        std::mem::swap(&mut alternative.hands[other][0], &mut alternative.deck[0]);
        let scenarios = [
            Scenario {
                state,
                knowledge,
                probability: 0.5,
            },
            Scenario {
                state: alternative,
                knowledge,
                probability: 0.5,
            },
        ];
        let subgame = build_subgame(&scenarios, 1, 1000).unwrap();
        assert_eq!(subgame.game.information_sets.len(), 1);
        assert_eq!(subgame.action_lists.len(), 1);
        assert!(build_subgame(&scenarios, 1, 1).is_err());
        assert!(build_subgame(&scenarios, 0, 1000).is_err());
    }
}
