//! Local debug HTTP bridge. Assets remain in the unchanged repository UI folders.
use crate::game::{self, CARDS, Card, GameState, Move, Phase};
use crate::move_explain::{card_name, explain_move, suit_name};
use crate::rng::PythonRandom;
use serde_json::{Value, json};
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpError {
    pub status: u16,
    pub message: String,
}
impl HttpError {
    fn bad(message: impl ToString) -> Self {
        Self {
            status: 400,
            message: message.to_string(),
        }
    }
}
impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.message.fmt(f)
    }
}
impl std::error::Error for HttpError {}

pub fn move_view(index: usize, action: &Move) -> Value {
    match action {
        Move::Play {
            actual_card,
            declared_card,
            chosen_suit,
        } => {
            json!({"id":index,"type":"play","actual":actual_card,"declared":declared_card,"chosen_suit":chosen_suit})
        }
        _ => {
            json!({"id":index,"type":match action { Move::Draw => "draw", Move::Challenge => "challenge", Move::Accept => "accept", Move::Skip => "skip", _ => unreachable!() }})
        }
    }
}

pub struct DebugGame {
    pub version: u64,
    pub state: GameState,
    pub top_status: String,
    pub move_explain: Option<Value>,
    pub history: Vec<Value>,
}
impl DebugGame {
    pub fn new() -> Result<Self, HttpError> {
        let mut seed = [0; 4];
        fs::File::open("/dev/urandom")
            .and_then(|mut file| file.read_exact(&mut seed))
            .map_err(HttpError::bad)?;
        Self::with_seed(i64::from(u32::from_ne_bytes(seed)))
    }
    pub fn with_seed(seed: i64) -> Result<Self, HttpError> {
        let state = game::new_game(seed, 1).map_err(HttpError::bad)?;
        let mut game = Self {
            version: 1,
            state,
            top_status: "Starting card".into(),
            move_explain: None,
            history: vec![],
        };
        game.new_history();
        Ok(game)
    }
    fn new_history(&mut self) {
        self.history = vec![
            json!({"player":null,"text":format!("New game. Player {} starts; starting card {}.", self.state.turn + 1, card_name(self.state.top))}),
        ];
    }
    pub fn reset_decimal(&mut self, seed: Option<&str>) -> Result<Value, HttpError> {
        let state = if let Some(seed) = seed {
            game::new_game_decimal(seed, 1).map_err(HttpError::bad)?
        } else {
            Self::new()?.state
        };
        self.state = state;
        self.version += 1;
        self.top_status = "Starting card".into();
        self.move_explain = None;
        self.new_history();
        self.view()
    }
    pub fn public_snapshot(&self) -> Value {
        let state = &self.state;
        json!({"phase":state.phase,"turn":state.turn,"hand_counts":[state.hands[0].len(),state.hands[1].len()],"top":state.top,"chosen_suit":state.chosen_suit,"draw_penalty":state.draw_penalty,"skip_pending":state.skip_pending,"opening_card":state.opening_card,"provisional_winner":state.provisional_winner,"winner":state.winner,"top_status":self.top_status})
    }
    pub fn view(&self) -> Result<Value, HttpError> {
        let state = &self.state;
        let legal_moves: Vec<_> = game::move_generator(state)
            .map_err(HttpError::bad)?
            .iter()
            .enumerate()
            .map(|(index, action)| move_view(index, action))
            .collect();
        Ok(
            json!({"version":self.version,"phase":state.phase,"turn":state.turn,"hands":state.hands,"top":state.top,"chosen_suit":state.chosen_suit,"draw_penalty":state.draw_penalty,"skip_pending":state.skip_pending,"opening_card":state.opening_card,"provisional_winner":state.provisional_winner,"winner":state.winner,"deck_count":state.deck.len(),"pile_count":state.pile.len(),"top_status":self.top_status,"history":self.history,"move_explain":self.move_explain,"legal_moves":legal_moves}),
        )
    }
    pub fn max_hand(&mut self, count: u64) -> Result<Value, HttpError> {
        if count != 30 && count != 31 {
            return Err(HttpError::bad("Debug hand count must be 30 or 31"));
        }
        let top = Card::parse("9S").map_err(HttpError::bad)?;
        let remaining = Card::parse("AS").map_err(HttpError::bad)?;
        let hand: Vec<_> = CARDS
            .into_iter()
            .filter(|&card| card != top && card != remaining)
            .collect();
        let mut position = GameState {
            deck: vec![],
            pile: vec![top],
            hands: [hand, vec![remaining]],
            turn: 0,
            top,
            chosen_suit: None,
            draw_penalty: 0,
            skip_pending: false,
            phase: Phase::Turn,
            provisional_winner: None,
            winner: None,
            rng_state: PythonRandom::seed(0).state(),
            opening_card: false,
        };
        if count == 31 {
            position.deck = vec![remaining];
            position.hands[1].clear();
            position.provisional_winner = Some(1);
            position = game::play(&position, &Move::Draw).map_err(HttpError::bad)?;
        }
        game::move_generator(&position).map_err(HttpError::bad)?;
        self.state = position;
        self.version += 1;
        self.top_status = "Starting card".into();
        self.move_explain = None;
        self.history = vec![
            json!({"player":null,"text":format!("Debug stress hand: {count} cards. {}", if count == 30 { "Both players still hold cards." } else { "Player 1 drew the last available card; empty-handed Player 2 won." })}),
        ];
        self.view()
    }
    pub fn apply_move(&mut self, version: u64, move_id: usize) -> Result<Value, HttpError> {
        if version != self.version {
            return Err(Self::stale());
        }
        let moves = game::move_generator(&self.state).map_err(HttpError::bad)?;
        let action = moves
            .get(move_id)
            .ok_or_else(|| HttpError::bad("Unknown move_id for this version"))?;
        let before = self.state.clone();
        let public_before = self.public_snapshot();
        self.state = game::play(&before, action).map_err(HttpError::bad)?;
        self.version += 1;
        self.record(&before, action);
        self.move_explain = explain_move(Some(&public_before), &self.public_snapshot(), 0);
        self.view()
    }
    fn stale() -> HttpError {
        HttpError {
            status: 409,
            message: "This game changed. Refresh and choose a move again.".into(),
        }
    }
    fn add(&mut self, player: usize, text: String) {
        self.history.push(json!({"player":player,"text":text}));
    }
    fn drawn(&mut self, before: &GameState, who: usize, requested: u64, reason: &str) {
        let count = self.state.hands[who]
            .len()
            .saturating_sub(before.hands[who].len());
        let mut text = format!(
            "Drew {count} card{}{reason}.",
            if count == 1 { "" } else { "s" }
        );
        if count < requested as usize {
            text.push_str(&format!(
                " {} unpaid cards cancelled; no more cards available.",
                requested as usize - count
            ));
        }
        self.add(who, text);
    }
    fn record(&mut self, before: &GameState, action: &Move) {
        let player = before.turn;
        if before.phase == Phase::Response && matches!(action, Move::Play { .. } | Move::Draw) {
            self.add(player, format!("Accepted {}.", card_name(before.top)));
            self.top_status = "Accepted".into();
        }
        match action {
            Move::Play {
                declared_card,
                chosen_suit,
                ..
            } => {
                let suit = chosen_suit
                    .map(|suit| format!("; continuing suit {}", suit_name(suit)))
                    .unwrap_or_default();
                self.add(
                    player,
                    format!("Declared {} face down{suit}.", card_name(*declared_card)),
                );
                self.top_status = "Awaiting response".into();
            }
            Move::Challenge => {
                let actual = *before.pile.last().expect("validated pile");
                let verdict = if actual == before.top {
                    "truthful declaration"
                } else {
                    "bluff caught"
                };
                self.add(
                    player,
                    format!(
                        "Challenged {}: revealed {}; {verdict}.",
                        card_name(before.top),
                        card_name(actual)
                    ),
                );
                self.drawn(
                    before,
                    1 - self.state.turn,
                    u64::from(before.draw_penalty) + 2,
                    " for the challenge",
                );
                self.top_status = "Revealed".into();
            }
            Move::Accept => {
                self.add(player, format!("Accepted {}.", card_name(before.top)));
                self.top_status = "Accepted".into();
                if before.hands[player].is_empty() && before.draw_penalty > 0 {
                    self.drawn(
                        before,
                        player,
                        u64::from(before.draw_penalty),
                        " for the return penalty",
                    );
                } else if before.skip_pending && before.hands.iter().any(|hand| !hand.is_empty()) {
                    self.add(player, "Skipped the turn under the ace.".into());
                }
            }
            Move::Draw => self.drawn(
                before,
                player,
                u64::from(before.draw_penalty.max(1)),
                if before.draw_penalty > 0 {
                    " for the penalty"
                } else {
                    ""
                },
            ),
            Move::Skip => self.add(player, "Skipped the turn under the ace.".into()),
        }
        if let Some(player) = before
            .provisional_winner
            .filter(|&player| !self.state.hands[player].is_empty())
        {
            self.add(player, "Returned to play.".into());
        }
        if let Some(player) = self.state.winner {
            self.add(player, "Won the game.".into());
        } else if let Some(player) = self
            .state
            .provisional_winner
            .filter(|_| self.state.provisional_winner != before.provisional_winner)
        {
            self.add(
                player,
                "Emptied their hand; victory awaits resolution.".into(),
            );
        }
    }
    pub fn apply_payload(&mut self, route: &str, payload: &Value) -> Result<Value, HttpError> {
        let payload = payload
            .as_object()
            .ok_or_else(|| HttpError::bad("JSON body must be an object"))?;
        match route {
            "/api/new" => {
                if payload.keys().any(|key| key != "seed")
                    || payload
                        .get("seed")
                        .is_some_and(|seed| integer(seed).is_none())
                {
                    return Err(HttpError::bad(
                        "New game accepts only an optional integer seed",
                    ));
                }
                self.reset_decimal(payload.get("seed").and_then(integer).as_deref())
            }
            "/api/debug/max-hand" => {
                if payload.len() != 1 || !payload.contains_key("count") {
                    return Err(HttpError::bad("Debug hand preset requires only count"));
                }
                let count = integer(&payload["count"])
                    .and_then(|count| count.parse::<u64>().ok())
                    .ok_or_else(|| HttpError::bad("Debug hand count must be 30 or 31"))?;
                self.max_hand(count)
            }
            "/api/move" => {
                if payload.len() != 2
                    || !payload.contains_key("version")
                    || !payload.contains_key("move_id")
                {
                    return Err(HttpError::bad("Move requires only version and move_id"));
                }
                let version = integer(&payload["version"]);
                let move_id = integer(&payload["move_id"]);
                let (Some(version), Some(move_id)) = (version, move_id) else {
                    return Err(HttpError::bad("Version and move_id must be integers"));
                };
                let version = version.parse::<u64>().map_err(|_| Self::stale())?;
                if version != self.version {
                    return Err(Self::stale());
                }
                let move_id = move_id
                    .parse::<usize>()
                    .map_err(|_| HttpError::bad("Unknown move_id for this version"))?;
                self.apply_move(version, move_id)
            }
            _ => Err(HttpError {
                status: 404,
                message: "Not found".into(),
            }),
        }
    }
}
fn integer(value: &Value) -> Option<String> {
    let Value::Number(number) = value else {
        return None;
    };
    let text = number.to_string();
    let text = if text == "-0" { "0".into() } else { text };
    (!text.bytes().any(|byte| matches!(byte, b'.' | b'e' | b'E'))).then_some(text)
}

// Accept JSON byte encodings with or without a Unicode byte-order mark.
fn json_body(body: &[u8]) -> Result<Value, HttpError> {
    let (body, width, little) = if body.starts_with(&[0xff, 0xfe, 0, 0]) {
        (&body[4..], 4, true)
    } else if body.starts_with(&[0, 0, 0xfe, 0xff]) {
        (&body[4..], 4, false)
    } else if body.starts_with(&[0xff, 0xfe]) {
        (&body[2..], 2, true)
    } else if body.starts_with(&[0xfe, 0xff]) {
        (&body[2..], 2, false)
    } else if body.starts_with(&[0xef, 0xbb, 0xbf]) {
        (&body[3..], 1, false)
    } else if body.len() >= 4 && body[0] == 0 {
        (body, if body[1] == 0 { 4 } else { 2 }, false)
    } else if body.len() >= 4 && body[1] == 0 {
        (body, if body[2] == 0 && body[3] == 0 { 4 } else { 2 }, true)
    } else if body.len() == 2 && body[0] == 0 {
        (body, 2, false)
    } else if body.len() == 2 && body[1] == 0 {
        (body, 2, true)
    } else {
        (body, 1, false)
    };
    if width == 1 {
        return serde_json::from_slice(body).map_err(HttpError::bad);
    }
    if body.len() % width != 0 {
        return Err(HttpError::bad("Incomplete Unicode JSON body"));
    }
    let decoded = if width == 2 {
        let words: Vec<u16> = body
            .as_chunks::<2>()
            .0
            .iter()
            .map(|bytes| {
                if little {
                    u16::from_le_bytes([bytes[0], bytes[1]])
                } else {
                    u16::from_be_bytes([bytes[0], bytes[1]])
                }
            })
            .collect();
        String::from_utf16(&words).map_err(HttpError::bad)?
    } else {
        body.as_chunks::<4>()
            .0
            .iter()
            .map(|bytes| {
                let bytes = [bytes[0], bytes[1], bytes[2], bytes[3]];
                let word = if little {
                    u32::from_le_bytes(bytes)
                } else {
                    u32::from_be_bytes(bytes)
                };
                char::from_u32(word).ok_or_else(|| HttpError::bad("Invalid Unicode JSON body"))
            })
            .collect::<Result<String, HttpError>>()?
    };
    serde_json::from_str(&decoded).map_err(HttpError::bad)
}

/// Kept separate from transport so repeated security headers cannot be merged away.
pub fn local_request(headers: &[(String, String)], port: u16) -> Result<(), HttpError> {
    let values = |name: &str| {
        headers
            .iter()
            .filter(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
            .collect::<Vec<_>>()
    };
    let hosts = values("Host");
    let allowed = [format!("127.0.0.1:{port}"), format!("localhost:{port}")];
    if hosts.len() != 1
        || !(allowed.contains(&hosts[0].to_ascii_lowercase())
            || port == 80
                && matches!(
                    hosts[0].to_ascii_lowercase().as_str(),
                    "127.0.0.1" | "localhost"
                ))
    {
        return Err(HttpError::bad("Use the local server address"));
    }
    let origins = values("Origin");
    if !origins.is_empty() && (origins.len() != 1 || origins[0] != format!("http://{}", hosts[0])) {
        return Err(HttpError::bad("Cross-origin requests are not allowed"));
    }
    if values("Sec-Fetch-Site")
        .first()
        .is_some_and(|value| !matches!(*value, "same-origin" | "none"))
    {
        return Err(HttpError::bad("Cross-origin requests are not allowed"));
    }
    Ok(())
}
fn body_length(headers: &[(String, String)]) -> Result<usize, HttpError> {
    let lengths: Vec<_> = headers
        .iter()
        .filter(|(key, _)| key.eq_ignore_ascii_case("Content-Length"))
        .map(|(_, value)| value)
        .collect();
    let length = if lengths.len() == 1
        && !lengths[0].is_empty()
        && lengths[0].bytes().all(|byte| byte.is_ascii_digit())
    {
        lengths[0].parse::<usize>().ok()
    } else {
        None
    };
    if headers
        .iter()
        .any(|(key, _)| key.eq_ignore_ascii_case("Transfer-Encoding"))
        || !length.is_some_and(|length| (1..=4096).contains(&length))
    {
        return Err(HttpError::bad(
            "Send a JSON body of at most 4096 bytes with Content-Length",
        ));
    }
    let content_type = headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("Content-Type"))
        .map(|(_, value)| value.split(';').next().unwrap_or("").trim());
    if !content_type.is_some_and(|value| value.eq_ignore_ascii_case("application/json")) {
        return Err(HttpError::bad("Content-Type must be application/json"));
    }
    Ok(length.unwrap())
}
fn request_path(url: &str) -> &str {
    let url = if let Some((_, rest)) = url.split_once("://") {
        rest.find('/').map(|index| &rest[index..]).unwrap_or("")
    } else {
        url
    };
    url.split(['?', '#']).next().unwrap_or("")
}
fn unquote(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if index + 2 < bytes.len() && bytes[index] == b'%' {
            let hex = |byte: u8| (byte as char).to_digit(16);
            if let (Some(high), Some(low)) = (hex(bytes[index + 1]), hex(bytes[index + 2])) {
                decoded.push((high * 16 + low) as u8);
                index += 3;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}
fn asset_type(extension: &str) -> Option<&'static str> {
    Some(match extension {
        "html" => "text/html",
        "css" => "text/css",
        "js" => "text/javascript",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "ico" => "image/vnd.microsoft.icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        _ => return None,
    })
}
fn static_asset(root: &Path, url: &str) -> Result<(Vec<u8>, &'static str), HttpError> {
    let decoded = unquote(request_path(url));
    if decoded.contains('\0') {
        return Err(HttpError::bad("embedded null byte"));
    }
    if !decoded.starts_with('/') {
        return Err(HttpError::bad("Request path must start with /"));
    }
    let path = if decoded == "/" {
        "/web/index.html"
    } else {
        &decoded
    };
    let folder = path.split('/').nth(1).unwrap_or("");
    let not_found = || HttpError {
        status: 404,
        message: "Not found".into(),
    };
    if !matches!(folder, "web" | "design-system" | "free-playing-cards") {
        return Err(not_found());
    }
    let asset = root
        .join(path.trim_start_matches('/'))
        .canonicalize()
        .map_err(|_| not_found())?;
    let extension = asset
        .extension()
        .and_then(|part| part.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let content_type = asset_type(&extension).ok_or_else(not_found)?;
    if !asset.starts_with(root.join(folder)) || !asset.is_file() {
        return Err(not_found());
    }
    Ok((fs::read(asset).map_err(HttpError::bad)?, content_type))
}

pub struct DebugServer {
    pub listener: TcpListener,
    pub game: Arc<Mutex<DebugGame>>,
    pub port: u16,
    root: PathBuf,
}
pub fn create_server(port: u16) -> Result<DebugServer, Box<dyn std::error::Error + Send + Sync>> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    let port = listener.local_addr()?.port();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).canonicalize()?;
    // ponytail: one shared debug game; add sessions for independent simultaneous games.
    Ok(DebugServer {
        listener,
        game: Arc::new(Mutex::new(DebugGame::new()?)),
        port,
        root,
    })
}
impl DebugServer {
    pub fn serve_forever(self) -> io::Result<()> {
        let server = Arc::new(self);
        for stream in server.listener.incoming() {
            let stream = stream?;
            let server = Arc::clone(&server);
            std::thread::spawn(move || server.handle_connection(stream));
        }
        Ok(())
    }
    /// One request per connection, with a five-second socket timeout.
    pub fn handle_connection(&self, mut stream: TcpStream) {
        if stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .is_err()
            || stream
                .set_write_timeout(Some(Duration::from_secs(5)))
                .is_err()
        {
            return;
        }
        let result = self.handle_inner(&mut BufReader::new(&mut stream));
        let (status, bytes, content_type) = match result {
            Ok((bytes, content_type)) => (200, bytes, content_type),
            Err(error) => (
                error.status,
                serde_json::to_vec(&json!({"error":error.message})).expect("JSON error"),
                "application/json",
            ),
        };
        let reason = match status {
            200 => "OK",
            400 => "Bad Request",
            404 => "Not Found",
            409 => "Conflict",
            414 => "Request-URI Too Long",
            431 => "Request Header Fields Too Large",
            501 => "Not Implemented",
            505 => "HTTP Version Not Supported",
            _ => "Error",
        };
        let header = format!(
            "HTTP/1.0 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",
            bytes.len()
        );
        let _ = stream
            .write_all(header.as_bytes())
            .and_then(|_| stream.write_all(&bytes));
        // Finish the response before closing a socket with an unread rejected body.
        let _ = stream.shutdown(std::net::Shutdown::Write);
    }
    fn handle_inner(
        &self,
        reader: &mut impl BufRead,
    ) -> Result<(Vec<u8>, &'static str), HttpError> {
        let line = read_http_line(reader, 414)?;
        let parts: Vec<_> = line.split_whitespace().collect();
        if parts.len() != 3 || !parts[2].starts_with("HTTP/") {
            return Err(HttpError::bad("Bad request syntax"));
        }
        let method = parts[0];
        let version = parts[2].strip_prefix("HTTP/").unwrap();
        let (major, minor) = version
            .split_once('.')
            .ok_or_else(|| HttpError::bad("Bad request version"))?;
        let major: u32 = major
            .parse()
            .map_err(|_| HttpError::bad("Bad request version"))?;
        minor
            .parse::<u32>()
            .map_err(|_| HttpError::bad("Bad request version"))?;
        if major >= 2 {
            return Err(HttpError {
                status: 505,
                message: "Invalid HTTP version".into(),
            });
        }
        let url = if parts[1].starts_with("//") {
            format!("/{}", parts[1].trim_start_matches('/'))
        } else {
            parts[1].to_owned()
        };
        let mut headers: Vec<(String, String)> = Vec::new();
        let mut header_lines = 0;
        loop {
            let line = read_http_line(reader, 431)?;
            header_lines += 1;
            if header_lines > 100 {
                return Err(HttpError {
                    status: 431,
                    message: "Too many headers".into(),
                });
            }
            if line.is_empty() {
                break;
            }
            if line.starts_with([' ', '\t']) {
                if let Some((_, value)) = headers.last_mut() {
                    value.push_str("\r\n");
                    value.push_str(&line);
                }
            } else if let Some((key, value)) = line.split_once(':') {
                headers.push((
                    key.to_owned(),
                    value.trim_start_matches([' ', '\t']).to_owned(),
                ));
            } else {
                return Err(HttpError::bad("Malformed HTTP header"));
            }
        }
        local_request(&headers, self.port)?;
        let route = request_path(&url);
        let value = if method == "GET" {
            if unquote(route) == "/api/state" {
                self.game.lock().map_err(HttpError::bad)?.view()?
            } else {
                return static_asset(&self.root, &url);
            }
        } else if method == "POST" {
            if !matches!(route, "/api/new" | "/api/move" | "/api/debug/max-hand") {
                return Err(HttpError {
                    status: 404,
                    message: "Not found".into(),
                });
            }
            let length = body_length(&headers)?;
            let mut body = vec![0; length];
            reader.read_exact(&mut body).map_err(|error| {
                if error.kind() == io::ErrorKind::UnexpectedEof {
                    HttpError::bad("Incomplete request body")
                } else {
                    HttpError::bad(error)
                }
            })?;
            let payload = json_body(&body)?;
            self.game
                .lock()
                .map_err(HttpError::bad)?
                .apply_payload(route, &payload)?
        } else {
            return Err(HttpError {
                status: 501,
                message: format!("Unsupported method ('{method}')"),
            });
        };
        Ok((
            serde_json::to_vec(&value).map_err(HttpError::bad)?,
            "application/json",
        ))
    }
}
fn read_http_line(reader: &mut impl BufRead, long_status: u16) -> Result<String, HttpError> {
    let mut line = Vec::new();
    reader
        .take(65537)
        .read_until(b'\n', &mut line)
        .map_err(HttpError::bad)?;
    if line.len() > 65536 {
        return Err(HttpError {
            status: long_status,
            message: "HTTP line too long".into(),
        });
    }
    if line.is_empty() {
        return Err(HttpError::bad("Incomplete HTTP request"));
    }
    while line
        .last()
        .is_some_and(|byte| matches!(byte, b'\r' | b'\n'))
    {
        line.pop();
    }
    // Decode HTTP header bytes as ISO-8859-1.
    Ok(line.into_iter().map(char::from).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn debug_versions_presets_and_invalid_payloads_are_atomic() {
        let mut game = DebugGame::with_seed(42).unwrap();
        let before = game.view().unwrap();
        assert_eq!(
            integer(&serde_json::from_str("-0").unwrap()),
            Some("0".into())
        );
        for (route, payload, status) in [
            ("/api/new", json!({"seed":true}), 400),
            ("/api/new", json!({"seed":null}), 400),
            ("/api/move", json!({"version":1,"move_id":false}), 400),
            ("/api/move", json!({"version":0,"move_id":0}), 409),
            ("/api/move", json!({"version":1,"move_id":-1}), 400),
            ("/api/debug/max-hand", json!({"count":32}), 400),
        ] {
            assert_eq!(
                game.apply_payload(route, &payload).unwrap_err().status,
                status
            );
            assert_eq!(game.view().unwrap(), before);
        }
        assert_eq!(
            game.max_hand(30).unwrap()["hands"][0]
                .as_array()
                .unwrap()
                .len(),
            30
        );
        let maximum = game.max_hand(31).unwrap();
        assert_eq!(maximum["hands"][0].as_array().unwrap().len(), 31);
        assert_eq!(maximum["winner"], 1);
        assert_eq!(maximum["legal_moves"], json!([]));
    }
    #[test]
    fn rejects_foreign_duplicate_host_origin_and_framing() {
        let headers = |pairs: &[(&str, &str)]| {
            pairs
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect::<Vec<_>>()
        };
        assert!(
            local_request(
                &headers(&[
                    ("Host", "localhost:8767"),
                    ("Origin", "http://localhost:8767")
                ]),
                8767
            )
            .is_ok()
        );
        for pairs in [
            vec![("Host", "example.com")],
            vec![("Host", "localhost:8767"), ("Host", "localhost:8767")],
            vec![
                ("Host", "localhost:8767"),
                ("Origin", "https://example.com"),
            ],
            vec![("Host", "localhost:8767"), ("Sec-Fetch-Site", "cross-site")],
        ] {
            assert_eq!(
                local_request(&headers(&pairs), 8767).unwrap_err().status,
                400
            );
        }
        for pairs in [
            vec![("Content-Length", "0")],
            vec![("Content-Length", "4097")],
            vec![("Content-Length", "2"), ("Content-Length", "2")],
            vec![("Content-Length", "2"), ("Transfer-Encoding", "chunked")],
            vec![("Content-Length", "+2")],
        ] {
            assert!(body_length(&headers(&pairs)).is_err());
        }
        assert_eq!(
            body_length(&headers(&[
                ("Content-Length", "2"),
                ("Content-Type", "application/json; charset=utf-8")
            ])),
            Ok(2)
        );
    }
    #[test]
    fn serves_only_public_assets() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        assert_eq!(
            static_asset(root, "/").unwrap().0,
            fs::read(root.join("web/index.html")).unwrap()
        );
        assert!(
            static_asset(
                root,
                "/free-playing-cards/svg%20cards/card%20fronts/hearts/7%20of%20hearts.svg"
            )
            .is_ok()
        );
        for path in [
            "/.git/config",
            "/src/server.rs",
            "/RULES.md",
            "/design-system/../src/server.rs",
            "/web/%2e%2e/.git/config",
            "/design-system/",
            "/design-system/README.md",
        ] {
            assert_eq!(static_asset(root, path).unwrap_err().status, 404, "{path}");
        }
    }
    #[test]
    fn real_http_transport_rejects_bad_framing_and_closes_every_response() {
        fn request(extra: &str, body: &str) -> (u16, Value) {
            let server = create_server(0).unwrap();
            let address = format!("127.0.0.1:{}", server.port);
            let worker = std::thread::spawn(move || {
                let (stream, _) = server.listener.accept().unwrap();
                server.handle_connection(stream);
            });
            let mut stream = TcpStream::connect(&address).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(6)))
                .unwrap();
            write!(
                stream,
                "POST /api/new HTTP/1.1\r\nHost: {address}\r\n{extra}\r\n{body}"
            )
            .unwrap();
            stream.shutdown(std::net::Shutdown::Write).unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            worker.join().unwrap();
            let (headers, body) = response.split_once("\r\n\r\n").unwrap();
            assert!(headers.contains("Connection: close"));
            assert!(headers.contains("Cache-Control: no-store"));
            assert!(headers.contains("X-Content-Type-Options: nosniff"));
            let status = headers.split_whitespace().nth(1).unwrap().parse().unwrap();
            (status, serde_json::from_str(body).unwrap())
        }
        let content = "Content-Type: application/json\r\n";
        let (status, result) =
            request(&format!("{content}Content-Length: 11\r\n"), "{\"seed\":42}");
        assert_eq!(status, 200);
        assert_eq!(result["turn"], 0);
        assert_eq!(result["version"], 2);
        let (status, _) = request(
            &format!("{content}Content-Length: 4097\r\n"),
            &"x".repeat(4097),
        );
        assert_eq!(status, 400);
        for (extra, body, expected) in [
            (
                format!("{content}Content-Length: 2\r\nContent-Length: 2\r\n"),
                "{}",
                "Send a JSON body of at most 4096 bytes with Content-Length",
            ),
            (
                format!("{content}Content-Length: 2\r\nTransfer-Encoding: chunked\r\n"),
                "{}",
                "Send a JSON body of at most 4096 bytes with Content-Length",
            ),
            (
                format!("{content}Content-Length: 4\r\n"),
                "{}",
                "Incomplete request body",
            ),
            (
                format!("{content}Content-Length: 2\r\nOrigin: https://example.com\r\n"),
                "{}",
                "Cross-origin requests are not allowed",
            ),
        ] {
            let (status, result) = request(&extra, body);
            assert_eq!(status, 400);
            assert_eq!(result["error"], expected);
        }
    }
    #[test]
    fn json_byte_encodings_accept_supported_unicode_formats() {
        let expected = json!({"seed":42});
        let text = r#"{"seed":42}"#;
        for little in [false, true] {
            let utf16: Vec<u8> = text
                .encode_utf16()
                .flat_map(|word| {
                    if little {
                        word.to_le_bytes()
                    } else {
                        word.to_be_bytes()
                    }
                })
                .collect();
            let utf32: Vec<u8> = text
                .chars()
                .flat_map(|character| {
                    if little {
                        (character as u32).to_le_bytes()
                    } else {
                        (character as u32).to_be_bytes()
                    }
                })
                .collect();
            assert_eq!(json_body(&utf16).unwrap(), expected);
            assert_eq!(json_body(&utf32).unwrap(), expected);
            let mut utf16_bom = if little {
                vec![0xff, 0xfe]
            } else {
                vec![0xfe, 0xff]
            };
            utf16_bom.extend(utf16);
            assert_eq!(json_body(&utf16_bom).unwrap(), expected);
            let mut utf32_bom = if little {
                vec![0xff, 0xfe, 0, 0]
            } else {
                vec![0, 0, 0xfe, 0xff]
            };
            utf32_bom.extend(utf32);
            assert_eq!(json_body(&utf32_bom).unwrap(), expected);
        }
        let mut utf8_bom = vec![0xef, 0xbb, 0xbf];
        utf8_bom.extend(text.as_bytes());
        assert_eq!(json_body(&utf8_bom).unwrap(), expected);
        assert!(json_body(&[0xff, 0xfe, 0]).is_err());
    }
}
