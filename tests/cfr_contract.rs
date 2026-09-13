//! Original analytical and hidden-play checks were written before inspecting implementations.
//! The private draw-order regression was added after independent source review.
use bluff_mau_mau::engines::cfr::{Game, InformationSet, Node, Solver};
use std::collections::HashMap;

fn push(nodes: &mut Vec<Node>, node: Node) -> usize {
    let id = nodes.len();
    nodes.push(node);
    id
}

fn near(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{actual} != {expected} ± {tolerance}"
    );
}

#[test]
fn hidden_matching_pennies_has_zero_exploitability_at_uniform_play() {
    let game = Game {
        nodes: vec![
            Node::Terminal(1.0),
            Node::Terminal(-1.0),
            Node::Terminal(-1.0),
            Node::Terminal(1.0),
            Node::Decision {
                information_set: 1,
                children: vec![0, 1],
            },
            Node::Decision {
                information_set: 1,
                children: vec![2, 3],
            },
            Node::Decision {
                information_set: 0,
                children: vec![4, 5],
            },
        ],
        information_sets: vec![
            InformationSet {
                player: 0,
                actions: 2,
            },
            InformationSet {
                player: 1,
                actions: 2,
            },
        ],
        root: 6,
    };
    let mut solver = Solver::new(game).unwrap();
    for _ in 0..2 {
        let report = solver.report();
        near(report.value, 0.0, 1e-12);
        near(report.best_response_values[0], 0.0, 1e-12);
        near(report.best_response_values[1], 0.0, 1e-12);
        near(report.exploitability, 0.0, 1e-12);
        solver.train(100);
    }
    for strategy in solver.average_strategy() {
        near(strategy[0], 0.5, 1e-12);
        near(strategy[1], 0.5, 1e-12);
    }
}

#[test]
fn best_response_uses_chance_weights_and_one_action_for_hidden_worlds() {
    let game = Game {
        nodes: vec![
            Node::Terminal(1.0),
            Node::Terminal(-1.0),
            Node::Terminal(-1.0),
            Node::Terminal(1.0),
            Node::Decision {
                information_set: 0,
                children: vec![0, 1],
            },
            Node::Decision {
                information_set: 0,
                children: vec![2, 3],
            },
            Node::Chance(vec![(0.9, 4), (0.1, 5)]),
        ],
        information_sets: vec![InformationSet {
            player: 0,
            actions: 2,
        }],
        root: 6,
    };
    let mut solver = Solver::new(game).unwrap();
    let initial = solver.report();
    // A clairvoyant per-world response would incorrectly obtain 1.0.
    near(initial.value, 0.0, 1e-12);
    near(initial.best_response_values[0], 0.8, 1e-12);
    near(initial.best_response_values[1], 0.0, 1e-12);
    near(initial.nash_conv, 0.8, 1e-12);
    near(initial.exploitability, 0.4, 1e-12);
    solver.train(1000);
    let trained = solver.report();
    assert_eq!(trained.iterations, 1000);
    near(trained.value, 0.8, 0.002);
    assert!(trained.exploitability < 0.002);
}

#[test]
fn best_response_resolves_descendant_infosets_before_shallower_occurrences() {
    let mut nodes = vec![
        Node::Terminal(1.0),
        Node::Terminal(-1.0),
        Node::Terminal(0.4),
        Node::Terminal(0.0),
        Node::Terminal(0.4),
    ];
    let followup = push(
        &mut nodes,
        Node::Decision {
            information_set: 1,
            children: vec![0, 1],
        },
    );
    let first = push(
        &mut nodes,
        Node::Decision {
            information_set: 0,
            children: vec![followup, 2],
        },
    );
    let mut second = push(
        &mut nodes,
        Node::Decision {
            information_set: 0,
            children: vec![3, 4],
        },
    );
    for _ in 0..4 {
        second = push(&mut nodes, Node::Chance(vec![(1.0, second)]));
    }
    let root = push(&mut nodes, Node::Chance(vec![(0.5, first), (0.5, second)]));
    let solver = Solver::new(Game {
        nodes,
        information_sets: vec![
            InformationSet {
                player: 0,
                actions: 2,
            },
            InformationSet {
                player: 0,
                actions: 2,
            },
        ],
        root,
    })
    .unwrap();
    // Choosing left in both hidden worlds and then +1 obtains 0.5.
    // Optimizing the worlds separately incorrectly obtains (1 + 0.4) / 2.
    near(solver.report().best_response_values[0], 0.5, 1e-12);
}

fn kuhn_poker() -> Game {
    fn subtree(
        cards: [usize; 2],
        history: &str,
        nodes: &mut Vec<Node>,
        infos: &mut Vec<InformationSet>,
        ids: &mut HashMap<(usize, usize, String), usize>,
    ) -> usize {
        let showdown = if cards[0] > cards[1] { 1.0 } else { -1.0 };
        let terminal = match history {
            "pp" => Some(showdown),
            "bp" => Some(1.0),
            "bb" | "pbb" => Some(2.0 * showdown),
            "pbp" => Some(-1.0),
            _ => None,
        };
        if let Some(value) = terminal {
            return push(nodes, Node::Terminal(value));
        }
        let player = usize::from(history == "p" || history == "b");
        let key = (player, cards[player], history.to_owned());
        let information_set = *ids.entry(key).or_insert_with(|| {
            infos.push(InformationSet { player, actions: 2 });
            infos.len() - 1
        });
        let children = ["p", "b"]
            .into_iter()
            .map(|action| subtree(cards, &format!("{history}{action}"), nodes, infos, ids))
            .collect();
        push(
            nodes,
            Node::Decision {
                information_set,
                children,
            },
        )
    }
    let (mut nodes, mut information_sets, mut ids, mut deals) =
        (Vec::new(), Vec::new(), HashMap::new(), Vec::new());
    for first in 0..3 {
        for second in 0..3 {
            if first != second {
                let child = subtree(
                    [first, second],
                    "",
                    &mut nodes,
                    &mut information_sets,
                    &mut ids,
                );
                deals.push((1.0 / 6.0, child));
            }
        }
    }
    assert_eq!(information_sets.len(), 12);
    let root = push(&mut nodes, Node::Chance(deals));
    Game {
        nodes,
        information_sets,
        root,
    }
}

fn exhaustive_pure_response(game: &Game, profile: &[Vec<f64>], player: usize) -> f64 {
    fn value(game: &Game, profile: &[Vec<f64>], node: usize) -> f64 {
        match &game.nodes[node] {
            Node::Terminal(payoff) => *payoff,
            Node::Chance(children) => children
                .iter()
                .map(|(probability, child)| probability * value(game, profile, *child))
                .sum(),
            Node::Decision {
                information_set,
                children,
            } => children
                .iter()
                .enumerate()
                .map(|(action, child)| {
                    profile[*information_set][action] * value(game, profile, *child)
                })
                .sum(),
        }
    }
    let infos: Vec<_> = game
        .information_sets
        .iter()
        .enumerate()
        .filter_map(|(id, info)| (info.player == player).then_some(id))
        .collect();
    assert_eq!(infos.len(), 6);
    (0..1 << infos.len())
        .map(|choices| {
            let mut candidate = profile.to_vec();
            for (bit, info) in infos.iter().enumerate() {
                candidate[*info] = if choices & (1 << bit) == 0 {
                    vec![1.0, 0.0]
                } else {
                    vec![0.0, 1.0]
                };
            }
            value(game, &candidate, game.root) * if player == 0 { 1.0 } else { -1.0 }
        })
        .fold(f64::NEG_INFINITY, f64::max)
}

#[test]
fn kuhn_self_play_converges_to_its_known_value_and_small_exact_exploitability() {
    // Standard 3-card Kuhn, unit ante/bet. Its equilibrium value is -1/18.
    // An independent optimal policy is published by Google DeepMind in:
    // open_spiel/games/kuhn_poker/kuhn_poker.cc (GetOptimalPolicy).
    let mut solver = Solver::new(kuhn_poker()).unwrap();
    let initial = solver.report().exploitability;
    solver.train(20_000);
    let report = solver.report();
    assert_eq!(report.iterations, 20_000);
    near(report.value, -1.0 / 18.0, 0.005);
    assert!(
        report.exploitability < 0.02,
        "exploitability {}",
        report.exploitability
    );
    assert!(report.exploitability < initial / 10.0);
    near(
        report.nash_conv,
        report.best_response_values.iter().sum(),
        1e-12,
    );
    near(report.exploitability, report.nash_conv / 2.0, 1e-12);
    assert!(report.best_response_values[0] + 1e-12 >= report.value);
    assert!(report.best_response_values[1] + 1e-12 >= -report.value);
    let profile = solver.average_strategy();
    let independent_game = kuhn_poker();
    for player in 0..2 {
        near(
            report.best_response_values[player],
            exhaustive_pure_response(&independent_game, &profile, player),
            1e-12,
        );
    }
    for strategy in profile {
        assert!(strategy.iter().all(|p| p.is_finite() && *p >= 0.0));
        near(strategy.iter().sum(), 1.0, 1e-12);
    }
}

#[test]
fn merging_histories_that_forget_ones_own_action_is_rejected() {
    let game = Game {
        nodes: vec![
            Node::Terminal(1.0),
            Node::Terminal(-1.0),
            Node::Terminal(-1.0),
            Node::Terminal(1.0),
            Node::Decision {
                information_set: 1,
                children: vec![0, 1],
            },
            Node::Decision {
                information_set: 1,
                children: vec![2, 3],
            },
            Node::Decision {
                information_set: 0,
                children: vec![4, 5],
            },
        ],
        information_sets: vec![
            InformationSet {
                player: 0,
                actions: 2,
            },
            InformationSet {
                player: 0,
                actions: 2,
            },
        ],
        root: 6,
    };
    assert!(Solver::new(game).is_err());
}

#[test]
fn actual_game_adapter_hides_played_identity_until_the_challenge_reveals_it() {
    use bluff_mau_mau::{
        engines::{
            observation::new_knowledge,
            solving::{Scenario, build_subgame},
        },
        game::{self, CARDS, Card, GameState, Move, Phase},
        rng::PythonRandom,
    };
    let card = |text: &str| Card::parse(text).unwrap();
    let hands = [vec![card("8H"), card("9H")], vec![card("AC")]];
    let top = card("10D");
    let state = GameState {
        deck: CARDS
            .into_iter()
            .filter(|c| !hands[0].contains(c) && !hands[1].contains(c) && *c != top)
            .collect(),
        hands,
        pile: vec![top],
        turn: 0,
        top,
        chosen_suit: None,
        draw_penalty: 0,
        skip_pending: false,
        phase: Phase::Turn,
        provisional_winner: None,
        winner: None,
        rng_state: PythonRandom::seed(7).state(),
        opening_card: true,
    };
    let legal = game::move_generator(&state).unwrap();
    let subgame = build_subgame(
        &[Scenario {
            knowledge: new_knowledge(top),
            state,
            probability: 1.0,
        }],
        3,
        200_000,
    )
    .unwrap();
    let Node::Chance(worlds) = &subgame.game.nodes[subgame.game.root] else {
        panic!("missing initial chance node")
    };
    let Node::Decision {
        information_set: root_info,
        children,
    } = &subgame.game.nodes[worlds[0].1]
    else {
        panic!("missing root decision")
    };
    assert_eq!(subgame.action_lists[*root_info].len(), legal.len());
    assert!(
        legal
            .iter()
            .all(|action| subgame.action_lists[*root_info].contains(action))
    );
    let mut response_infos = Vec::new();
    let mut revealed_infos = Vec::new();
    for actual in ["8H", "9H"] {
        let action = Move::Play {
            actual_card: card(actual),
            declared_card: card("10H"),
            chosen_suit: None,
        };
        let index = subgame.action_lists[*root_info]
            .iter()
            .position(|candidate| *candidate == action)
            .unwrap();
        let Node::Decision {
            information_set: response_info,
            children: responses,
        } = &subgame.game.nodes[children[index]]
        else {
            panic!("missing response decision")
        };
        response_infos.push(*response_info);
        let challenge = subgame.action_lists[*response_info]
            .iter()
            .position(|action| *action == Move::Challenge)
            .unwrap();
        let Node::Decision {
            information_set: revealed_info,
            ..
        } = &subgame.game.nodes[responses[challenge]]
        else {
            panic!("missing post-reveal decision")
        };
        assert_eq!(subgame.game.information_sets[*revealed_info].player, 1);
        revealed_infos.push(*revealed_info);
    }
    assert_eq!(
        response_infos[0], response_infos[1],
        "opponent cannot observe the hidden played identity"
    );
    assert_ne!(
        revealed_infos[0], revealed_infos[1],
        "a challenge makes the actual identity public"
    );
}

#[test]
fn private_multi_card_draw_order_survives_the_opponents_turn() {
    use bluff_mau_mau::{
        engines::{
            observation::new_knowledge,
            solving::{Scenario, Subgame, build_subgame},
        },
        game::{self, CARDS, Card, GameState, Move, Phase},
        rng::PythonRandom,
    };
    fn child(subgame: &Subgame, node: usize, action: Move) -> (usize, usize) {
        let Node::Decision {
            information_set,
            children,
        } = &subgame.game.nodes[node]
        else {
            panic!("expected a decision")
        };
        let index = subgame.action_lists[*information_set]
            .iter()
            .position(|candidate| *candidate == action)
            .unwrap();
        (*information_set, children[index])
    }
    let card = |text: &str| Card::parse(text).unwrap();
    let hands = [vec![card("8C")], vec![card("9D")]];
    let pile = vec![card("7C"), card("7H")];
    let prefix = [card("AC"), card("AD"), card("10S")];
    let mut deck = prefix.to_vec();
    deck.extend(CARDS.into_iter().filter(|c| {
        !prefix.contains(c) && !hands[0].contains(c) && !hands[1].contains(c) && !pile.contains(c)
    }));
    let state = GameState {
        deck,
        hands,
        pile,
        turn: 0,
        top: card("7H"),
        chosen_suit: None,
        draw_penalty: 2,
        skip_pending: false,
        phase: Phase::Turn,
        provisional_winner: None,
        winner: None,
        rng_state: PythonRandom::seed(7).state(),
        opening_card: false,
    };
    let mut alternate = state.clone();
    alternate.deck.swap(0, 1);
    let knowledge = new_knowledge(state.top);
    let first_draw = game::play(&state, &Move::Draw).unwrap();
    let alternate_draw = game::play(&alternate, &Move::Draw).unwrap();
    assert_eq!(
        first_draw.hands[0],
        vec![card("8C"), card("AC"), card("AD")]
    );
    assert_eq!(
        alternate_draw.hands[0],
        vec![card("8C"), card("AD"), card("AC")]
    );
    let scenarios = [state, alternate].map(|state| Scenario {
        state,
        knowledge,
        probability: 0.5,
    });
    let subgame = build_subgame(&scenarios, 3, 100_000).unwrap();
    let Node::Chance(roots) = &subgame.game.nodes[subgame.game.root] else {
        panic!("expected initial worlds")
    };
    let mut before = Vec::new();
    let mut opponent = Vec::new();
    let mut after = Vec::new();
    for (_, node) in roots {
        let (own_info, opponent_node) = child(&subgame, *node, Move::Draw);
        let (opponent_info, own_node) = child(&subgame, opponent_node, Move::Draw);
        let Node::Decision {
            information_set: own_after,
            ..
        } = &subgame.game.nodes[own_node]
        else {
            panic!("expected next own turn")
        };
        before.push(own_info);
        opponent.push(opponent_info);
        after.push(*own_after);
    }
    assert_eq!(before[0], before[1], "the initial deck order is hidden");
    assert_eq!(
        opponent[0], opponent[1],
        "the opponent cannot see our draw order"
    );
    assert_ne!(
        after[0], after[1],
        "the drawing player remembers the ordered cards they received"
    );
}
