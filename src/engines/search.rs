//! Root belief sampling with observation-only rollout policies; not an equilibrium solver.
use super::{
    Bot, Observation,
    baseline::{Baseline, candidates},
    observation::{PileKnowledge, advance_known, card_mask, known_cards, observe},
    tactical::Tactical,
};
use crate::{
    game::{CARDS, Card, GameState, Move, Phase, apply_generated, contribution, generated_moves},
    rng::PythonRandom,
};

#[derive(Clone, Copy, Debug)]
pub struct BeliefSearch {
    samples: usize,
    horizon: usize,
}

impl BeliefSearch {
    /// Samples per candidate and maximum subsequent rollout decisions.
    pub fn new(samples: usize, horizon: usize) -> Result<Self, String> {
        if samples == 0 || horizon == 0 {
            return Err("Search samples and horizon must be positive".into());
        }
        Ok(Self { samples, horizon })
    }
}

fn check_observation(o: &Observation) -> Result<(), String> {
    let mut hand = 0u32;
    for &card in &o.hand {
        if card.0 >= 32 || hand & card_mask(card) != 0 {
            return Err("Invalid observed hand".into());
        }
        hand |= card_mask(card);
    }
    if o.player > 1
        || o.top.0 >= 32
        || o.pile_count == 0
        || [o.hand.len(), o.opponent_count, o.deck_count, o.pile_count]
            .into_iter()
            .try_fold(0usize, usize::checked_add)
            != Some(32)
        || o.known_pile_cards & hand != 0
        || o.known_pile_cards.count_ones() as usize > o.pile_count
        || o.phase == Phase::Response && o.known_pile_cards.count_ones() as usize == o.pile_count
    {
        return Err("Inconsistent search observation".into());
    }
    Ok(())
}

fn truth_probability(o: &Observation) -> f64 {
    if o.hand.contains(&o.top)
        || o.phase == Phase::Response && o.known_pile_cards & card_mask(o.top) != 0
    {
        return 0.0;
    }
    // ponytail: snapshot prior, not a posterior conditioned on action history.
    // Approximate a truth-first player's chance of holding one legal identity.
    // A full history/particle filter must replace this before equilibrium claims.
    let unknown = 32 - o.hand.len() - o.known_pile_cards.count_ones() as usize;
    let eligible = CARDS
        .iter()
        .filter(|card| !o.hand.contains(card) && o.known_pile_cards & card_mask(**card) == 0)
        .count()
        .min(12);
    let mut no_truth = 1.0;
    for i in 0..(o.opponent_count + 1).min(unknown) {
        no_truth *= unknown.saturating_sub(eligible + i) as f64 / (unknown - i) as f64;
    }
    (1.0 - no_truth).clamp(0.05, 0.98)
}

fn sample_world(
    o: &Observation,
    rng: &mut PythonRandom,
) -> Result<(GameState, PileKnowledge), String> {
    let mut known = known_cards(o.known_pile_cards);
    let mut unseen: Vec<Card> = CARDS
        .into_iter()
        .filter(|card| !o.hand.contains(card) && !known.contains(card))
        .collect();
    rng.shuffle(&mut unseen);
    let top = if o.opening_card || o.top.rank() == 5 && o.chosen_suit.is_none() {
        o.top
    } else if o.phase != Phase::Response && known.len() == o.pile_count {
        known[rng.randbelow(known.len())]
    } else if !o.hand.contains(&o.top)
        && !(o.phase == Phase::Response && known.contains(&o.top))
        && rng.random() < truth_probability(o)
    {
        o.top
    } else {
        // The latest opposing play is unknown; older known cards cannot be it.
        let options: Vec<Card> = unseen.iter().copied().filter(|c| *c != o.top).collect();
        if options.is_empty() {
            *unseen.first().ok_or("No possible hidden top card")?
        } else {
            options[rng.randbelow(options.len())]
        }
    };
    known.retain(|card| *card != top);
    unseen.retain(|card| *card != top);
    let missing = o
        .pile_count
        .checked_sub(known.len() + 1)
        .ok_or("Invalid pile knowledge")?;
    if missing + o.opponent_count + o.deck_count != unseen.len() {
        return Err("Cannot construct hidden world from observation".into());
    }
    let mut pile = known;
    pile.extend_from_slice(&unseen[..missing]);
    rng.shuffle(&mut pile);
    pile.push(top);
    let mut hands = [Vec::new(), Vec::new()];
    hands[o.player] = o.hand.clone();
    hands[1 - o.player] = unseen[missing..missing + o.opponent_count].to_vec();
    let mut knowledge = [0; 2];
    knowledge[o.player] = o.known_pile_cards;
    // Unknown old discards came from the opponent. Shared reveals cannot be
    // reconstructed from this API, so their remaining memory is underestimated.
    knowledge[1 - o.player] =
        pile.iter().fold(0, |mask, c| mask | card_mask(*c)) & !o.known_pile_cards;
    if o.opening_card {
        knowledge[1 - o.player] |= card_mask(top);
    }
    let state = GameState {
        deck: unseen[missing + o.opponent_count..].to_vec(),
        pile,
        hands,
        turn: o.player,
        top: o.top,
        chosen_suit: o.chosen_suit,
        draw_penalty: o.draw_penalty,
        skip_pending: o.skip_pending,
        phase: o.phase,
        provisional_winner: o.provisional_winner,
        winner: None,
        rng_state: PythonRandom::seed(rng.getrandbits(64)).state(),
        opening_card: o.opening_card,
    };
    crate::game::validate(&state)?;
    Ok((state, knowledge))
}

fn shortlist(o: &Observation, moves: &[Move]) -> Vec<Move> {
    let mut result = Vec::new();
    let mut bluffs: Vec<(f64, Move)> = Vec::new();
    for &action in moves {
        match action {
            Move::Play {
                actual_card,
                declared_card,
                chosen_suit,
            } => {
                if o.phase == Phase::Response && !o.skip_pending {
                    continue; // Accept then choose the continuation on the next decision.
                }
                if actual_card == declared_card {
                    result.push(action);
                    continue;
                }
                let continuation = o
                    .hand
                    .iter()
                    .filter(|&&c| {
                        c != actual_card
                            && (c.rank() == 5
                                || c.rank() == declared_card.rank()
                                || c.suit() == chosen_suit.unwrap_or(declared_card.suit()))
                    })
                    .count();
                let score = 0.2 * continuation as f64
                    + if declared_card.rank() == 7 { 2.0 } else { 0.0 }
                    + 0.1 * contribution(declared_card) as f64
                    + if o.hand.contains(&declared_card) {
                        1.0
                    } else {
                        0.0
                    }
                    - if o.known_pile_cards & card_mask(declared_card) != 0 {
                        5.0
                    } else {
                        0.0
                    }
                    - if matches!(actual_card.rank(), 5 | 7) || contribution(actual_card) > 0 {
                        0.8
                    } else {
                        0.0
                    };
                bluffs.push((score, action));
            }
            _ => result.push(action),
        }
    }
    bluffs.sort_by(|a, b| b.0.total_cmp(&a.0));
    // ponytail: eight bluff representatives bound rollout cost; widen if an
    // action-ablation experiment shows valuable declarations are being missed.
    let mut identities = Vec::new();
    for (_, action) in bluffs {
        let Move::Play {
            declared_card,
            chosen_suit,
            ..
        } = action
        else {
            unreachable!()
        };
        if !identities.contains(&(declared_card, chosen_suit)) {
            identities.push((declared_card, chosen_suit));
            result.push(action);
        }
        if identities.len() == 8 {
            break;
        }
    }
    result
}

fn step(state: &mut GameState, knowledge: &mut PileKnowledge, action: &Move) -> Result<(), String> {
    let actor = state.turn;
    let actual = *state.pile.last().unwrap();
    apply_generated(state, action)?;
    *knowledge = advance_known(actor, actual, action, &state.pile, *knowledge);
    Ok(())
}

fn rollout(
    mut state: GameState,
    mut knowledge: PileKnowledge,
    root: usize,
    horizon: usize,
    mut rng: PythonRandom,
) -> Result<f64, String> {
    let own = Tactical::default();
    let opponent = Baseline::mixed(0, 100, [30, 40, 50][rng.randbelow(3)])?;
    for _ in 0..horizon {
        if let Some(winner) = state.winner {
            return Ok(f64::from(winner == root));
        }
        let policy: &dyn Bot = if state.turn == root { &own } else { &opponent };
        let action = policy.choose(
            &observe(&state, knowledge),
            &generated_moves(&state),
            &mut rng,
        )?;
        step(&mut state, &mut knowledge, &action)?;
    }
    if let Some(winner) = state.winner {
        return Ok(f64::from(winner == root));
    }
    // A bounded continuation estimate, not a rules-level draw or solved value.
    let advantage = state.hands[1 - root].len() as f64 - state.hands[root].len() as f64;
    Ok(1.0 / (1.0 + (-0.35 * advantage).exp()))
}

impl Bot for BeliefSearch {
    fn name(&self) -> String {
        format!(
            "BeliefSearch[S{}-H{}-conservative]",
            self.samples, self.horizon
        )
    }

    fn choose(
        &self,
        o: &Observation,
        legal: &[Move],
        rng: &mut PythonRandom,
    ) -> Result<Move, String> {
        let moves = candidates(o, legal)?;
        if moves.len() == 1 {
            return Ok(moves[0]);
        }
        check_observation(o)?;
        let incumbent = Tactical::default().choose(o, &moves, rng)?;
        let mut actions = shortlist(o, &moves);
        if !actions.contains(&incumbent) {
            actions.push(incumbent);
        }
        if actions.is_empty() {
            return Err("Search has no surviving action".into());
        }
        let reference = actions
            .iter()
            .position(|action| *action == incumbent)
            .unwrap();
        let mut differences = vec![0.0; actions.len()];
        let mut squared_differences = vec![0.0; actions.len()];
        for _ in 0..self.samples {
            let (world, knowledge) = sample_world(o, rng)?;
            let rollout_rng = PythonRandom::seed(rng.getrandbits(64));
            // Average each action over the same hidden worlds before choosing.
            // Future policies see only their own observation, not the full world.
            let mut values = Vec::with_capacity(actions.len());
            for action in &actions {
                let mut state = world.clone();
                let mut known = knowledge;
                step(&mut state, &mut known, action)?;
                values.push(rollout(
                    state,
                    known,
                    o.player,
                    self.horizon,
                    rollout_rng.clone(),
                )?);
            }
            for i in 0..actions.len() {
                let difference = values[i] - values[reference];
                differences[i] += difference;
                squared_differences[i] += difference * difference;
            }
        }
        // Noisy maxima over many actions repeatedly displaced a strong rollout
        // policy. Keep it unless paired simulations show a clear improvement.
        // This gate is a stability heuristic, not a confidence guarantee over
        // all actions or a correction for the approximate belief model.
        let n = self.samples as f64;
        let mut selected = reference;
        let mut best_margin = 0.0;
        if self.samples > 1 {
            for i in 0..actions.len() {
                let mean = differences[i] / n;
                let variance = ((squared_differences[i] - n * mean * mean) / (n - 1.0)).max(0.0);
                let margin = mean - 2.0 * (variance / n).sqrt() - 0.03;
                if margin > best_margin {
                    best_margin = margin;
                    selected = i;
                }
            }
        }
        Ok(actions[selected])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sampled_worlds_preserve_observation_and_all_cards() {
        let mut rng = PythonRandom::seed(63);
        let mut state = crate::game::new_game(12, 0).unwrap();
        let mut knowledge = super::super::new_knowledge(state.top);
        for _ in 0..100 {
            if state.winner.is_some() {
                break;
            }
            let observation = observe(&state, knowledge);
            check_observation(&observation).unwrap();
            for _ in 0..4 {
                let (sample, known) = sample_world(&observation, &mut rng).unwrap();
                assert_eq!(observe(&sample, known), observation);
                crate::game::validate(&sample).unwrap();
                assert_eq!(generated_moves(&sample), generated_moves(&state));
            }
            let legal = generated_moves(&state);
            let action = Baseline::RandomLegal
                .choose(&observation, &legal, &mut rng)
                .unwrap();
            step(&mut state, &mut knowledge, &action).unwrap();
        }
        assert!(BeliefSearch::new(0, 1).is_err());
        assert!(BeliefSearch::new(1, 0).is_err());
    }

    #[test]
    fn an_unrestricted_queen_has_a_public_actual_identity() {
        let mut state = crate::game::new_game(0, 0).unwrap();
        let mut knowledge = super::super::new_knowledge(state.top);
        // Put a queen in the actor's hand while preserving all 32 cards.
        let queen = Card::new(5, crate::game::Suit::H);
        let owner = state.hands.iter().position(|hand| hand.contains(&queen));
        if let Some(owner) = owner {
            state.turn = owner;
        } else if let Some(index) = state.deck.iter().position(|card| *card == queen) {
            std::mem::swap(&mut state.deck[index], &mut state.hands[state.turn][0]);
        } else {
            std::mem::swap(&mut state.pile[0], &mut state.hands[state.turn][0]);
            state.top = state.pile[0];
            knowledge = super::super::new_knowledge(state.top);
        }
        step(
            &mut state,
            &mut knowledge,
            &Move::Play {
                actual_card: queen,
                declared_card: queen,
                chosen_suit: Some(crate::game::Suit::S),
            },
        )
        .unwrap();
        step(&mut state, &mut knowledge, &Move::Challenge).unwrap();
        let o = observe(&state, knowledge);
        assert_eq!(o.top, queen);
        assert_eq!(o.chosen_suit, None);
        for seed in 0..50 {
            let (sample, _) = sample_world(&o, &mut PythonRandom::seed(seed)).unwrap();
            assert_eq!(sample.pile.last(), Some(&queen));
        }
    }
}
