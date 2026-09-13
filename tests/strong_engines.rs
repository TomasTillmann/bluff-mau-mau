//! Black-box contracts written from the rules and public API without inspecting
//! either new engine's implementation. Playing strength belongs in held-out matches.
use bluff_mau_mau::{
    engines::{
        observation::{Bot, advance_knowledge, card_mask, new_knowledge, observe},
        search::BeliefSearch,
        tactical::Tactical,
    },
    game::{self, CARDS, Card, GameState, Move, Phase},
    rng::PythonRandom,
};

fn card(text: &str) -> Card {
    Card::parse(text).unwrap()
}

fn response(hands: [&[&str]; 2], pile: &[&str], declared: &str) -> GameState {
    let hands = hands.map(|hand| hand.iter().map(|text| card(text)).collect::<Vec<_>>());
    let pile = pile.iter().map(|text| card(text)).collect::<Vec<_>>();
    let top = card(declared);
    let state = GameState {
        deck: CARDS
            .into_iter()
            .filter(|c| !hands[0].contains(c) && !hands[1].contains(c) && !pile.contains(c))
            .collect(),
        provisional_winner: if hands[0].is_empty() {
            Some(0)
        } else {
            hands[1].is_empty().then_some(1)
        },
        hands,
        pile,
        turn: 0,
        top,
        chosen_suit: None,
        draw_penalty: game::contribution(top),
        skip_pending: top.rank() == 7,
        phase: Phase::Response,
        winner: None,
        rng_state: PythonRandom::seed(0).state(),
        opening_card: false,
    };
    game::validate(&state).unwrap();
    state
}

fn choose(bot: &dyn Bot, state: &GameState, knowledge: [u32; 2], seed: u64) -> Move {
    let legal = game::move_generator(state).unwrap();
    let action = bot
        .choose(
            &observe(state, knowledge),
            &legal,
            &mut PythonRandom::seed(seed),
        )
        .unwrap();
    assert!(
        legal.contains(&action),
        "{} returned {action:?}",
        bot.name()
    );
    action
}

#[test]
fn parameters_and_empty_legal_lists_are_checked() {
    assert!(Tactical::new(0).is_ok());
    assert!(Tactical::new(100).is_ok());
    assert!(Tactical::new(101).is_err());
    assert!(Tactical::new(255).is_err());
    assert!(BeliefSearch::new(1, 1).is_ok());
    assert!(BeliefSearch::new(0, 1).is_err());
    assert!(BeliefSearch::new(1, 0).is_err());
    let state = game::new_game(17, 0).unwrap();
    let observation = observe(&state, new_knowledge(state.top));
    for bot in [
        &Tactical::default() as &dyn Bot,
        &BeliefSearch::new(2, 4).unwrap(),
    ] {
        assert!(
            bot.choose(&observation, &[], &mut PythonRandom::seed(7))
                .is_err()
        );
    }
}

#[test]
fn public_positions_produce_legal_reproducible_moves() {
    let tactical = Tactical::default();
    let search = BeliefSearch::new(2, 4).unwrap();
    let bots: [&dyn Bot; 2] = [&tactical, &search];
    for seed in 0..8 {
        let mut state = game::new_game(seed, 0).unwrap();
        let mut knowledge = new_knowledge(state.top);
        let mut explorer = PythonRandom::seed(seed as u64 + 1000);
        for step in 0..32 {
            if state.winner.is_some() {
                break;
            }
            let legal = game::move_generator(&state).unwrap();
            for bot in bots {
                let action = choose(bot, &state, knowledge, step);
                assert_eq!(
                    action,
                    choose(bot, &state, knowledge, step),
                    "{}",
                    bot.name()
                );
            }
            // Follow independent legal actions, not just states preferred by the engines.
            let action = legal[explorer.randbelow(legal.len())];
            let after = game::play(&state, &action).unwrap();
            knowledge = advance_knowledge(&state, &action, &after, knowledge);
            state = after;
        }
    }
}

#[test]
fn unseen_hands_pile_identity_deck_order_and_game_rng_do_not_change_decisions() {
    let first = response([&["9H", "AC"], &["JC", "QS"]], &["7D", "8D"], "10H");
    let mut second = first.clone();
    std::mem::swap(&mut second.hands[1][0], &mut second.deck[0]);
    std::mem::swap(&mut second.pile[1], &mut second.deck[1]);
    second.deck.reverse();
    second.rng_state = PythonRandom::seed(999).state();
    game::validate(&second).unwrap();
    let knowledge = [card_mask(card("7D")); 2];
    assert_ne!(first, second);
    assert_eq!(observe(&first, knowledge), observe(&second, knowledge));
    assert_eq!(
        game::move_generator(&first).unwrap(),
        game::move_generator(&second).unwrap()
    );
    for bot in [
        &Tactical::default() as &dyn Bot,
        &BeliefSearch::new(4, 8).unwrap(),
    ] {
        for seed in 0..8 {
            assert_eq!(
                choose(bot, &first, knowledge, seed),
                choose(bot, &second, knowledge, seed)
            );
        }
    }
}

#[test]
fn guaranteed_wins_outrank_challenges_and_known_bluffs_are_called() {
    let known_in_hand = response([&["9H", "AC"], &["JC", "QS"]], &["7D", "8D"], "9H");
    let known_in_pile = response([&["9C", "AC"], &["JC", "QS"]], &["9H", "8D"], "9H");
    let already_out = response([&[], &["JC"]], &["9H", "8D"], "9H");
    let last_ace = response([&["AS"], &["9C"]], &["7D", "AH"], "AH");
    let mut final_return = response([&["7H"], &[]], &["7D", "9H"], "9H");
    final_return.phase = Phase::Turn;
    game::validate(&final_return).unwrap();
    for bot in [
        &Tactical::default() as &dyn Bot,
        &BeliefSearch::new(2, 4).unwrap(),
    ] {
        assert_eq!(choose(bot, &known_in_hand, [0; 2], 0), Move::Challenge);
        let known = [card_mask(card("9H")); 2];
        assert_eq!(choose(bot, &known_in_pile, known, 0), Move::Challenge);
        assert_eq!(choose(bot, &already_out, known, 0), Move::Accept);
        for (state, final_card) in [(&last_ace, "AS"), (&final_return, "7H")] {
            assert_eq!(
                choose(bot, state, [0; 2], 0),
                Move::Play {
                    actual_card: card(final_card),
                    declared_card: card(final_card),
                    chosen_suit: None,
                }
            );
        }
    }
}

#[test]
fn tactical_counter_ace_does_not_accept_away_the_chance_to_play() {
    let state = response([&["AC", "9H"], &["JC", "QS"]], &["7D", "AH"], "AH");
    let expected = Move::Play {
        actual_card: card("AC"),
        declared_card: card("AC"),
        chosen_suit: None,
    };
    for challenge in [0, 40, 100] {
        assert_eq!(
            choose(&Tactical::new(challenge).unwrap(), &state, [0; 2], 0),
            expected
        );
    }
}
