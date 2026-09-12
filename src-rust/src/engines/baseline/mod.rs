//! The same small baseline policies and percentage grid as the original engine.
mod greedy;
mod tactics;

use crate::engines::observation::{Bot, Observation};
use crate::game::{Move, Phase};
use crate::rng::PythonRandom;
pub use tactics::candidates;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Baseline {
    RandomLegal,
    HonestFirst,
    MixedGreedy {
        bluff: u8,
        no_truth_bluff: u8,
        challenge: u8,
    },
}

impl Default for Baseline {
    fn default() -> Self {
        Self::MixedGreedy {
            bluff: 10,
            no_truth_bluff: 10,
            challenge: 20,
        }
    }
}

impl Baseline {
    pub fn from_name(name: &str) -> Result<Self, String> {
        match name {
            "RandomLegal[uniform]" => Ok(Self::RandomLegal),
            "HonestFirst[B0-N0-C0]" => Ok(Self::HonestFirst),
            _ => {
                let values = name
                    .strip_prefix("MixedGreedy[B")
                    .and_then(|tail| tail.strip_suffix(']'))
                    .ok_or("Unknown baseline name")?;
                let (bluff, tail) = values.split_once("-N").ok_or("Invalid MixedGreedy name")?;
                let (no_truth_bluff, challenge) =
                    tail.split_once("-C").ok_or("Invalid MixedGreedy name")?;
                let percentage =
                    |value: &str| value.parse::<u8>().map_err(|_| "Invalid bot percentage");
                let bot = Self::mixed(
                    percentage(bluff)?,
                    percentage(no_truth_bluff)?,
                    percentage(challenge)?,
                )?;
                if bot.name() != name {
                    return Err("Use the canonical parameterized baseline name".into());
                }
                Ok(bot)
            }
        }
    }

    pub fn mixed(bluff: u8, no_truth_bluff: u8, challenge: u8) -> Result<Self, String> {
        if [bluff, no_truth_bluff, challenge]
            .iter()
            .any(|&value| value > 100)
        {
            return Err("Bot percentages must be integers from 0 to 100".into());
        }
        Ok(Self::MixedGreedy {
            bluff,
            no_truth_bluff,
            challenge,
        })
    }
}

impl Bot for Baseline {
    fn name(&self) -> String {
        match self {
            Self::RandomLegal => "RandomLegal[uniform]".into(),
            Self::HonestFirst => "HonestFirst[B0-N0-C0]".into(),
            Self::MixedGreedy {
                bluff,
                no_truth_bluff,
                challenge,
            } => format!("MixedGreedy[B{bluff}-N{no_truth_bluff}-C{challenge}]"),
        }
    }

    fn choose(
        &self,
        observation: &Observation,
        legal_moves: &[Move],
        rng: &mut PythonRandom,
    ) -> Result<Move, String> {
        if let Self::MixedGreedy {
            bluff,
            no_truth_bluff,
            challenge,
        } = self
        {
            Self::mixed(*bluff, *no_truth_bluff, *challenge)?;
        }
        let moves = candidates(observation, legal_moves)?;
        if moves.len() == 1 {
            return Ok(moves[0]);
        }
        match self {
            Self::RandomLegal => Ok(moves[rng.randbelow(moves.len())]),
            Self::HonestFirst => {
                if observation.phase == Phase::Response {
                    moves
                        .into_iter()
                        .find(|action| *action == Move::Accept)
                        .ok_or_else(|| "Missing legal accept".into())
                } else {
                    greedy::greedy_turn(observation, &moves, rng, 0, 0)
                }
            }
            Self::MixedGreedy {
                bluff,
                no_truth_bluff,
                challenge,
            } => {
                if observation.phase == Phase::Response {
                    // Even C=0/C=100 consumes random(), matching the policy contract.
                    let action = if rng.random() < f64::from(*challenge) / 100.0 {
                        Move::Challenge
                    } else {
                        Move::Accept
                    };
                    moves
                        .into_iter()
                        .find(|candidate| *candidate == action)
                        .ok_or_else(|| "Missing legal response".into())
                } else {
                    greedy::greedy_turn(observation, &moves, rng, *bluff, *no_truth_bluff)
                }
            }
        }
    }
}

pub fn mixed_grid() -> Vec<Baseline> {
    let mut bots = Vec::with_capacity(1331);
    for bluff in (0..=100).step_by(10) {
        for no_truth_bluff in (0..=100).step_by(10) {
            for challenge in (0..=100).step_by(10) {
                bots.push(Baseline::MixedGreedy {
                    bluff,
                    no_truth_bluff,
                    challenge,
                });
            }
        }
    }
    bots
}
