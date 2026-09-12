//! Only facts the acting player can know cross the policy boundary.
use crate::game::{CARDS, Card, GameState, Move, Phase, Suit};
use crate::rng::PythonRandom;

pub type PileKnowledge = [u32; 2];
pub const EMPTY_KNOWLEDGE: PileKnowledge = [0, 0];

#[derive(Clone, Debug, PartialEq)]
pub struct Observation {
    pub player: usize,
    pub hand: Vec<Card>,
    pub opponent_count: usize,
    pub top: Card,
    pub chosen_suit: Option<Suit>,
    pub phase: Phase,
    pub draw_penalty: u32,
    pub skip_pending: bool,
    pub provisional_winner: Option<usize>,
    pub deck_count: usize,
    pub pile_count: usize,
    /// A 32-bit set of known actual card identities, never their shuffled order.
    pub known_pile_cards: u32,
    pub opening_card: bool,
}

pub trait Bot: Send + Sync {
    fn name(&self) -> String;
    fn choose(
        &self,
        observation: &Observation,
        legal_moves: &[Move],
        rng: &mut PythonRandom,
    ) -> Result<Move, String>;
}

pub fn card_mask(card: Card) -> u32 {
    1u32 << card.0
}
pub fn known_cards(mask: u32) -> Vec<Card> {
    CARDS
        .into_iter()
        .filter(|card| mask & card_mask(*card) != 0)
        .collect()
}
pub fn new_knowledge(starting_card: Card) -> PileKnowledge {
    [card_mask(starting_card); 2]
}

pub fn advance_knowledge(
    before: &GameState,
    action: &Move,
    after: &GameState,
    knowledge: PileKnowledge,
) -> PileKnowledge {
    advance_known(
        before.turn,
        *before.pile.last().unwrap(),
        action,
        &after.pile,
        knowledge,
    )
}

/// The trusted match loop needs only the old actor and old top, not a full state clone.
pub(crate) fn advance_known(
    actor: usize,
    old_actual: Card,
    action: &Move,
    remaining_pile: &[Card],
    mut knowledge: PileKnowledge,
) -> PileKnowledge {
    match action {
        Move::Play { actual_card, .. } => knowledge[actor] |= card_mask(*actual_card),
        Move::Challenge => {
            knowledge[0] |= card_mask(old_actual);
            knowledge[1] |= card_mask(old_actual);
        }
        _ => {}
    }
    let remaining = remaining_pile
        .iter()
        .fold(0, |mask, card| mask | card_mask(*card));
    [knowledge[0] & remaining, knowledge[1] & remaining]
}

pub fn observe(state: &GameState, knowledge: PileKnowledge) -> Observation {
    Observation {
        player: state.turn,
        hand: state.hands[state.turn].clone(),
        opponent_count: state.hands[1 - state.turn].len(),
        top: state.top,
        chosen_suit: state.chosen_suit,
        phase: state.phase,
        draw_penalty: state.draw_penalty,
        skip_pending: state.skip_pending,
        provisional_winner: state.provisional_winner,
        deck_count: state.deck.len(),
        pile_count: state.pile.len(),
        known_pile_cards: knowledge[state.turn],
        opening_card: state.opening_card,
    }
}
