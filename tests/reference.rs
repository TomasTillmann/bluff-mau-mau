//! Independent golden outputs frozen from the original Python engine before
//! inspecting the Rust implementation. The queen correction intentionally differs
//! from some historical outputs; only unaffected goldens remain authoritative.
//! No Python runtime or legacy gameplay implementation is used by these tests.
use bluff_mau_mau::engines::{
    baseline::Baseline,
    matches::{MatchOptions, MatchResult, run_match},
    observation::{self, Bot},
};
use bluff_mau_mau::{
    game::{self, Card, GameState, Move, Phase},
    rng::{PythonRandom, RngState},
};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::{Mutex, OnceLock};

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

// This boundary follows the corrected rule, never the observed hash/result.
// Active ace skips already excluded queens in the original reference.
fn queen_rule_changes_moves(state: &GameState) -> bool {
    state.winner.is_none()
        && !state.hands[state.turn].is_empty()
        && !state.skip_pending
        && (matches!(state.top.rank(), 0 | 7) || state.top == Card::parse("KS").unwrap())
}

#[test]
fn setup_matches_python_including_negative_and_1024_bit_seeds() {
    assert_eq!(reference()["setups"].as_array().unwrap().len(), 140);
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
fn unchanged_ordered_moves_and_frozen_transitions_match_python() {
    let (mut positions, mut branches) = (0, 0);
    for case in reference()["cases"].as_array().unwrap() {
        let s = state(case);
        if queen_rule_changes_moves(&s) {
            continue;
        }
        positions += 1;
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
    assert_eq!((positions, branches), (1900, 49_948));
}

#[test]
fn unchanged_trace_prefixes_preserve_state_and_personalized_pile_knowledge() {
    let mut checked = 0;
    for trace in reference()["traces"].as_array().unwrap() {
        let mut s = game::new_game(
            trace["seed"].as_i64().unwrap(),
            trace["dealer"].as_u64().unwrap() as usize,
        )
        .unwrap();
        let mut k = observation::new_knowledge(s.top);
        for (step, record) in trace["steps"].as_array().unwrap().iter().enumerate() {
            if queen_rule_changes_moves(&s) {
                break;
            }
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
            checked += 1;
        }
    }
    assert_eq!(checked, 236, "retained trace decisions");
}

#[test]
fn every_grid_configuration_matches_unaffected_policy_goldens_and_rng() {
    let (mut positions, mut choices) = (0, 0);
    for case in reference()["policies"].as_array().unwrap() {
        let s = state(case);
        if queen_rule_changes_moves(&s) {
            continue;
        }
        positions += 1;
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
            choices += 1;
        }
    }
    assert_eq!((positions, choices), (3, 3 * 1333));
}

type Choices = Vec<(Move, RngState)>;

struct RecordingBot {
    policy: Baseline,
    choices: Mutex<Choices>,
}
impl Bot for RecordingBot {
    fn name(&self) -> String {
        self.policy.name()
    }
    fn choose(
        &self,
        obs: &observation::Observation,
        moves: &[Move],
        rng: &mut PythonRandom,
    ) -> Result<Move, String> {
        let action = self.policy.choose(obs, moves, rng)?;
        self.choices.lock().unwrap().push((action, rng.state()));
        Ok(action)
    }
}

// Exercise the checked public API, then derive counters from the recorded events.
// The production runner instead uses its trusted in-place path and rolling counters.
fn checked_match(
    bots: [&dyn Bot; 2],
    seed: i64,
    bot_seeds: [i64; 2],
    limit: usize,
) -> (MatchResult, [Choices; 2]) {
    let mut state = game::new_game(seed, 0).unwrap();
    let mut knowledge = observation::new_knowledge(state.top);
    let mut rngs = bot_seeds.map(PythonRandom::seed_signed);
    let mut choices: [Choices; 2] = std::array::from_fn(|_| Vec::new());
    let mut events = Vec::new();
    while state.winner.is_none() && events.len() < limit {
        let actor = state.turn;
        let moves = game::move_generator(&state).unwrap();
        let action = bots[actor]
            .choose(
                &observation::observe(&state, knowledge),
                &moves,
                &mut rngs[actor],
            )
            .unwrap();
        let after = game::play(&state, &action).unwrap();
        game::validate(&after).unwrap();
        events.push((
            actor,
            state.phase,
            state.pile.last() != Some(&state.top),
            action,
        ));
        choices[actor].push((action, rngs[actor].state()));
        knowledge = observation::advance_knowledge(&state, &action, &after, knowledge);
        state = after;
    }
    let count = |predicate: fn(Phase, bool, Move) -> bool| {
        std::array::from_fn(|seat| {
            events
                .iter()
                .filter(|&&(actor, phase, bluff, action)| {
                    actor == seat && predicate(phase, bluff, action)
                })
                .count()
        })
    };
    let result = MatchResult {
        final_state: state,
        knowledge,
        decisions: events.len(),
        plays: count(|_, _, action| matches!(action, Move::Play { .. })),
        bluffs: count(
            |_, _, action| matches!(action, Move::Play { actual_card, declared_card, .. } if actual_card != declared_card),
        ),
        responses: count(|phase, _, _| phase == Phase::Response),
        challenges: count(|_, _, action| action == Move::Challenge),
        correct_challenges: count(|_, bluff, action| bluff && action == Move::Challenge),
    };
    (result, choices)
}

#[test]
fn historical_match_inputs_replay_through_checked_public_rules_and_rng() {
    // Old final hashes encode obsolete queen actions. Preserve all 45 inputs,
    // comparing independent execution paths rather than blessing new snapshots.
    let mut checked = 0;
    for case in reference()["matches"].as_array().unwrap() {
        let a = bot(case["names"][0].as_str().unwrap());
        let b = bot(case["names"][1].as_str().unwrap());
        let bots = [a, b].map(|policy| RecordingBot {
            policy,
            choices: Mutex::new(Vec::new()),
        });
        let seed = case["seed"].as_i64().unwrap();
        let bot_seeds = serde_json::from_value(case["bot_seeds"].clone()).unwrap();
        let max_decisions = case["max_decisions"].as_u64().unwrap() as usize;
        let result = run_match(
            [&bots[0], &bots[1]],
            MatchOptions {
                seed,
                bot_seeds,
                max_decisions,
                ..Default::default()
            },
        )
        .unwrap();
        let (expected, choices) = checked_match([&a, &b], seed, bot_seeds, max_decisions);
        assert_eq!(
            result.final_state, expected.final_state,
            "match {} seed{}",
            case["names"], case["seed"]
        );
        assert_eq!(result.knowledge, expected.knowledge);
        assert_eq!(result.decisions, expected.decisions);
        for (name, actual, expected) in [
            ("plays", result.plays, expected.plays),
            ("bluffs", result.bluffs, expected.bluffs),
            ("responses", result.responses, expected.responses),
            ("challenges", result.challenges, expected.challenges),
            (
                "correct_challenges",
                result.correct_challenges,
                expected.correct_challenges,
            ),
        ] {
            assert_eq!(actual, expected, "match counter {name}");
        }
        for seat in 0..2 {
            assert_eq!(
                *bots[seat].choices.lock().unwrap(),
                choices[seat],
                "policy choices and RNG, seat {seat}"
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 45);
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
fn unchanged_debug_http_prefixes_match_python() {
    let mut checked = 0;
    for case in reference()["http"].as_array().unwrap() {
        let mut game = bluff_mau_mau::server::DebugGame::with_seed(
            case["seed"].as_str().unwrap().parse().unwrap(),
        )
        .unwrap();
        if queen_rule_changes_moves(&game.state) {
            continue;
        }
        assert_eq!(hash(&game.view().unwrap()), case["initial_hash"]);
        checked += 1;
        for (step, record) in case["steps"].as_array().unwrap().iter().enumerate() {
            let view = game
                .apply_move(game.version, record["index"].as_u64().unwrap() as usize)
                .unwrap();
            if queen_rule_changes_moves(&game.state) {
                break;
            }
            assert_eq!(
                hash(&view),
                record["hash"],
                "HTTP seed{} step{step}",
                case["seed"]
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 93, "retained HTTP views");
}

#[test]
fn arena_schedule_matches_python() {
    assert_eq!(reference()["schedules"].as_array().unwrap().len(), 27);
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

fn checked_statistics(candidate: &dyn Bot, opponents: &[&dyn Bot]) -> Value {
    let mut records = Vec::new();
    for opponent in opponents {
        for seed in [13, 14] {
            for seat in 0..2 {
                let bots = if seat == 0 {
                    [candidate, *opponent]
                } else {
                    [*opponent, candidate]
                };
                records.push((checked_match(bots, seed, [19, 23], 120).0, seat));
            }
        }
    }
    let sum = |field: fn(&MatchResult, usize) -> usize| {
        records
            .iter()
            .map(|(result, seat)| field(result, *seat))
            .sum::<usize>()
    };
    let games = records.len();
    let wins = sum(|r, s| usize::from(r.winner() == Some(s)));
    let losses = sum(|r, s| usize::from(r.winner() == Some(1 - s)));
    let truncated = sum(|r, _| usize::from(r.truncated()));
    let decisions = sum(|r, _| r.decisions);
    let plays = sum(|r, s| r.plays[s]);
    let bluffs = sum(|r, s| r.bluffs[s]);
    let responses = sum(|r, s| r.responses[s]);
    let challenges = sum(|r, s| r.challenges[s]);
    let correct = sum(|r, s| r.correct_challenges[s]);
    let rate = |n: usize, d: usize| (d != 0).then(|| n as f64 / d as f64);
    assert_eq!(games, 8);
    assert_eq!(wins + losses + truncated, games);
    json!({
        "games":games, "wins":wins, "losses":losses, "truncated":truncated,
        "total_game_decisions":decisions, "plays":plays, "bluffs":bluffs,
        "responses":responses, "challenges":challenges, "correct_challenges":correct,
        "mean_game_length":decisions as f64 / games as f64,
        "bluff_rate":rate(bluffs, plays), "challenge_rate":rate(challenges, responses),
        "challenge_success_rate":rate(correct, challenges)
    })
}

#[test]
fn paired_matches_and_candidate_evaluations_aggregate_checked_public_games() {
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
    assert_eq!(actual.len(), 3);
    for (name, candidate) in &roster {
        let opponents: Vec<_> = roster
            .iter()
            .filter(|(other, _)| other != name)
            .map(|(_, bot)| *bot)
            .collect();
        assert_eq!(
            json!(actual[name]),
            checked_statistics(*candidate, &opponents)
        );
    }
    let winner = Baseline::mixed(0, 80, 30).unwrap();
    let candidates: Vec<&dyn Bot> = vec![&bots[2], &winner];
    let actual = evaluate_candidates(&candidates, &roster[..2], &[13, 14], [19, 23], 120).unwrap();
    assert_eq!(actual.len(), 2);
    for candidate in candidates {
        assert_eq!(
            json!(actual[&candidate.name()]),
            checked_statistics(candidate, &[&bots[0], &bots[1]])
        );
    }
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
