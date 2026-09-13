//! Independent regressions for the rule that queens cannot cover effect cards.
//! Expectations come from the rules; only public game APIs are exercised.
use bluff_mau_mau::{
    game::{self, CARDS, Card, GameState, Move, Phase, SUITS, Suit},
    rng::PythonRandom,
};

fn card(text: &str) -> Card {
    Card::parse(text).unwrap()
}

fn effect_tops() -> Vec<Card> {
    ["7H", "7D", "7C", "7S", "AH", "AD", "AC", "AS", "KS"]
        .map(card)
        .to_vec()
}

fn position(top: Card, phase: Phase, active: bool) -> GameState {
    let mut first = ["QH", "QD", "QC", "QS", "8H", "8D"]
        .map(card)
        .into_iter()
        .filter(|&c| c != top)
        .collect::<Vec<_>>();
    first.sort();
    let second = ["9C", "9S"]
        .map(card)
        .into_iter()
        .filter(|&c| c != top)
        .collect::<Vec<_>>();
    let mut pile = vec![];
    if phase == Phase::Response {
        pile.push(if top == card("10D") {
            card("JD")
        } else {
            card("10D")
        });
    }
    pile.push(top);
    let state = GameState {
        deck: CARDS
            .into_iter()
            .filter(|c| !first.contains(c) && !second.contains(c) && !pile.contains(c))
            .collect(),
        pile,
        hands: [first, second],
        turn: 0,
        top,
        chosen_suit: None,
        draw_penalty: if active {
            if top == card("KS") {
                4
            } else if top.rank() == 0 {
                2
            } else {
                0
            }
        } else {
            0
        },
        skip_pending: active && top.rank() == 7,
        phase,
        provisional_winner: None,
        winner: None,
        opening_card: phase == Phase::Turn && !active,
        rng_state: PythonRandom::seed(913).state(),
    };
    game::validate(&state).unwrap();
    state
}

fn queen_play(actual: Card, queen: Card, suit: Suit) -> Move {
    Move::Play {
        actual_card: actual,
        declared_card: queen,
        chosen_suit: Some(suit),
    }
}

fn assert_no_queen(state: &GameState) {
    game::validate(state).unwrap();
    let moves = game::move_generator(state).unwrap();
    assert!(
        !moves
            .iter()
            .any(|m| matches!(m, Move::Play { declared_card, .. } if declared_card.rank() == 5)),
        "queen offered on {}: phase={:?}, penalty={}, skip={}, opening={}",
        state.top,
        state.phase,
        state.draw_penalty,
        state.skip_pending,
        state.opening_card,
    );
}

#[test]
fn inactive_top_declarations_follow_the_corrected_rule_in_card_order() {
    for top in CARDS {
        for chosen in [
            None,
            Some(Suit::H),
            Some(Suit::D),
            Some(Suit::C),
            Some(Suit::S),
        ] {
            if top.rank() != 5 && chosen.is_some() {
                continue;
            }
            let expected: Vec<_> = CARDS
                .into_iter()
                .filter(|candidate| {
                    if candidate.rank() == 5 {
                        !effect_tops().contains(&top)
                    } else if top.rank() == 5 {
                        chosen.is_none_or(|suit| candidate.suit() == suit)
                    } else {
                        candidate.rank() == top.rank() || candidate.suit() == top.suit()
                    }
                })
                .collect();
            assert_eq!(
                game::declarations(top, chosen, 0, false),
                expected,
                "top {top}, chosen {chosen:?}"
            );
        }
    }
}

#[test]
fn every_top_allows_all_queen_suits_exactly_when_it_is_not_an_effect_card() {
    for top in CARDS {
        let state = position(top, Phase::Turn, false);
        let moves = game::move_generator(&state).unwrap();
        assert_eq!(moves.first(), Some(&Move::Draw));
        for &actual in &state.hands[0] {
            for queen in ["QH", "QD", "QC", "QS"].map(card) {
                for suit in SUITS {
                    let action = queen_play(actual, queen, suit);
                    assert_eq!(
                        moves.contains(&action),
                        !effect_tops().contains(&top),
                        "top {top}, action {action:?}"
                    );
                    if !effect_tops().contains(&top) {
                        let after = game::play(&state, &action).unwrap();
                        assert_eq!(after.top, queen);
                        assert_eq!(after.chosen_suit, Some(suit));
                        game::validate(&after).unwrap();
                    }
                }
            }
        }
    }
}

#[test]
fn queen_declarations_are_rejected_atomically_in_turn_and_response_states() {
    for top in effect_tops() {
        for phase in [Phase::Turn, Phase::Response] {
            for active in [false, true] {
                if phase == Phase::Response && !active {
                    continue;
                }
                let mut state = position(top, phase, active);
                // A spent/revealed effect also prohibits queens, independently of opening rules.
                state.opening_card = false;
                game::validate(&state).unwrap();
                let before = state.clone();
                for actual in [card("QH"), card("8H")] {
                    for queen in ["QH", "QD", "QC", "QS"].map(card) {
                        for suit in SUITS {
                            let action = queen_play(actual, queen, suit);
                            assert!(
                                game::play(&state, &action).is_err(),
                                "accepted {action:?} on {top}, {phase:?}, active={active}"
                            );
                            assert_eq!(state, before);
                        }
                    }
                }
                assert_no_queen(&state);
            }
        }
    }
}

#[test]
fn real_queens_can_still_bluff_legal_nonqueen_counters() {
    for top in effect_tops() {
        for phase in [Phase::Turn, Phase::Response] {
            for active in [false, true] {
                if phase == Phase::Response && !active {
                    continue;
                }
                let state = position(top, phase, active);
                let declaration = if active {
                    if top.rank() == 7 {
                        card("AH")
                    } else {
                        card("7S")
                    }
                } else {
                    Card::new(1, top.suit()) // Same-suit eight after an inactive effect.
                };
                let action = Move::Play {
                    actual_card: card("QH"),
                    declared_card: declaration,
                    chosen_suit: None,
                };
                let moves = game::move_generator(&state).unwrap();
                assert!(
                    moves.contains(&action),
                    "real queen lost legal bluff on {top}, {phase:?}, active={active}"
                );
                let after = game::play(&state, &action).unwrap();
                assert_eq!(after.top, declaration);
                assert_eq!(after.pile.last(), Some(&card("QH")));
                game::validate(&after).unwrap();
            }
        }
    }
}

#[test]
fn active_effects_keep_their_legal_counters_and_control_move_order() {
    for top in effect_tops() {
        let expected = if top.rank() == 7 {
            ["AH", "AD", "AC", "AS"].map(card).to_vec()
        } else if top == card("KS") {
            vec![card("7S")]
        } else {
            let mut sevens = ["7H", "7D", "7C", "7S"].map(card).to_vec();
            if top == card("7S") {
                sevens.push(card("KS"));
            }
            sevens
        };
        for phase in [Phase::Turn, Phase::Response] {
            let state = position(top, phase, true);
            assert_eq!(
                game::declarations(top, None, state.draw_penalty, state.skip_pending),
                expected
            );
            let moves = game::move_generator(&state).unwrap();
            let controls: Vec<_> = moves
                .iter()
                .copied()
                .filter(|m| !matches!(m, Move::Play { .. }))
                .collect();
            let expected_controls = match (phase, top.rank() == 7) {
                (Phase::Response, true) => vec![Move::Accept, Move::Challenge],
                (Phase::Response, false) => vec![Move::Accept, Move::Challenge, Move::Draw],
                (_, true) => vec![Move::Skip],
                (_, false) => vec![Move::Draw],
            };
            assert_eq!(controls, expected_controls);
            let expected_plays: Vec<_> = state.hands[0]
                .iter()
                .flat_map(|&actual| {
                    expected.iter().map(move |&declared| Move::Play {
                        actual_card: actual,
                        declared_card: declared,
                        chosen_suit: None,
                    })
                })
                .collect();
            assert_eq!(&moves[controls.len()..], expected_plays);
        }
    }
}

#[test]
fn drawing_on_an_opening_effect_never_unlocks_queen_declarations() {
    for top in effect_tops() {
        let mut state = position(top, Phase::Turn, false);
        assert!(state.opening_card);
        for _ in 0..2 {
            assert_no_queen(&state);
            state = game::play(&state, &Move::Draw).unwrap();
            assert_eq!(state.top, top);
            assert!(state.opening_card);
        }
        assert_no_queen(&state);
    }
}

#[test]
fn paying_a_penalty_or_consuming_a_skip_does_not_unlock_queens() {
    for top in effect_tops() {
        let state = position(top, Phase::Response, true);
        let action = if top.rank() == 7 {
            Move::Accept
        } else {
            Move::Draw
        };
        let after = game::play(&state, &action).unwrap();
        assert_eq!(after.top, top);
        assert_eq!(after.phase, Phase::Turn);
        assert_eq!(after.draw_penalty, 0);
        assert!(!after.skip_pending);
        assert!(!after.opening_card);
        assert_no_queen(&after);
    }
}

#[test]
fn challenge_revealed_effects_prohibit_queens_without_reactivating_effects() {
    for actual in effect_tops() {
        let mut state = position(actual, Phase::Response, true);
        state.top = card("10D");
        state.draw_penalty = 0;
        state.skip_pending = false;
        game::validate(&state).unwrap();
        let mut after = game::play(&state, &Move::Challenge).unwrap();
        assert_eq!(after.top, actual);
        assert_eq!(after.phase, Phase::Turn);
        assert_eq!(after.draw_penalty, 0);
        assert!(!after.skip_pending);
        assert!(!after.opening_card);
        assert_no_queen(&after);
        after = game::play(&after, &Move::Draw).unwrap();
        assert_eq!(after.top, actual);
        assert_no_queen(&after);
    }
}
