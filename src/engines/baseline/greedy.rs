use crate::engines::observation::Observation;
use crate::game::{Move, contribution};
use crate::rng::PythonRandom;

fn best_play(observation: &Observation, plays: &[Move], rng: &mut PythonRandom) -> Move {
    // ponytail: fixed priorities; evaluation/search can replace these after baseline comparisons.
    let score = |action: &Move| {
        let Move::Play {
            actual_card,
            declared_card,
            chosen_suit,
        } = action
        else {
            unreachable!()
        };
        let attack = contribution(*declared_card) > 0 || declared_card.rank() == 7;
        let ordinary = contribution(*actual_card) == 0 && ![7, 5].contains(&actual_card.rank());
        let suit_count = observation
            .hand
            .iter()
            .filter(|card| **card != *actual_card && Some(card.suit()) == *chosen_suit)
            .count();
        (
            *actual_card != *declared_card && observation.hand.contains(declared_card),
            if observation.opponent_count <= 2 {
                attack
            } else {
                !(attack || declared_card.rank() == 5)
            },
            ordinary,
            suit_count,
        )
    };
    let best = plays.iter().map(score).max().unwrap();
    let tied: Vec<Move> = plays
        .iter()
        .copied()
        .filter(|action| score(action) == best)
        .collect();
    tied[rng.randbelow(tied.len())]
}

pub fn greedy_turn(
    observation: &Observation,
    moves: &[Move],
    rng: &mut PythonRandom,
    bluff: u8,
    no_truth_bluff: u8,
) -> Result<Move, String> {
    let mut truthful = Vec::new();
    let mut bluffs = Vec::new();
    let mut pass = None;
    for &action in moves {
        match action {
            Move::Play {
                actual_card,
                declared_card,
                ..
            } => {
                if actual_card == declared_card {
                    truthful.push(action);
                } else {
                    bluffs.push(action);
                }
            }
            Move::Draw | Move::Skip if pass.is_none() => pass = Some(action),
            _ => {}
        }
    }
    let plays = if !truthful.is_empty() {
        if !bluffs.is_empty() && bluff != 0 && rng.random() < f64::from(bluff) / 100.0 {
            &bluffs
        } else {
            &truthful
        }
    } else if !bluffs.is_empty()
        && (pass.is_none()
            || no_truth_bluff != 0 && rng.random() < f64::from(no_truth_bluff) / 100.0)
    {
        &bluffs
    } else {
        return pass.ok_or_else(|| "No surviving play or pass".into());
    };
    Ok(best_play(observation, plays, rng))
}
