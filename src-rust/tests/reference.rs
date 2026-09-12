//! Independent golden outputs frozen from the original Python engine before
//! inspecting the Rust implementation. No Python runtime is used by these tests.
use bluff_mau_mau::engines::{
    baseline::Baseline,
    matches::{MatchOptions, run_match},
    observation::{self, Bot},
};
use bluff_mau_mau::{
    game::{self, Card, GameState, Move},
    rng::PythonRandom,
};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

fn reference() -> &'static Value {
    static FIXTURE: OnceLock<Value> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        serde_json::from_str(include_str!("fixtures/python-reference.json")).unwrap()
    })
}
fn canonical<T: Serialize>(value: &T) -> Vec<u8> {
    serde_json::to_vec(&serde_json::to_value(value).unwrap()).unwrap()
}
fn hash<T: Serialize>(value: &T) -> String {
    format!("{:x}", Sha256::digest(canonical(value)))
}
fn state(case: &Value) -> GameState {
    let mut value = case["state"].clone();
    value["rng_state"] = reference()["rng_states"][case["rng"].as_u64().unwrap() as usize].clone();
    serde_json::from_value(value).unwrap()
}
fn knowledge(value: &Value) -> [u32; 2] {
    std::array::from_fn(|i| {
        value[i].as_array().unwrap().iter().fold(0, |bits, c| {
            bits | (1u32 << Card::parse(c.as_str().unwrap()).unwrap().0)
        })
    })
}
fn bot(name: &str) -> Baseline {
    match name {
        "RandomLegal[uniform]" => Baseline::RandomLegal,
        "HonestFirst[B0-N0-C0]" => Baseline::HonestFirst,
        _ => {
            let p: Vec<u8> = name
                .trim_start_matches("MixedGreedy[")
                .trim_end_matches(']')
                .split('-')
                .map(|s| s[1..].parse().unwrap())
                .collect();
            Baseline::mixed(p[0], p[1], p[2]).unwrap()
        }
    }
}

#[test]
fn setup_matches_python_including_negative_and_1024_bit_seeds() {
    for case in reference()["setups"].as_array().unwrap() {
        let s = game::new_game_decimal(
            case["seed"].as_str().unwrap(),
            case["dealer"].as_u64().unwrap() as usize,
        )
        .unwrap();
        assert_eq!(hash(&s), case["hash"], "setup {case}");
    }
}

#[test]
fn exhaustive_ordered_moves_and_every_frozen_transition_match_python() {
    let mut branches = 0;
    for case in reference()["cases"].as_array().unwrap() {
        let s = state(case);
        let before = canonical(&s);
        let moves = game::move_generator(&s).unwrap_or_else(|e| panic!("{}: {e}", case["label"]));
        assert_eq!(
            moves.len() as u64,
            case["moves_count"].as_u64().unwrap(),
            "{}",
            case["label"]
        );
        assert_eq!(
            hash(&moves),
            case["moves_hash"],
            "ordered moves {}",
            case["label"]
        );
        let mut hash = Sha256::new();
        for i in case["indices"].as_array().unwrap() {
            let after = game::play(&s, &moves[i.as_u64().unwrap() as usize]).unwrap();
            game::validate(&after).unwrap_or_else(|e| panic!("after {}: {e}", case["label"]));
            hash.update(canonical(&after));
            hash.update(b"\n");
            branches += 1;
        }
        assert_eq!(
            format!("{:x}", hash.finalize()),
            case["after_hash"],
            "transitions {}",
            case["label"]
        );
        assert_eq!(canonical(&s), before, "input changed {}", case["label"]);
    }
    assert_eq!(branches, reference()["branches"].as_u64().unwrap());
}

#[test]
fn random_traces_preserve_full_state_and_personalized_pile_knowledge() {
    for trace in reference()["traces"].as_array().unwrap() {
        let mut s = game::new_game(
            trace["seed"].as_i64().unwrap(),
            trace["dealer"].as_u64().unwrap() as usize,
        )
        .unwrap();
        let mut k = observation::new_knowledge(s.top);
        for (step, record) in trace["steps"].as_array().unwrap().iter().enumerate() {
            let moves = game::move_generator(&s).unwrap();
            assert_eq!(
                hash(&moves),
                record["moves_hash"],
                "trace {} step{step}",
                trace["seed"]
            );
            let m = &moves[record["index"].as_u64().unwrap() as usize];
            let after = game::play(&s, m).unwrap();
            k = observation::advance_knowledge(&s, m, &after, k);
            assert_eq!(
                k,
                knowledge(&record["knowledge"]),
                "trace knowledge {} step{step}",
                trace["seed"]
            );
            assert_eq!(
                hash(&after),
                record["state_hash"],
                "trace state {} step{step}",
                trace["seed"]
            );
            s = after;
        }
    }
}

#[test]
fn every_grid_configuration_chooses_same_move_and_consumes_same_rng() {
    for case in reference()["policies"].as_array().unwrap() {
        let s = state(case);
        let moves = game::move_generator(&s).unwrap();
        let obs = observation::observe(&s, knowledge(&case["knowledge"]));
        for expected in case["expected"].as_array().unwrap() {
            let name = expected["name"].as_str().unwrap();
            let mut rng = PythonRandom::seed(case["seed"].as_u64().unwrap());
            let selected = bot(name).choose(&obs, &moves, &mut rng).unwrap();
            assert_eq!(
                serde_json::to_value(selected).unwrap(),
                expected["move"],
                "{name} state {}",
                case["state"]
            );
            assert_eq!(hash(&rng.state()), expected["rng_hash"], "rng {name}");
        }
    }
}

#[test]
fn complete_matches_match_python_final_states_and_all_counters() {
    for case in reference()["matches"].as_array().unwrap() {
        let a = bot(case["names"][0].as_str().unwrap());
        let b = bot(case["names"][1].as_str().unwrap());
        let result = run_match(
            [&a, &b],
            MatchOptions {
                seed: case["seed"].as_i64().unwrap(),
                bot_seeds: serde_json::from_value(case["bot_seeds"].clone()).unwrap(),
                max_decisions: case["max_decisions"].as_u64().unwrap() as usize,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            hash(&result.final_state),
            case["final_hash"],
            "match {} seed{}",
            case["names"],
            case["seed"]
        );
        assert_eq!(result.knowledge, knowledge(&case["knowledge"]));
        assert_eq!(json!(result.decisions), case["decisions"]);
        for (name, actual) in [
            ("plays", result.plays),
            ("bluffs", result.bluffs),
            ("responses", result.responses),
            ("challenges", result.challenges),
            ("correct_challenges", result.correct_challenges),
        ] {
            assert_eq!(json!(actual), case[name], "match counter {name}");
        }
    }
}

#[test]
fn malformed_states_and_illegal_actions_are_rejected_without_mutation() {
    let original = game::new_game(42, 0).unwrap();
    let mut invalid = Vec::new();
    let mut s = original.clone();
    s.deck.pop();
    invalid.push(s);
    let mut s = original.clone();
    s.deck[0] = s.pile[0];
    invalid.push(s);
    let mut s = original.clone();
    s.pile.clear();
    invalid.push(s);
    let mut s = original.clone();
    s.turn = 2;
    invalid.push(s);
    let mut s = original.clone();
    s.top = Card(32);
    invalid.push(s);
    let mut s = original.clone();
    s.deck[0] = Card(255);
    invalid.push(s);
    let mut s = original.clone();
    s.draw_penalty = 1;
    invalid.push(s);
    let mut s = original.clone();
    s.phase = game::Phase::Finished;
    invalid.push(s);
    for s in invalid {
        assert!(game::move_generator(&s).is_err());
        assert!(game::play(&s, &Move::Draw).is_err());
    }
    for m in [Move::Accept, Move::Challenge, Move::Skip] {
        assert!(game::play(&original, &m).is_err());
    }
    assert_eq!(original, game::new_game(42, 0).unwrap());
    assert!(game::new_game(0, 2).is_err());
}

#[test]
fn debug_http_views_history_and_explanations_match_python() {
    for case in reference()["http"].as_array().unwrap() {
        let mut game = bluff_mau_mau::server::DebugGame::with_seed(
            case["seed"].as_str().unwrap().parse().unwrap(),
        )
        .unwrap();
        assert_eq!(hash(&game.view().unwrap()), case["initial_hash"]);
        for (step, record) in case["steps"].as_array().unwrap().iter().enumerate() {
            let view = game
                .apply_move(game.version, record["index"].as_u64().unwrap() as usize)
                .unwrap();
            assert_eq!(
                hash(&view),
                record["hash"],
                "HTTP seed{} step{step}",
                case["seed"]
            );
        }
    }
}

#[test]
fn arena_schedule_matches_python() {
    for case in reference()["schedules"].as_array().unwrap() {
        let names: Vec<String> = serde_json::from_value(case["names"].clone()).unwrap();
        let actual = bluff_mau_mau::engines::arena::round_pairs(
            &names,
            case["seed"].as_i64().unwrap(),
            case["round"].as_u64().unwrap() as usize,
        )
        .unwrap();
        assert_eq!(json!(actual), case["pairs"]);
    }
}

#[test]
fn paired_matches_and_grid_evaluation_statistics_match_python() {
    use bluff_mau_mau::engines::{
        matches::{evaluate_candidates, round_robin},
        observation::Bot,
    };
    let bots = [
        Baseline::RandomLegal,
        Baseline::HonestFirst,
        Baseline::mixed(10, 10, 20).unwrap(),
    ];
    let roster: Vec<_> = bots.iter().map(|b| (b.name(), b as &dyn Bot)).collect();
    let actual = round_robin(&roster, &[13, 14], [19, 23], 120).unwrap();
    assert_eq!(
        serde_json::to_value(actual).unwrap(),
        reference()["evaluations"]["round_robin"]
    );
    let winner = Baseline::mixed(0, 80, 30).unwrap();
    let candidates: Vec<&dyn Bot> = vec![&bots[2], &winner];
    let actual = evaluate_candidates(&candidates, &roster[..2], &[13, 14], [19, 23], 120).unwrap();
    assert_eq!(
        serde_json::to_value(actual).unwrap(),
        reference()["evaluations"]["grid"]
    );
}

#[test]
fn extreme_imported_penalties_cannot_overflow_the_fast_match_or_history() {
    struct Counter;
    impl Bot for Counter {
        fn name(&self) -> String {
            "counter".into()
        }
        fn choose(
            &self,
            _: &observation::Observation,
            moves: &[Move],
            _: &mut PythonRandom,
        ) -> Result<Move, String> {
            Ok(*moves.iter().find(|m| matches!(m,Move::Play{declared_card,..} if *declared_card==Card::parse("7H").unwrap())).unwrap())
        }
    }
    let mut s = game::new_game(42, 0).unwrap();
    s.opening_card = false;
    s.top = Card::parse("7H").unwrap();
    s.draw_penalty = u32::MAX - 1;
    game::validate(&s).unwrap();
    let moves = game::move_generator(&s).unwrap();
    let m = Counter
        .choose(
            &observation::observe(&s, [0, 0]),
            &moves,
            &mut PythonRandom::seed(0),
        )
        .unwrap();
    assert!(game::play(&s, &m).is_err());
    assert!(
        run_match(
            [&Counter, &Counter],
            MatchOptions {
                initial_state: Some(s.clone()),
                max_decisions: 1,
                ..Default::default()
            }
        )
        .is_err()
    );
    // A huge challenge still draws all available cards and reports the exact shortage.
    s.draw_penalty = 2;
    let mut pending = game::play(&s, &m).unwrap();
    pending.draw_penalty = u32::MAX - 1;
    let before = pending.hands.clone();
    let mut debug = bluff_mau_mau::server::DebugGame::with_seed(0).unwrap();
    debug.state = pending;
    debug.history.clear();
    let view = debug.apply_move(debug.version, 1).unwrap();
    let loser = 1 - debug.state.turn;
    let drawn = debug.state.hands[loser].len() - before[loser].len();
    let unpaid = u64::from(u32::MAX) + 1 - drawn as u64;
    assert!(view["history"].as_array().unwrap().iter().any(|item| {
        item["text"]
            .as_str()
            .unwrap()
            .contains(&format!("{unpaid} unpaid cards cancelled"))
    }));
    game::validate(&debug.state).unwrap();
}
