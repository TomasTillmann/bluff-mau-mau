"""Local debug UI for the engine: python server.py [--port 8767]."""

import argparse
import json
import mimetypes
from dataclasses import replace
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from secrets import randbits
from threading import Lock
from urllib.parse import unquote, urlsplit

from bluff_mau_mau import CARDS, Accept, Card, Challenge, Draw, GameState, MoveGenerator, NewGame, Play, PlayCard, Skip
from move_explain import explain_move

ROOT = Path(__file__).resolve().parent
SUIT_NAMES = {"H": "hearts", "D": "diamonds", "C": "clubs", "S": "spades"}
ASSET_TYPES = {".html", ".css", ".js", ".svg", ".png", ".jpg", ".jpeg", ".webp",
               ".ico", ".woff", ".woff2", ".ttf", ".otf"}


def code(card):
    return card.rank + card.suit


def card_name(card):
    rank = {"J": "jack", "Q": "queen", "K": "king", "A": "ace"}.get(card.rank, card.rank)
    return f"{rank} of {SUIT_NAMES[card.suit]}"


def move_view(index, move):
    if isinstance(move, PlayCard):
        return {"id": index, "type": "play", "actual": code(move.actual_card),
                "declared": code(move.declared_card), "chosen_suit": move.chosen_suit}
    return {"id": index, "type": type(move).__name__.lower()}


class StaleMove(ValueError):
    pass


class DebugGame:
    def __init__(self):
        self.version = 0
        self.new()

    def new(self, seed=None):
        if seed is not None and type(seed) is not int:
            raise ValueError("Seed must be an integer")
        self.state = NewGame(randbits(32) if seed is None else seed, dealer=1)
        self.version += 1
        self.top_status = "Starting card"
        self.move_explain = None
        self.history = [{"player": None, "text":
            f"New game. Player {self.state.turn + 1} starts; starting card {card_name(self.state.top)}."}]
        return self.view()

    def public_snapshot(self):
        state = self.state
        return {"phase": state.phase, "turn": state.turn,
                "hand_counts": [len(hand) for hand in state.hands],
                "top": code(state.top), "chosen_suit": state.chosen_suit,
                "draw_penalty": state.draw_penalty, "skip_pending": state.skip_pending,
                "provisional_winner": state.provisional_winner, "winner": state.winner,
                "top_status": self.top_status}

    def view(self):
        state = self.state
        after_accept_actions = []
        if state.phase == "response":
            accepted = Play(state, Accept())
            if accepted.phase == "turn" and accepted.turn == state.turn:
                after_accept_actions = [type(move).__name__.lower() for move in MoveGenerator(accepted)
                                        if isinstance(move, (Draw, Skip))]
        return {
            "version": self.version, "phase": state.phase, "turn": state.turn,
            "hands": [[code(card) for card in hand] for hand in state.hands],
            "top": code(state.top), "chosen_suit": state.chosen_suit,
            "draw_penalty": state.draw_penalty, "skip_pending": state.skip_pending,
            "provisional_winner": state.provisional_winner, "winner": state.winner,
            "deck_count": len(state.deck), "pile_count": len(state.pile),
            "top_status": self.top_status, "history": list(self.history),
            "move_explain": self.move_explain,
            "legal_moves": [move_view(i, move) for i, move in enumerate(MoveGenerator(state))],
            "after_accept_actions": after_accept_actions,
        }

    def max_hand(self, count):
        if type(count) is not int or count not in (30, 31):
            raise ValueError("Debug hand count must be 30 or 31")
        top, remaining = Card("9", "S"), Card("A", "S")
        hand = tuple(card for card in CARDS if card not in (top, remaining))
        position = GameState(deck=(), pile=(top,), hands=(hand, (remaining,)), turn=0, top=top)
        if count == 31:
            position = Play(replace(position, deck=(remaining,), hands=(hand, ()),
                                    provisional_winner=1), Draw())
        MoveGenerator(position)
        self.state = position
        self.version += 1
        self.top_status = "Starting card"
        self.move_explain = None
        self.history = [{"player": None, "text": f"Debug stress hand: {count} cards. " +
                         ("Both players still hold cards." if count == 30 else
                          "Player 1 drew the last available card; empty-handed Player 2 won.")}]
        return self.view()

    def move(self, version, move_id):
        if type(version) is not int or type(move_id) is not int:
            raise ValueError("Version and move_id must be integers")
        if version != self.version:
            raise StaleMove("This game changed. Refresh and choose a move again.")
        moves = MoveGenerator(self.state)
        if not 0 <= move_id < len(moves):
            raise ValueError("Unknown move_id for this version")
        before, move = self.state, moves[move_id]
        public_before = self.public_snapshot()
        self.state = Play(before, move)
        self.version += 1
        self.record(before, move)
        self.move_explain = explain_move(public_before, self.public_snapshot())
        return self.view()

    def record(self, before, move):
        after, player = self.state, before.turn

        def add(who, text):
            self.history.append({"player": who, "text": text})

        def drawn(who, requested, reason):
            count = len(after.hands[who]) - len(before.hands[who])
            text = f"Drew {count} card{'s' if count != 1 else ''}{reason}."
            if count < requested:
                text += f" {requested - count} unpaid cards cancelled; no more cards available."
            add(who, text)

        if isinstance(move, PlayCard):
            suit = f"; continuing suit {SUIT_NAMES[move.chosen_suit]}" if move.chosen_suit else ""
            add(player, f"Declared {card_name(move.declared_card)} face down{suit}.")
            self.top_status = "Awaiting response"
        elif isinstance(move, Challenge):
            actual = card_name(before.pile[-1])
            verdict = "truthful declaration" if before.pile[-1] == before.top else "bluff caught"
            add(player, f"Challenged {card_name(before.top)}: revealed {actual}; {verdict}.")
            drawn(1 - after.turn, before.draw_penalty + 2, " for the challenge")
            self.top_status = "Revealed"
        elif isinstance(move, Accept):
            add(player, f"Accepted {card_name(before.top)}.")
            self.top_status = "Accepted"
            if not before.hands[player] and before.draw_penalty:
                drawn(player, before.draw_penalty, " for the return penalty")
            elif not before.hands[player] and before.skip_pending and after.winner is None:
                add(player, "Empty hand skipped under the ace.")
        elif isinstance(move, Draw):
            drawn(player, before.draw_penalty or 1, " for the penalty" if before.draw_penalty else "")
        else:
            add(player, "Skipped the turn under the ace.")
        if before.provisional_winner is not None and after.hands[before.provisional_winner]:
            add(before.provisional_winner, "Returned to play.")
        if after.winner is not None:
            add(after.winner, "Won the game.")
        elif after.provisional_winner is not None and after.provisional_winner != before.provisional_winner:
            add(after.provisional_winner, "Emptied their hand; victory awaits resolution.")


class Handler(BaseHTTPRequestHandler):
    def setup(self):
        super().setup()
        self.connection.settimeout(5)

    def reply(self, status, data, content_type="application/json"):
        if content_type == "application/json":
            data = json.dumps(data).encode()
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(data)))
        self.send_header("Cache-Control", "no-store")
        self.send_header("X-Content-Type-Options", "nosniff")
        self.send_header("Connection", "close")
        self.end_headers()
        self.wfile.write(data)
        self.close_connection = True

    def local_request(self):
        hosts = self.headers.get_all("Host", [])
        port = self.server.server_port
        allowed = {f"127.0.0.1:{port}", f"localhost:{port}"}
        if port == 80:
            allowed.update(("127.0.0.1", "localhost"))
        if len(hosts) != 1 or hosts[0].lower() not in allowed:
            raise ValueError("Use the local server address")
        origins = self.headers.get_all("Origin", [])
        if origins and (len(origins) != 1 or origins[0] != f"http://{hosts[0]}"):
            raise ValueError("Cross-origin requests are not allowed")
        if self.headers.get("Sec-Fetch-Site") not in (None, "same-origin", "none"):
            raise ValueError("Cross-origin requests are not allowed")

    def do_GET(self):
        try:
            self.local_request()
            request_path = unquote(urlsplit(self.path).path)
            if not request_path.startswith("/"):
                raise ValueError("Request path must start with /")
            if request_path == "/api/state":
                with self.server.game_lock:
                    self.reply(200, self.server.game.view())
                return
            if request_path == "/":
                request_path = "/web/index.html"
            folder = request_path.split("/", 2)[1]
            if folder not in ("web", "design-system", "free-playing-cards"):
                self.reply(404, {"error": "Not found"})
                return
            asset = (ROOT / request_path.lstrip("/")).resolve()
            if (not asset.is_relative_to(ROOT / folder) or asset.suffix.lower() not in ASSET_TYPES
                    or not asset.is_file()):
                self.reply(404, {"error": "Not found"})
                return
            self.reply(200, asset.read_bytes(), mimetypes.guess_type(asset)[0] or "application/octet-stream")
        except (ValueError, OSError) as error:
            self.reply(400, {"error": str(error)})

    def do_POST(self):
        try:
            self.local_request()
            route = urlsplit(self.path).path
            if route not in ("/api/new", "/api/move", "/api/debug/max-hand"):
                self.reply(404, {"error": "Not found"})
                return
            lengths = self.headers.get_all("Content-Length", [])
            if (self.headers.get("Transfer-Encoding") is not None or len(lengths) != 1
                    or not lengths[0].isascii() or not lengths[0].isdigit()
                    or not 0 < int(lengths[0]) <= 4096):
                raise ValueError("Send a JSON body of at most 4096 bytes with Content-Length")
            if self.headers.get_content_type() != "application/json":
                raise ValueError("Content-Type must be application/json")
            body = self.rfile.read(int(lengths[0]))
            if len(body) != int(lengths[0]):
                raise ValueError("Incomplete request body")
            payload = json.loads(body)
            if type(payload) is not dict:
                raise ValueError("JSON body must be an object")
            with self.server.game_lock:
                if route == "/api/new":
                    if set(payload) - {"seed"} or ("seed" in payload and type(payload["seed"]) is not int):
                        raise ValueError("New game accepts only an optional integer seed")
                    result = self.server.game.new(payload.get("seed"))
                elif route == "/api/debug/max-hand":
                    if set(payload) != {"count"}:
                        raise ValueError("Debug hand preset requires only count")
                    result = self.server.game.max_hand(payload["count"])
                else:
                    if set(payload) != {"version", "move_id"}:
                        raise ValueError("Move requires only version and move_id")
                    result = self.server.game.move(payload["version"], payload["move_id"])
            self.reply(200, result)
        except StaleMove as error:
            self.reply(409, {"error": str(error)})
        except (ValueError, OSError) as error:
            self.reply(400, {"error": str(error)})


def create_server(port=8767):
    server = ThreadingHTTPServer(("127.0.0.1", port), Handler)
    # ponytail: one shared debug game; add sessions only for independent simultaneous games.
    server.game = DebugGame()
    server.game_lock = Lock()
    return server


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=8767)
    args = parser.parse_args()
    if not 0 <= args.port <= 65535:
        parser.error("port must be between 0 and 65535")
    with create_server(args.port) as server:
        print(f"Bluff Mau-Mau debug table: http://127.0.0.1:{server.server_port}", flush=True)
        try:
            server.serve_forever()
        except KeyboardInterrupt:
            pass


if __name__ == "__main__":
    main()
