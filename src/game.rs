//! Pure two-player Bluff Mau-Mau rules. Ordering matches the original rule engine.
use crate::rng::{PythonRandom, RngState};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{fmt, str::FromStr};

pub const RANKS: [&str; 8] = ["7", "8", "9", "10", "J", "Q", "K", "A"];
pub const SUITS: [Suit; 4] = [Suit::H, Suit::D, Suit::C, Suit::S];
pub const CARDS: [Card; 32] = {
    let mut cards = [Card(0); 32];
    let mut i = 0;
    while i < 32 {
        cards[i] = Card(i as u8);
        i += 1;
    }
    cards
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Suit {
    H,
    D,
    C,
    S,
}

impl fmt::Display for Suit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::H => "H",
            Self::D => "D",
            Self::C => "C",
            Self::S => "S",
        })
    }
}

impl FromStr for Suit {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "H" => Ok(Self::H),
            "D" => Ok(Self::D),
            "C" => Ok(Self::C),
            "S" => Ok(Self::S),
            _ => Err("Unknown suit".into()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Card(pub u8);

impl Card {
    pub const fn new(rank: u8, suit: Suit) -> Self {
        assert!(rank < 8, "Card rank must be 0..7");
        Self((suit as u8) * 8 + rank)
    }
    pub const fn rank(self) -> u8 {
        self.0 % 8
    }
    pub fn suit(self) -> Suit {
        SUITS[(self.0 / 8) as usize]
    }
    pub fn parse(value: &str) -> Result<Self, String> {
        value.parse()
    }
}

impl FromStr for Card {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if !value.is_ascii() || value.len() < 2 {
            return Err("Card needs rank 7..A and suit H, D, C, or S".into());
        }
        let (rank, suit) = value.split_at(value.len() - 1);
        let rank = RANKS
            .iter()
            .position(|&r| r == rank)
            .ok_or("Unknown rank")? as u8;
        Ok(Self::new(rank, suit.parse()?))
    }
}

impl fmt::Display for Card {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 >= 32 {
            return write!(f, "InvalidCard({})", self.0);
        }
        write!(f, "{}{}", RANKS[self.rank() as usize], self.suit())
    }
}

impl Serialize for Card {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.0 >= 32 {
            return Err(serde::ser::Error::custom("Invalid Card"));
        }
        serializer.collect_str(self)
    }
}
impl<'de> Deserialize<'de> for Card {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    #[default]
    Turn,
    Response,
    Finished,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum Move {
    Play {
        actual_card: Card,
        declared_card: Card,
        #[serde(default)]
        chosen_suit: Option<Suit>,
    },
    Draw,
    Challenge,
    Accept,
    Skip,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameState {
    pub deck: Vec<Card>,
    pub pile: Vec<Card>,
    pub hands: [Vec<Card>; 2],
    pub turn: usize,
    pub top: Card,
    #[serde(default)]
    pub chosen_suit: Option<Suit>,
    #[serde(default)]
    pub draw_penalty: u32,
    #[serde(default)]
    pub skip_pending: bool,
    #[serde(default)]
    pub phase: Phase,
    #[serde(default)]
    pub provisional_winner: Option<usize>,
    #[serde(default)]
    pub winner: Option<usize>,
    #[serde(default)]
    pub rng_state: RngState,
    #[serde(default)]
    pub opening_card: bool,
}

pub fn contribution(card: Card) -> u32 {
    if card.rank() == 0 {
        2
    } else if card == Card::new(6, Suit::S) {
        4
    } else {
        0
    }
}

pub fn validate(state: &GameState) -> Result<(), String> {
    let mut seen = 0u32;
    let mut count = 0;
    for card in state
        .deck
        .iter()
        .chain(&state.pile)
        .chain(&state.hands[0])
        .chain(&state.hands[1])
    {
        if card.0 >= 32 || seen & (1u32 << card.0) != 0 {
            return Err("State must contain each of the 32 cards exactly once".into());
        }
        seen |= 1u32 << card.0;
        count += 1;
    }
    if state.pile.is_empty() || count != 32 {
        return Err("State must contain each of the 32 cards exactly once".into());
    }
    if state.turn > 1 {
        return Err("Turn must be player 0 or 1".into());
    }
    if state.top.0 >= 32 {
        return Err("Top must be an effective card identity".into());
    }
    if state.chosen_suit.is_some() && state.top.rank() != 5 {
        return Err("Only a queen can have a continuing suit".into());
    }
    if !state.draw_penalty.is_multiple_of(2)
        || (state.draw_penalty != 0
            && (contribution(state.top) == 0 || state.draw_penalty < contribution(state.top)))
        || (state.skip_pending && state.top.rank() != 7)
        || (state.draw_penalty != 0 && state.skip_pending)
    {
        return Err("Invalid pending effect".into());
    }
    if state.opening_card
        && (state.phase != Phase::Turn
            || state.pile.len() != 1
            || state.top != state.pile[0]
            || state.chosen_suit.is_some()
            || state.draw_penalty != 0
            || state.skip_pending
            || state.provisional_winner.is_some()
            || state.winner.is_some())
    {
        return Err(
            "An opening card must be the untouched starting discard with no pending effect".into(),
        );
    }
    for player in [state.provisional_winner, state.winner]
        .into_iter()
        .flatten()
    {
        if player > 1 || !state.hands[player].is_empty() {
            return Err("A victory claim must identify an empty-handed player".into());
        }
    }
    if state.phase == Phase::Response
        && state.hands.iter().all(Vec::is_empty)
        && state.provisional_winner != Some(state.turn)
    {
        return Err("The responder must hold the earlier empty-hand victory claim".into());
    }
    if (state.phase == Phase::Finished) != state.winner.is_some() {
        return Err("Finished phase must have a winner".into());
    }
    if let Some(winner) = state.winner {
        if state.turn != winner {
            return Err("Finished turn must identify the winner".into());
        }
        if state.provisional_winner.is_some() || state.draw_penalty != 0 || state.skip_pending {
            return Err("Finished games have no pending effects or victory claim".into());
        }
    } else {
        if state.hands.iter().any(Vec::is_empty) != state.provisional_winner.is_some() {
            return Err("An empty hand needs a provisional victory claim".into());
        }
        if state.phase == Phase::Turn && state.hands[state.turn].is_empty() {
            return Err("Forced empty-hand actions must already be resolved".into());
        }
    }
    if state.phase == Phase::Response
        && (state.pile.len() < 2
            || (state.top.rank() == 5 && state.chosen_suit.is_none())
            || state.draw_penalty < contribution(state.top)
            || state.skip_pending != (state.top.rank() == 7))
    {
        return Err("A response needs a complete declaration and its pending effect".into());
    }
    Ok(())
}

pub fn new_game(seed: i64, dealer: usize) -> Result<GameState, String> {
    deal(PythonRandom::seed_signed(seed), dealer)
}
pub fn new_game_u64(seed: u64, dealer: usize) -> Result<GameState, String> {
    deal(PythonRandom::seed(seed), dealer)
}
pub fn new_game_decimal(seed: &str, dealer: usize) -> Result<GameState, String> {
    deal(PythonRandom::seed_decimal(seed)?, dealer)
}

pub(crate) fn deal(mut rng: PythonRandom, dealer: usize) -> Result<GameState, String> {
    if dealer > 1 {
        return Err("Dealer must be 0 or 1".into());
    }
    let mut cards = CARDS;
    rng.shuffle(&mut cards);
    let mut hands = [Vec::with_capacity(5), Vec::with_capacity(5)];
    for i in 0..10 {
        hands[if i % 2 == 0 { 1 - dealer } else { dealer }].push(cards[i]);
    }
    let top = cards[10];
    Ok(GameState {
        deck: cards[11..].to_vec(),
        pile: vec![top],
        hands,
        turn: 1 - dealer,
        top,
        chosen_suit: None,
        draw_penalty: 0,
        skip_pending: false,
        phase: Phase::Turn,
        provisional_winner: None,
        winner: None,
        rng_state: rng.state(),
        opening_card: true,
    })
}

pub fn declarations(
    top: Card,
    chosen_suit: Option<Suit>,
    draw_penalty: u32,
    skip_pending: bool,
) -> Vec<Card> {
    let king_spades = Card::new(6, Suit::S);
    let seven_spades = Card::new(0, Suit::S);
    CARDS
        .into_iter()
        .filter(|&card| {
            if skip_pending {
                return card.rank() == 7;
            }
            if draw_penalty != 0 {
                return if top == king_spades {
                    card.rank() == 5 || card == seven_spades
                } else {
                    matches!(card.rank(), 0 | 5) || (top == seven_spades && card == king_spades)
                };
            }
            if top.rank() == 5 && chosen_suit.is_none() {
                return true;
            }
            card.rank() == 5
                || card.rank() == top.rank()
                || card.suit() == chosen_suit.unwrap_or_else(|| top.suit())
        })
        .collect()
}

pub fn move_generator(state: &GameState) -> Result<Vec<Move>, String> {
    validate(state)?;
    Ok(generated_moves(state))
}

pub(crate) fn generated_moves(state: &GameState) -> Vec<Move> {
    if state.winner.is_some() {
        return Vec::new();
    }
    let declarations = declarations(
        state.top,
        state.chosen_suit,
        state.draw_penalty,
        state.skip_pending,
    );
    let mut moves =
        Vec::with_capacity(3 + state.hands[state.turn].len() * (declarations.len() + 12));
    if state.phase == Phase::Response {
        moves.extend([Move::Accept, Move::Challenge]);
    }
    if state.hands[state.turn].is_empty() {
        return moves;
    }
    if state.skip_pending {
        if state.phase == Phase::Turn {
            moves.push(Move::Skip);
        }
    } else if !state.deck.is_empty() || state.pile.len() > 1 {
        moves.push(Move::Draw);
    }
    for &actual_card in &state.hands[state.turn] {
        for &declared_card in &declarations {
            if declared_card.rank() == 5 {
                for chosen_suit in SUITS {
                    moves.push(Move::Play {
                        actual_card,
                        declared_card,
                        chosen_suit: Some(chosen_suit),
                    });
                }
            } else {
                moves.push(Move::Play {
                    actual_card,
                    declared_card,
                    chosen_suit: None,
                });
            }
        }
    }
    moves
}

fn is_legal(state: &GameState, action: &Move) -> bool {
    if state.winner.is_some() {
        return false;
    }
    match action {
        Move::Accept | Move::Challenge => state.phase == Phase::Response,
        Move::Draw => {
            !state.hands[state.turn].is_empty()
                && !state.skip_pending
                && (!state.deck.is_empty() || state.pile.len() > 1)
        }
        Move::Skip => {
            !state.hands[state.turn].is_empty() && state.skip_pending && state.phase == Phase::Turn
        }
        Move::Play {
            actual_card,
            declared_card,
            chosen_suit,
        } => {
            actual_card.0 < 32
                && declared_card.0 < 32
                && state.hands[state.turn].contains(actual_card)
                && (chosen_suit.is_some() == (declared_card.rank() == 5))
                && declarations(
                    state.top,
                    state.chosen_suit,
                    state.draw_penalty,
                    state.skip_pending,
                )
                .contains(declared_card)
        }
    }
}

pub fn play(state: &GameState, action: &Move) -> Result<GameState, String> {
    validate(state)?;
    if !is_legal(state, action) {
        return Err("Move is not legal in this state".into());
    }
    let mut result = state.clone();
    apply_generated(&mut result, action)?;
    Ok(result)
}

fn draw(state: &mut GameState, player: usize, mut count: u32) {
    let mut rng = PythonRandom::from_state(&state.rng_state);
    let mut drawn = false;
    while count != 0 {
        if state.deck.is_empty() {
            if state.pile.len() == 1 {
                break;
            }
            let top = state.pile.pop().expect("a validated state has a pile");
            std::mem::swap(&mut state.deck, &mut state.pile);
            state.pile.push(top);
            rng.shuffle(&mut state.deck);
        }
        let take = (count as usize).min(state.deck.len());
        state.hands[player].extend(state.deck.drain(..take));
        count -= take as u32;
        drawn = true;
    }
    if drawn && state.provisional_winner == Some(player) {
        state.provisional_winner = state.hands[1 - player].is_empty().then_some(1 - player);
    }
    state.rng_state = rng.state();
}

fn finish(state: &mut GameState, player: usize) {
    state.turn = player;
    state.phase = Phase::Finished;
    state.winner = Some(player);
    state.provisional_winner = None;
    state.draw_penalty = 0;
    state.skip_pending = false;
    state.opening_card = false;
}

/// Trusted match loop only: the move must come from this state's generated move list.
pub(crate) fn apply_generated(state: &mut GameState, action: &Move) -> Result<(), String> {
    let player = state.turn;
    let other = 1 - player;
    match *action {
        Move::Play {
            actual_card,
            declared_card,
            chosen_suit,
        } => {
            let amount = contribution(declared_card);
            let carried = if state.opening_card {
                contribution(state.top)
            } else {
                state.draw_penalty
            };
            // Check before mutation, including callers of the trusted match path.
            let penalty = if amount == 0 {
                0
            } else {
                carried
                    .checked_add(amount)
                    .ok_or("Draw penalty exceeds the supported integer range")?
            };
            state.hands[player].retain(|&card| card != actual_card);
            if state.hands[player].is_empty() && state.provisional_winner.is_none() {
                state.provisional_winner = Some(player);
            }
            state.pile.push(actual_card);
            state.turn = other;
            state.top = declared_card;
            state.chosen_suit = chosen_suit;
            state.phase = Phase::Response;
            state.opening_card = false;
            state.draw_penalty = penalty;
            state.skip_pending = declared_card.rank() == 7;
        }
        Move::Challenge => {
            let winner = if state.pile.last() == Some(&state.top) {
                other
            } else {
                player
            };
            // Any count above 31 has the same result: all drawable cards are taken.
            draw(state, 1 - winner, state.draw_penalty.saturating_add(2));
            state.turn = winner;
            state.phase = Phase::Turn;
            state.top = *state.pile.last().expect("a validated state has a pile");
            state.chosen_suit = None;
            state.draw_penalty = 0;
            state.skip_pending = false;
            if state.hands[winner].is_empty() {
                finish(state, winner);
            }
        }
        Move::Accept => {
            state.phase = Phase::Turn;
            if state.skip_pending && state.hands.iter().any(|hand| !hand.is_empty()) {
                state.skip_pending = false;
                state.turn = other;
                if state.hands[other].is_empty() {
                    finish(state, other);
                }
                return Ok(());
            }
            if !state.hands[player].is_empty() {
                return Ok(());
            }
            if state.draw_penalty != 0 {
                draw(state, player, state.draw_penalty);
                state.draw_penalty = 0;
                state.turn = other;
                if state.hands[other].is_empty() {
                    finish(state, other);
                }
            } else {
                finish(state, player);
            }
        }
        Move::Draw => {
            draw(state, player, state.draw_penalty.max(1));
            state.turn = other;
            state.phase = Phase::Turn;
            state.draw_penalty = 0;
            if state.hands[other].is_empty() {
                finish(state, other);
            }
        }
        Move::Skip => {
            state.turn = other;
            state.skip_pending = false;
            if state.hands[other].is_empty() {
                finish(state, other);
            }
        }
    }
    Ok(())
}
