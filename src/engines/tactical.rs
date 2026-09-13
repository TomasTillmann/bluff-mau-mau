//! Fast heuristic challenger: truthful tempo, usable follow-ups, and contextual calls.
use super::{Bot, Observation, baseline::candidates, observation::card_mask};
use crate::game::{Move, Phase, contribution};
use crate::rng::PythonRandom;

#[derive(Clone, Copy, Debug, Default)]
pub struct Tactical {
    challenge: u8,
}

impl Tactical {
    /// Base uncertain-call percentage, before hand-size and available-counter adjustments.
    /// Truth is preferred whenever available; otherwise this policy attempts a bluff.
    pub fn new(challenge: u8) -> Result<Self, String> {
        if challenge > 100 {
            return Err("Challenge percentage must be from 0 to 100".into());
        }
        Ok(Self { challenge })
    }

    fn best_play(
        &self,
        observation: &Observation,
        moves: &[Move],
        rng: &mut PythonRandom,
    ) -> Option<Move> {
        let truth_available = moves.iter().any(|action| {
            matches!(action, Move::Play { actual_card, declared_card, .. } if actual_card == declared_card)
        });
        let mut best = f64::NEG_INFINITY;
        let mut tied = Vec::new();
        for &action in moves {
            let Move::Play {
                actual_card,
                declared_card,
                chosen_suit,
            } = action
            else {
                continue;
            };
            let truthful = actual_card == declared_card;
            if truth_available && !truthful {
                continue;
            }
            // ponytail: local card/tempo evaluation; belief search handles future opponents.
            let next_suit = chosen_suit.unwrap_or_else(|| declared_card.suit());
            let continuations = observation
                .hand
                .iter()
                .filter(|&&card| {
                    card != actual_card
                        && (card.rank() == 5
                            || card.rank() == declared_card.rank()
                            || card.suit() == next_suit)
                })
                .count();
            let mut score = 0.18 * continuations as f64;
            if declared_card.rank() == 7 {
                score += 2.0;
            } else {
                score += 0.65 * f64::from(contribution(declared_card));
            }
            if declared_card.rank() == 5 {
                score -= 0.65;
            }
            if !truthful {
                if observation.known_pile_cards & card_mask(declared_card) != 0 {
                    score -= 4.0;
                }
                // Penalized bluffs lose more cards when called; an ace only risks two.
                score -= 0.9 * f64::from(contribution(declared_card));
                if actual_card.rank() == 5
                    || actual_card.rank() == 7
                    || contribution(actual_card) > 0
                {
                    score -= 0.8;
                }
            }
            if score > best {
                best = score;
                tied.clear();
            }
            if score == best {
                tied.push(action);
            }
        }
        (!tied.is_empty()).then(|| tied[rng.randbelow(tied.len())])
    }
}

impl Bot for Tactical {
    fn name(&self) -> String {
        format!("Tactical[C{}]", self.challenge)
    }

    fn choose(
        &self,
        observation: &Observation,
        legal_moves: &[Move],
        rng: &mut PythonRandom,
    ) -> Result<Move, String> {
        let moves = candidates(observation, legal_moves)?;
        if moves.len() == 1 {
            return Ok(moves[0]);
        }
        if observation.phase == Phase::Response {
            let truthful_counter = moves.iter().any(|action| {
                matches!(action, Move::Play { actual_card, declared_card, .. } if actual_card == declared_card)
            });
            if observation.skip_pending && truthful_counter {
                return self
                    .best_play(observation, &moves, rng)
                    .ok_or_else(|| "Missing legal ace counter".into());
            }
            let mut chance = f64::from(self.challenge);
            if observation.opponent_count <= 1 {
                chance += 20.0;
            } else if observation.opponent_count >= 5 {
                chance -= 20.0;
            }
            if truthful_counter && !observation.hand.is_empty() {
                chance *= 0.5;
            } else if observation.draw_penalty != 0 {
                chance += 20.0;
            }
            if moves.contains(&Move::Challenge) && rng.random() < chance.clamp(0.0, 100.0) / 100.0 {
                return Ok(Move::Challenge);
            }
            if observation.skip_pending
                && let Some(action) = self.best_play(observation, &moves, rng)
            {
                return Ok(action);
            }
            return moves
                .into_iter()
                .find(|action| *action == Move::Accept)
                .ok_or_else(|| "Missing legal accept".into());
        }
        self.best_play(observation, &moves, rng)
            .or_else(|| {
                moves
                    .into_iter()
                    .find(|action| matches!(action, Move::Draw | Move::Skip))
            })
            .ok_or_else(|| "No surviving play or pass".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{Card, Suit};

    #[test]
    fn counters_an_ace_before_accepting_its_skip() {
        let ace = Card::new(7, Suit::D);
        let counter = Move::Play {
            actual_card: ace,
            declared_card: ace,
            chosen_suit: None,
        };
        let observation = Observation {
            player: 0,
            hand: vec![ace, Card::new(1, Suit::H)],
            opponent_count: 3,
            top: Card::new(7, Suit::H),
            chosen_suit: None,
            phase: Phase::Response,
            draw_penalty: 0,
            skip_pending: true,
            provisional_winner: None,
            deck_count: 20,
            pile_count: 7,
            known_pile_cards: 0,
            opening_card: false,
        };
        for seed in 0..20 {
            assert_eq!(
                Tactical::default()
                    .choose(
                        &observation,
                        &[Move::Accept, Move::Challenge, counter],
                        &mut PythonRandom::seed(seed)
                    )
                    .unwrap(),
                counter
            );
        }
        assert!(Tactical::new(101).is_err());
        assert!(
            Tactical::default()
                .choose(&observation, &[], &mut PythonRandom::seed(0))
                .is_err()
        );
    }
}
