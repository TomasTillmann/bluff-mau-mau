use crate::engines::observation::{Observation, card_mask};
use crate::game::{Move, Phase, contribution, declarations};

pub fn candidates(observation: &Observation, legal_moves: &[Move]) -> Result<Vec<Move>, String> {
    if legal_moves.is_empty() {
        return Err("A bot needs at least one legal move".into());
    }
    let hand = &observation.hand;
    if observation.phase == Phase::Response {
        let mut response = None;
        if hand.is_empty()
            && observation.draw_penalty == 0
            && (!observation.skip_pending || observation.opponent_count == 0)
        {
            response = Some(Move::Accept);
        } else if hand.len() == 1 {
            let last = hand[0];
            let winning_final = observation.opponent_count == 0 && contribution(last) > 0
                || observation.opponent_count == 1 && last.rank() == 7;
            if winning_final
                && declarations(
                    observation.top,
                    observation.chosen_suit,
                    observation.draw_penalty,
                    observation.skip_pending,
                )
                .contains(&last)
            {
                if observation.skip_pending {
                    return Ok(legal_moves.iter().copied().filter(|action| matches!(action, Move::Play { actual_card, declared_card, .. } if *actual_card == last && *declared_card == last)).collect());
                }
                response = Some(Move::Accept);
            }
        }
        if response.is_none() {
            let last_chance = observation.opponent_count == 0
                && (hand.is_empty() && observation.draw_penalty > 0
                    || hand.len() == 1 && observation.skip_pending);
            let known_bluff = hand.contains(&observation.top)
                || observation.known_pile_cards & card_mask(observation.top) != 0;
            if last_chance || known_bluff {
                response = Some(Move::Challenge);
            }
        }
        return Ok(legal_moves
            .iter()
            .copied()
            .filter(|action| response.is_none_or(|wanted| *action == wanted))
            .collect());
    }
    if hand.len() == 1 {
        for &action in legal_moves {
            if let Move::Play {
                actual_card,
                declared_card,
                ..
            } = action
                && actual_card == declared_card
                && (observation.opponent_count == 0 && contribution(declared_card) > 0
                    || observation.opponent_count == 1 && declared_card.rank() == 7)
            {
                return Ok(vec![action]);
            }
        }
    }
    if observation.opponent_count == 0 {
        let returns: Vec<Move> = legal_moves.iter().copied().filter(|action| matches!(action, Move::Play { declared_card, .. } if contribution(*declared_card) > 0 || declared_card.rank() == 7 && hand.len() > 1)).collect();
        if !returns.is_empty() {
            return Ok(returns);
        }
    }
    Ok(legal_moves.to_vec())
}
