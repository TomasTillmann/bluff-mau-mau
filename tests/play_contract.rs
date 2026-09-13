//! HTTP contracts prepared before inspecting the new play-session implementation.
//! Tests use the public TCP interface, never a bot's private cards or seeded play reset.
use bluff_mau_mau::{
    game::{self, CARDS, Card, Suit},
    move_explain::card_name,
    server::{DebugServer, create_server},
};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    io::{Read, Write},
    net::{Shutdown, TcpStream},
    time::Duration,
};

fn request(server: &DebugServer, method: &str, route: &str, body: &str) -> (u16, Value) {
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            let (stream, _) = server.listener.accept().unwrap();
            server.handle_connection(stream);
        });
        let mut client = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(15)))
            .unwrap();
        write!(client, "{method} {route} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", server.port, body.len()).unwrap();
        client.shutdown(Shutdown::Write).unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        worker.join().unwrap();
        let (headers, body) = response.split_once("\r\n\r\n").unwrap();
        assert!(headers.contains("Cache-Control: no-store"));
        let status = headers.split_whitespace().nth(1).unwrap().parse().unwrap();
        (status, serde_json::from_str(body).unwrap())
    })
}

fn get(server: &DebugServer, route: &str) -> Value {
    let (status, result) = request(server, "GET", route, "");
    assert_eq!(status, 200, "{route}: {result}");
    result
}

fn post(server: &DebugServer, route: &str, body: Value) -> Value {
    let (status, result) = request(server, "POST", route, &body.to_string());
    assert_eq!(status, 200, "{route}: {result}");
    result
}

fn new_game(server: &DebugServer, bot: &str) -> Value {
    post(server, "/api/play/new", json!({"bot_id":bot}))
}

fn play(server: &DebugServer, before: &Value, action: &Value) -> Value {
    let after = post(
        server,
        "/api/play/move",
        json!({"version":before["version"], "move_id":action["id"]}),
    );
    assert!(after["version"].as_u64().unwrap() > before["version"].as_u64().unwrap());
    assert_private_human_view(&after);
    after
}

fn assert_private_human_view(view: &Value) {
    assert_eq!(view["mode"], "play");
    assert_eq!(view["human_player"], 0);
    assert!(view["bot"]["id"].is_string());
    let hands = view["hands"].as_array().unwrap();
    assert_eq!(hands.len(), 2);
    assert_eq!(hands[1], json!([]), "opponent hand leaked");
    let own: Vec<Card> = serde_json::from_value(hands[0].clone()).unwrap();
    assert_eq!(own.len(), own.iter().collect::<HashSet<_>>().len());
    assert_eq!(view["hand_counts"][0].as_u64().unwrap() as usize, own.len());
    let opponent_count = view["hand_counts"][1].as_u64().unwrap();
    assert_eq!(
        own.len() as u64
            + opponent_count
            + view["deck_count"].as_u64().unwrap()
            + view["pile_count"].as_u64().unwrap(),
        32
    );
    let moves = view["legal_moves"].as_array().unwrap();
    if view["winner"].is_null() {
        assert_eq!(
            view["turn"], 0,
            "bot did not finish its automatic decisions"
        );
        assert!(!moves.is_empty());
    } else {
        assert!(moves.is_empty());
    }
    let top: Card = serde_json::from_value(view["top"].clone()).unwrap();
    let suit: Option<Suit> = serde_json::from_value(view["chosen_suit"].clone()).unwrap();
    let penalty = view["draw_penalty"].as_u64().unwrap() as u32;
    let skip = view["skip_pending"].as_bool().unwrap();
    let declarations = game::declarations(top, suit, penalty, skip);
    let mut ids = HashSet::new();
    for action in moves {
        assert!(ids.insert(action["id"].as_u64().unwrap()));
        match action["type"].as_str().unwrap() {
            "play" => {
                let actual: Card = serde_json::from_value(action["actual"].clone()).unwrap();
                let declared: Card = serde_json::from_value(action["declared"].clone()).unwrap();
                assert!(
                    own.contains(&actual),
                    "bot's actual card leaked through legal moves"
                );
                assert!(declarations.contains(&declared));
                assert_eq!(!action["chosen_suit"].is_null(), declared.rank() == 5);
            }
            "accept" | "challenge" => assert_eq!(view["phase"], "response"),
            "draw" => {
                assert!(!own.is_empty() && !skip);
                assert!(
                    view["deck_count"].as_u64().unwrap() > 0
                        || view["pile_count"].as_u64().unwrap() > 1
                );
            }
            "skip" => assert!(skip && !own.is_empty()),
            other => panic!("unknown action {other}"),
        }
    }
    // Actual-card fields belong only to selectable human moves, never snapshots/history.
    fn inspect(value: &Value, legal: bool) {
        match value {
            Value::Object(object) => {
                for (name, child) in object {
                    assert!(
                        !["deck", "pile", "rng_state", "actual_card", "opponent_hand"]
                            .contains(&name.as_str()),
                        "hidden state field {name}"
                    );
                    if name == "actual" {
                        assert!(legal, "hidden actual outside human legal actions");
                    }
                    if name == "hands" {
                        assert_eq!(child[1], json!([]));
                    }
                    inspect(child, legal || name == "legal_moves");
                }
            }
            Value::Array(values) => values.iter().for_each(|value| inspect(value, legal)),
            _ => {}
        }
    }
    inspect(view, false);
    for entry in view["history"].as_array().unwrap() {
        let text = entry["text"].as_str().unwrap();
        if entry["player"] == 1 && text.starts_with("Declared ") {
            let lower = text.to_lowercase();
            assert!(
                !["actual", "truthful", "bluff"]
                    .iter()
                    .any(|word| lower.contains(word))
            );
            let mentioned: usize = CARDS
                .iter()
                .map(|card| text.matches(&card_name(*card)).count())
                .sum();
            assert_eq!(mentioned, 1, "bot declaration exposed another card: {text}");
        }
    }
}

#[test]
fn catalog_contains_every_registered_bot_with_frozen_rating_sources() {
    let server = create_server(0).unwrap();
    let catalog = get(&server, "/api/bots");
    assert_eq!(catalog["rating_date"], "2026-09-13");
    let rows = catalog["bots"].as_array().unwrap();
    assert_eq!(rows.len(), 1335);
    let ids: HashSet<_> = rows.iter().map(|row| row["id"].as_str().unwrap()).collect();
    let by_name: HashMap<_, _> = rows
        .iter()
        .map(|row| (row["name"].as_str().unwrap(), row))
        .collect();
    assert_eq!(ids.len(), rows.len());
    assert_eq!(by_name.len(), rows.len());
    for row in rows {
        assert!(!row["id"].as_str().unwrap().is_empty());
        assert!(!row["family"].as_str().unwrap().is_empty());
        assert!(!row["description"].as_str().unwrap().is_empty());
        assert!(row["elo"].as_f64().unwrap().is_finite());
        let (games, wins, draws, losses) = (
            row["games"].as_u64().unwrap(),
            row["wins"].as_u64().unwrap(),
            row["draws"].as_u64().unwrap(),
            row["losses"].as_u64().unwrap(),
        );
        assert_eq!(games, wins + draws + losses);
        assert!(games > 0);
        let expected = (wins as f64 + 0.5 * draws as f64) / games as f64;
        assert!((row["score_rate"].as_f64().unwrap() - expected).abs() < 1e-8);
    }
    let ranking = include_str!("../docs/arena-ranking-2026-09-13.md");
    let mut baselines = 0;
    for line in ranking.lines() {
        let cells: Vec<_> = line.split('|').map(str::trim).collect();
        if cells.len() != 11 || cells[1].parse::<usize>().is_err() {
            continue;
        }
        let row = by_name[cells[2]];
        assert_eq!(row["elo_kind"], "arena");
        for (key, column) in [("games", 5), ("wins", 6), ("draws", 7), ("losses", 8)] {
            assert_eq!(
                row[key].as_u64().unwrap(),
                cells[column].replace(',', "").parse::<u64>().unwrap()
            );
        }
        assert!((row["elo"].as_f64().unwrap() - cells[9].parse::<f64>().unwrap()).abs() < 0.011);
        baselines += 1;
    }
    assert_eq!(baselines, 1333);
    for (name, wins, losses, elo) in [
        ("Tactical[C0]", 858, 142, 1678.89),
        ("BeliefSearch[S8-H40-conservative]", 852, 148, 1670.43),
    ] {
        let row = by_name[name];
        assert_eq!(row["elo_kind"], "performance");
        assert_eq!(row["games"], 1000);
        assert_eq!(row["wins"], wins);
        assert_eq!(row["draws"], 0);
        assert_eq!(row["losses"], losses);
        assert!((row["elo"].as_f64().unwrap() - elo).abs() < 0.011);
    }
}

#[test]
fn human_moves_return_only_human_decisions_and_challenges_reveal_known_bluffs() {
    let server = create_server(0).unwrap();
    let mut view = new_game(&server, "Tactical[C0]");
    assert_private_human_view(&view);
    assert_eq!(view["phase"], "turn");
    assert_eq!(view["hand_counts"], json!([5, 5]));
    let mut seen = HashSet::new();
    for preferred in [
        "draw",
        "accept",
        "play",
        "challenge",
        "play",
        "accept",
        "draw",
        "challenge",
        "play",
        "accept",
        "challenge",
        "play",
    ] {
        if !view["winner"].is_null() {
            view = new_game(&server, "Tactical[C0]");
        }
        let choices = view["legal_moves"].as_array().unwrap();
        let action = choices
            .iter()
            .find(|action| action["type"] == preferred)
            .or_else(|| choices.iter().find(|action| action["type"] == "play"))
            .unwrap_or(&choices[0])
            .clone();
        seen.insert(action["type"].as_str().unwrap().to_owned());
        view = play(&server, &view, &action);
        assert_eq!(get(&server, "/api/play/state"), view);
    }
    assert!(seen.contains("draw") && seen.contains("play") && seen.contains("accept"));
    let before = new_game(&server, "MixedGreedy[B0-N100-C100]");
    let bluff = before["legal_moves"]
        .as_array()
        .unwrap()
        .iter()
        .find(|action| action["type"] == "play" && action["actual"] != action["declared"])
        .unwrap()
        .clone();
    let actual: Card = serde_json::from_value(bluff["actual"].clone()).unwrap();
    let after = play(&server, &before, &bluff);
    assert!(
        after["history"].as_array().unwrap().iter().any(|entry| {
            let text = entry["text"].as_str().unwrap().to_lowercase();
            text.contains("reveal") && text.contains(&card_name(actual))
        }),
        "called bluff must publicly reveal its actual identity"
    );
}

#[test]
fn stale_or_malformed_requests_are_atomic_and_versions_survive_new_games() {
    let server = create_server(0).unwrap();
    let first = new_game(&server, "Tactical[C0]");
    let current = new_game(&server, "HonestFirst[B0-N0-C0]");
    assert!(current["version"].as_u64().unwrap() > first["version"].as_u64().unwrap());
    let (status, _) = request(
        &server,
        "POST",
        "/api/play/move",
        &json!({"version":first["version"],"move_id":first["legal_moves"][0]["id"]}).to_string(),
    );
    assert_eq!(status, 409);
    assert_eq!(get(&server, "/api/play/state"), current);
    for (route, body) in [
        ("/api/play/new", json!({"bot_id":"unknown bot"}).to_string()),
        (
            "/api/play/new",
            json!({"bot_id":"Tactical[C0]","seed":1}).to_string(),
        ),
        ("/api/play/new", "{}".into()),
        ("/api/play/new", "[]".into()),
        ("/api/play/new", "{invalid".into()),
        ("/api/play/new", json!({"bot_id":false}).to_string()),
        (
            "/api/play/move",
            json!({"version":current["version"],"move_id":u64::MAX}).to_string(),
        ),
        (
            "/api/play/move",
            json!({"version":current["version"],"move_id":false}).to_string(),
        ),
        (
            "/api/play/move",
            json!({"version":current["version"],"move_id":-1}).to_string(),
        ),
        (
            "/api/play/move",
            json!({"version":current["version"],"move_id":0,"seed":7}).to_string(),
        ),
        (
            "/api/play/move",
            json!({"version":true,"move_id":0}).to_string(),
        ),
    ] {
        let (status, _) = request(&server, "POST", route, &body);
        assert!(
            (400..500).contains(&status),
            "accepted invalid {route}: {body}"
        );
        assert_eq!(get(&server, "/api/play/state"), current);
    }
}

#[test]
fn debug_presets_and_normal_play_never_share_state_or_update_ratings() {
    let server = create_server(0).unwrap();
    let catalog = get(&server, "/api/bots");
    let debug = get(&server, "/api/state");
    let playing = new_game(&server, "Tactical[C0]");
    assert_eq!(get(&server, "/api/state"), debug);
    let stressed = post(&server, "/api/debug/max-hand", json!({"count":30}));
    assert_eq!(stressed["hands"][0].as_array().unwrap().len(), 30);
    assert_eq!(get(&server, "/api/play/state"), playing);
    let action = playing["legal_moves"]
        .as_array()
        .unwrap()
        .iter()
        .find(|action| action["type"] == "draw")
        .unwrap();
    play(&server, &playing, action);
    assert_eq!(get(&server, "/api/state"), stressed);
    let playing = get(&server, "/api/play/state");
    post(&server, "/api/new", json!({"seed":42}));
    assert_eq!(get(&server, "/api/play/state"), playing);
    assert_eq!(get(&server, "/api/bots"), catalog);
}
