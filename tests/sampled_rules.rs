//! Procedural-rule adapter checks against public core transitions and privacy expectations.
use bluff_mau_mau::{
    engines::{
        advance_knowledge,
        cfr::Node,
        mccfr::{SampledGame, SampledNode},
        new_knowledge,
        observation::card_mask,
        solving::{RulesGame, RulesHistory, Scenario, Subgame, build_subgame},
    },
    game::{self, CARDS, Card, GameState, Move, Phase},
    rng::PythonRandom,
};
use std::collections::HashSet;

fn card(text: &str) -> Card {
    Card::parse(text).unwrap()
}

fn position(first: &[&str], second: &[&str], top: &str, recycle: bool) -> Scenario {
    let hands = [first, second].map(|hand| hand.iter().map(|text| card(text)).collect::<Vec<_>>());
    let top = card(top);
    let rest: Vec<_> = CARDS
        .into_iter()
        .filter(|c| *c != top && !hands.iter().any(|hand| hand.contains(c)))
        .collect();
    let (deck, mut pile) = if recycle {
        (vec![], rest)
    } else {
        (rest, vec![])
    };
    pile.push(top);
    let empty = hands.iter().position(Vec::is_empty);
    let state = GameState {
        deck,
        pile,
        hands,
        turn: empty.map_or(0, |player| 1 - player),
        top,
        chosen_suit: None,
        phase: Phase::Turn,
        draw_penalty: 0,
        skip_pending: false,
        provisional_winner: empty,
        winner: None,
        opening_card: !recycle && empty.is_none(),
        rng_state: PythonRandom::seed(76).state(),
    };
    game::validate(&state).unwrap();
    Scenario {
        state,
        knowledge: new_knowledge(top),
        probability: 1.0,
    }
}

fn reference_step(scenario: &mut Scenario, action: Move) {
    let after = game::play(&scenario.state, &action).unwrap();
    scenario.knowledge = advance_knowledge(&scenario.state, &action, &after, scenario.knowledge);
    scenario.state = after;
}

fn choose(game: &RulesGame, state: &mut RulesHistory, action: Move, rng: &mut PythonRandom) {
    let index = state
        .legal_actions()
        .iter()
        .position(|candidate| *candidate == action)
        .unwrap();
    game.advance(state, index, rng).unwrap();
}

fn key(game: &RulesGame, state: &RulesHistory) -> Vec<u8> {
    let SampledNode::Decision { key, .. } = game.node(state).unwrap() else {
        panic!("expected an undecided position")
    };
    key
}

#[test]
fn every_branch_of_small_fixed_worlds_matches_core_and_literal_tree() {
    fn visit(
        scenario: &Scenario,
        subgame: &Subgame,
        node: usize,
        path: &mut Vec<usize>,
        expected: GameState,
        horizon: usize,
    ) -> usize {
        let procedural = RulesGame::from_scenarios(vec![scenario.clone()], horizon, 0.0).unwrap();
        let mut rng = PythonRandom::seed((path.len() * 19 + node) as u64);
        let mut sampled = procedural.start(&mut rng).unwrap();
        for &action in path.iter() {
            // External sampling RNG changes must not change a fixed world's deck/RNG.
            rng.getrandbits(64);
            procedural.advance(&mut sampled, action, &mut rng).unwrap();
        }
        let actual = procedural.node(&sampled).unwrap();
        if expected.winner.is_some() || path.len() == horizon {
            let payoff = expected
                .winner
                .map_or(0.0, |winner| if winner == 0 { 1.0 } else { -1.0 });
            let SampledNode::Terminal(value) = actual else {
                panic!("missing terminal")
            };
            assert_eq!(value, payoff);
            let Node::Terminal(value) = &subgame.game.nodes[node] else {
                panic!("literal tree is not terminal")
            };
            assert_eq!(*value, payoff);
            assert!(procedural.advance(&mut sampled, 0, &mut rng).is_err());
            return 1;
        }
        let SampledNode::Decision {
            player,
            key,
            actions,
        } = actual
        else {
            panic!("unexpected terminal")
        };
        let Node::Decision {
            information_set,
            children,
        } = &subgame.game.nodes[node]
        else {
            panic!("literal tree is not a decision")
        };
        let core = game::move_generator(&expected).unwrap();
        assert_eq!(player, expected.turn);
        assert_eq!(actions, core.len());
        assert_eq!(
            sampled
                .legal_actions()
                .iter()
                .copied()
                .collect::<HashSet<_>>(),
            core.into_iter().collect()
        );
        assert_eq!(
            sampled.legal_actions(),
            subgame.action_lists[*information_set]
        );
        assert_eq!(key, subgame.information_keys[*information_set]);
        assert!(procedural.advance(&mut sampled, actions, &mut rng).is_err());
        let mut count = 1;
        for (index, action) in sampled.legal_actions().iter().enumerate() {
            let after = game::play(&expected, action).unwrap();
            path.push(index);
            count += visit(scenario, subgame, children[index], path, after, horizon);
            path.pop();
        }
        count
    }
    let mut ace = position(&["AH", "8D"], &["AD"], "9H", false);
    reference_step(
        &mut ace,
        Move::Play {
            actual_card: card("AH"),
            declared_card: card("AH"),
            chosen_suit: None,
        },
    );
    let mut penalty = position(&["KS", "9D"], &["QH"], "7S", false);
    reference_step(
        &mut penalty,
        Move::Play {
            actual_card: card("KS"),
            declared_card: card("KS"),
            chosen_suit: None,
        },
    );
    let mut final_return = position(&[], &["8H"], "9H", false);
    reference_step(
        &mut final_return,
        Move::Play {
            actual_card: card("8H"),
            declared_card: card("7H"),
            chosen_suit: None,
        },
    );
    let cases = [
        position(&["8H"], &["9S"], "8D", false),
        position(&["7H"], &["AS"], "QH", false),
        position(&["7D"], &["QS"], "9H", true),
        ace,
        penalty,
        final_return,
    ];
    let mut checked = 0;
    for scenario in cases {
        let subgame = build_subgame(std::slice::from_ref(&scenario), 2, 100_000).unwrap();
        let Node::Chance(roots) = &subgame.game.nodes[subgame.game.root] else {
            panic!("missing root chance")
        };
        checked += visit(
            &scenario,
            &subgame,
            roots[0].1,
            &mut vec![],
            scenario.state.clone(),
            2,
        );
    }
    assert!(checked > 1000, "only {checked} nodes checked");
}

#[test]
fn hidden_actual_play_merges_response_keys_and_challenge_reveal_splits_them() {
    let scenario = position(&["8H", "9H"], &["AC"], "10D", false);
    let game = RulesGame::from_scenarios(vec![scenario], 4, 0.0).unwrap();
    let mut response_keys = Vec::new();
    let mut revealed_keys = Vec::new();
    for actual in ["8H", "9H"] {
        let mut rng = PythonRandom::seed(5);
        let mut state = game.start(&mut rng).unwrap();
        choose(
            &game,
            &mut state,
            Move::Play {
                actual_card: card(actual),
                declared_card: card("10H"),
                chosen_suit: None,
            },
            &mut rng,
        );
        response_keys.push(key(&game, &state));
        choose(&game, &mut state, Move::Challenge, &mut rng);
        revealed_keys.push(key(&game, &state));
    }
    assert_eq!(response_keys[0], response_keys[1]);
    assert_ne!(revealed_keys[0], revealed_keys[1]);
}

#[test]
fn recycling_clears_current_pile_facts_without_forgetting_private_history() {
    let base = position(&["8C"], &["9D"], "9H", true);
    let remembered = card_mask(card("AH"));
    let mut variants = Vec::new();
    for knowledge in [
        base.knowledge,
        [base.knowledge[0] | remembered, base.knowledge[1]],
        [base.knowledge[0], base.knowledge[1] | remembered],
    ] {
        let mut scenario = base.clone();
        scenario.knowledge = knowledge;
        let game = RulesGame::from_scenarios(vec![scenario.clone()], 5, 0.0).unwrap();
        let mut rng = PythonRandom::seed(12);
        let mut history = game.start(&mut rng).unwrap();
        let before = key(&game, &history);
        for _ in 0..2 {
            choose(&game, &mut history, Move::Draw, &mut rng);
            reference_step(&mut scenario, Move::Draw);
        }
        assert_eq!(scenario.state.pile, vec![card("9H")]);
        assert_eq!(scenario.knowledge, new_knowledge(card("9H")));
        variants.push((before, key(&game, &history), scenario));
    }
    assert_ne!(
        variants[0].0, variants[1].0,
        "own private knowledge is observable"
    );
    assert_ne!(
        variants[0].1, variants[1].1,
        "past knowledge survives recycling in perfect recall"
    );
    assert_eq!(
        variants[0].0, variants[2].0,
        "opponent knowledge is private"
    );
    assert_eq!(
        variants[0].1, variants[2].1,
        "opponent knowledge must not leak through recycling"
    );
    // With history intentionally restarted, the now-identical current views agree.
    let restarted: Vec<_> = variants
        .into_iter()
        .map(|(_, _, scenario)| {
            let game = RulesGame::from_scenarios(vec![scenario], 1, 0.0).unwrap();
            let history = game.start(&mut PythonRandom::seed(4)).unwrap();
            key(&game, &history)
        })
        .collect();
    assert!(restarted.windows(2).all(|pair| pair[0] == pair[1]));
}

#[test]
fn fresh_deal_uses_the_current_rng_and_preserves_private_draw_order() {
    let game = RulesGame::fresh_deals(3, -0.25).unwrap();
    for seed in [0, 11, 91] {
        let mut expected_rng = PythonRandom::seed(seed);
        expected_rng.getrandbits(37);
        let mut actual_rng = expected_rng.clone();
        let dealer = expected_rng.randbelow(2);
        let mut cards = CARDS;
        expected_rng.shuffle(&mut cards);
        let mut hands = [Vec::new(), Vec::new()];
        for (index, &card) in cards[..10].iter().enumerate() {
            hands[if index % 2 == 0 { 1 - dealer } else { dealer }].push(card);
        }
        let state = GameState {
            deck: cards[11..].to_vec(),
            pile: vec![cards[10]],
            hands,
            turn: 1 - dealer,
            top: cards[10],
            chosen_suit: None,
            phase: Phase::Turn,
            draw_penalty: 0,
            skip_pending: false,
            provisional_winner: None,
            winner: None,
            opening_card: true,
            rng_state: expected_rng.state(),
        };
        let scenario = Scenario {
            knowledge: new_knowledge(state.top),
            state,
            probability: 1.0,
        };
        let literal = build_subgame(&[scenario], 1, 1000).unwrap();
        let sampled = game.start(&mut actual_rng).unwrap();
        assert_eq!(actual_rng.state(), expected_rng.state());
        assert_eq!(key(&game, &sampled), literal.information_keys[0]);
        assert_eq!(sampled.legal_actions(), literal.action_lists[0]);
    }
    let mut first = position(&["8C", "AH"], &["9D"], "9H", false);
    let game1 = RulesGame::from_scenarios(vec![first.clone()], 1, 0.0).unwrap();
    first.state.hands[0].reverse();
    let game2 = RulesGame::from_scenarios(vec![first], 1, 0.0).unwrap();
    let a = game1.start(&mut PythonRandom::seed(1)).unwrap();
    let b = game2.start(&mut PythonRandom::seed(1)).unwrap();
    assert_ne!(key(&game1, &a), key(&game2, &b));
    assert_eq!(a.legal_actions(), b.legal_actions());
    assert!(RulesGame::fresh_deals(0, 0.0).is_err());
    assert!(RulesGame::fresh_deals(1, f64::NAN).is_err());
}
