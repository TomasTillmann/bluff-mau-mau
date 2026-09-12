"""Bridge checks: python -B -m unittest test_server -v (no browser required)."""

from dataclasses import replace
from http.client import HTTPConnection
import json
from threading import Thread
import unittest
from unittest.mock import patch

from bluff_mau_mau import CARDS, Accept, Challenge, Draw, MoveGenerator, NewGame, Play, PlayCard, Skip
from server import DebugGame, Handler, StaleMove, create_server
from test_bluff_mau_mau import card, finish_first, play, state


class GameBridgeTests(unittest.TestCase):
    def apply(self, game, move):
        return game.move(game.version, MoveGenerator(game.state).index(move))

    def test_projection_and_every_legal_move_match_engine(self):
        pending = play(state(), "JC", "7H")
        finished = Play(play(state(top="9S", hands=(("9H",), ("8C",))), "9H"), Challenge())
        empty_deck = state()
        empty_deck = replace(empty_deck, deck=(), hands=(empty_deck.hands[0], empty_deck.hands[1] + empty_deck.deck))
        cases = [NewGame(42), state(top="QH"), state(top="QD", chosen_suit="S"),
                 state(top="AH", skip_pending=True), state(top="7H", draw_penalty=2),
                 state(top="7S", draw_penalty=6), state(top="KS", draw_penalty=4),
                 empty_deck, pending, finish_first(), finished]
        for position in cases:
            game = DebugGame()
            game.state = position
            view = game.view()
            with self.subTest(phase=position.phase, top=position.top):
                self.assertEqual(view["hands"], [[c.rank + c.suit for c in hand] for hand in position.hands])
                self.assertEqual(view["top"], position.top.rank + position.top.suit)
                for field in ("phase", "turn", "chosen_suit", "draw_penalty", "skip_pending",
                              "provisional_winner", "winner"):
                    self.assertEqual(view[field], getattr(position, field))
                self.assertEqual((view["deck_count"], view["pile_count"]), (len(position.deck), len(position.pile)))
                self.assertFalse({"deck", "pile", "rng_state"} & view.keys())
                moves = MoveGenerator(position)
                self.assertEqual([m["id"] for m in view["legal_moves"]], list(range(len(moves))))
                for projected, move in zip(view["legal_moves"], moves):
                    if isinstance(move, PlayCard):
                        self.assertEqual(projected["type"], "play")
                        self.assertEqual(PlayCard(card(projected["actual"]), card(projected["declared"]),
                                                  projected["chosen_suit"]), move)
                    else:
                        self.assertEqual(projected["type"], type(move).__name__.lower())
                    game.state = position
                    result = game.move(game.version, projected["id"])
                    self.assertEqual(game.state, Play(position, move))
                    self.assertEqual(result["version"], game.version)

    def test_after_accept_actions_use_engine_legality_without_advancing_game(self):
        recyclable = play(state(), "JC", "9H")
        recyclable = replace(recyclable, deck=(),
                             hands=(recyclable.hands[0], recyclable.hands[1] + recyclable.deck))
        cases = [
            (state(), []),
            (play(state(), "JC", "9H"), ["draw"]),
            (play(state(), "JC", "AH"), ["skip"]),
            (play(state(), "JC", "7H"), ["draw"]),
            (play(state(top="7S", draw_penalty=2), "JC", "KS"), ["draw"]),
            (recyclable, ["draw"]),
            (play(finish_first(), "7H", "7H"), []),
            (play(finish_first(), "7H", "AH"), []),
            (play(finish_first(), "7H", "9H"), []),
            (play(finish_first(("7H",)), "7H", "7H"), []),
            (Play(play(finish_first(), "7H", "9H"), Accept()), []),
        ]
        game = DebugGame()
        for position, expected in cases:
            with self.subTest(phase=position.phase, top=position.top,
                              empty_responder=not position.hands[position.turn]):
                game.state = position
                version, history, top_status = game.version, list(game.history), game.top_status
                view = game.view()
                self.assertEqual(view["after_accept_actions"], expected)
                self.assertEqual(game.state, position)
                self.assertEqual((game.version, game.history, game.top_status),
                                 (version, history, top_status))
                if expected:
                    accepted = Play(position, Accept())
                    self.assertEqual((accepted.phase, accepted.turn, accepted.winner),
                                     ("turn", position.turn, None))
                    self.assertEqual(expected, [type(move).__name__.lower()
                                              for move in MoveGenerator(accepted)
                                              if isinstance(move, (Draw, Skip))])

    def test_public_history_and_top_status_only_reveal_challenged_card(self):
        game = DebugGame()
        game.state = state()
        game.history = []
        pending = self.apply(game, PlayCard(card("JC"), card("7H")))
        self.assertEqual((pending["top"], pending["top_status"]), ("7H", "Awaiting response"))
        self.assertEqual(pending["history"][-1]["text"], "Declared 7 of hearts face down.")
        accepted = self.apply(game, Accept())
        self.assertEqual((accepted["top"], accepted["top_status"]), ("7H", "Accepted"))
        self.assertNotIn("jack of clubs", json.dumps(accepted["history"]))
        self.apply(game, PlayCard(card("10D"), card("7S")))
        revealed = self.apply(game, Challenge())
        self.assertEqual((revealed["top"], revealed["top_status"]), ("10D", "Revealed"))
        self.assertIn("bluff caught", revealed["history"][-2]["text"])
        self.assertEqual(revealed["history"][-1], {"player": 1, "text": "Drew 6 cards for the challenge."})
        self.assertNotIn("jack of clubs", json.dumps(revealed["history"]))
        self.assertEqual(revealed["draw_penalty"], 0)
        self.assertFalse(revealed["skip_pending"])
        game.state = state(top="9H", hands=(("QH", "8C"), ("10D", "AD")))
        announced = self.apply(game, PlayCard(card("QH"), card("QH"), "S"))
        self.assertIn("continuing suit spades", announced["history"][-1]["text"])
        revealed = self.apply(game, Challenge())
        self.assertIsNone(revealed["chosen_suit"])
        self.assertIn("truthful declaration", revealed["history"][-2]["text"])
        self.assertEqual(revealed["history"][-1]["player"], 1)

    def test_automatic_return_skip_winner_and_short_draw_are_logged(self):
        for move, expected in ((PlayCard(card("7H"), card("7H")), "Drew 2 cards for the return penalty."),
                               (PlayCard(card("7H"), card("AH")), "Empty hand skipped under the ace.")):
            game = DebugGame()
            game.state = finish_first()
            self.apply(game, move)
            result = self.apply(game, Accept())
            self.assertIn({"player": 0, "text": expected}, result["history"])
        game.state = finish_first(("7H",))
        self.apply(game, PlayCard(card("7H"), card("7H")))
        result = self.apply(game, Accept())
        self.assertEqual((result["winner"], len(result["hands"][0])), (1, 2))
        self.assertIn({"player": 0, "text": "Drew 2 cards for the return penalty."}, result["history"])
        game.state = state(top="AH", skip_pending=True)
        self.assertIn("Skipped", self.apply(game, Skip())["history"][-1]["text"])
        game.state = finish_first(("8C",))
        result = self.apply(game, Draw())
        self.assertEqual((result["phase"], result["winner"], result["legal_moves"]), ("finished", 0, []))
        self.assertEqual(result["history"][-1], {"player": 0, "text": "Won the game."})
        game.state = play(state(), "JC", "7H")
        game.state = replace(game.state, deck=(),
                             hands=(game.state.hands[0], game.state.hands[1] + game.state.deck))
        result = self.apply(game, Challenge())
        self.assertIn("Drew 1 card", result["history"][-1]["text"])
        self.assertIn("3 unpaid cards cancelled", result["history"][-1]["text"])

    def test_versions_seed_and_invalid_ids_do_not_mutate_game(self):
        game = DebugGame()
        self.assertEqual(game.view()["turn"], 0)
        view = game.new(42)
        self.assertEqual(view["turn"], 0)
        self.assertEqual(game.state, NewGame(42, dealer=1))
        for version, move_id, error in ((view["version"] - 1, 0, StaleMove),
                                        (view["version"], -1, ValueError),
                                        (view["version"], len(view["legal_moves"]), ValueError),
                                        (True, 0, ValueError), (view["version"], False, ValueError)):
            with self.assertRaises(error):
                game.move(version, move_id)
            self.assertEqual(game.view(), view)
        game.move(view["version"], 0)
        with self.assertRaises(StaleMove):
            game.move(view["version"], 0)
        restarted = game.new(42)
        self.assertGreater(restarted["version"], view["version"])
        self.assertEqual(restarted["turn"], 0)
        self.assertEqual(game.state, NewGame(42, dealer=1))

    def test_maximum_hand_presets_obey_card_conservation_and_engine_moves(self):
        game = DebugGame()
        playable = game.max_hand(30)
        self.assertEqual(tuple(map(len, playable["hands"])), (30, 1))
        self.assertEqual((playable["phase"], playable["turn"], playable["winner"]), ("turn", 0, None))
        self.assertEqual({move["actual"] for move in playable["legal_moves"]}, set(playable["hands"][0]))
        before = replace(game.state, deck=game.state.hands[1], hands=(game.state.hands[0], ()),
                         provisional_winner=1)
        maximum = game.max_hand(31)
        self.assertEqual(game.state, Play(before, Draw()))
        self.assertEqual(tuple(map(len, maximum["hands"])), (31, 0))
        self.assertEqual((maximum["pile_count"], maximum["deck_count"], maximum["winner"]), (1, 0, 1))
        self.assertEqual(maximum["legal_moves"], [])
        # One of the 32 unique cards must always remain in the pile.
        with self.assertRaises(ValueError):
            MoveGenerator(replace(game.state, hands=(CARDS, ()), pile=()))
        for invalid in (True, 29, 32, "30", None):
            with self.assertRaises(ValueError):
                game.max_hand(invalid)
            self.assertEqual(game.view(), maximum)


class HttpBoundaryTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.logging = patch.object(Handler, "log_message")
        cls.logging.start()
        cls.server = create_server(0)
        cls.worker = Thread(target=cls.server.serve_forever, daemon=True)
        cls.worker.start()

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()
        cls.worker.join()
        cls.logging.stop()

    def request(self, method, path, body=None, headers=None):
        connection = HTTPConnection("127.0.0.1", self.server.server_port, timeout=5)
        try:
            connection.request(method, path, body, headers or {})
            response = connection.getresponse()
            data = response.read()
            return response.status, json.loads(data) if response.getheader("Content-Type") == "application/json" else data
        finally:
            connection.close()

    def test_http_moves_stale_reset_and_static_boundary(self):
        headers = {"Content-Type": "application/json"}
        status, initial = self.request("POST", "/api/new", '{"seed":42}', headers)
        self.assertEqual((status, initial["turn"]), (200, 0))
        payload = json.dumps({"version": initial["version"], "move_id": 0})
        self.assertEqual(self.request("POST", "/api/move", payload, headers)[0], 200)
        self.assertEqual(self.request("POST", "/api/move", payload, headers)[0], 409)
        status, current = self.request("GET", "/api/state")
        self.assertEqual((status, current["version"]), (200, initial["version"] + 1))
        status, reset = self.request("POST", "/api/new", "{}", headers)
        self.assertEqual((status, reset["turn"]), (200, 0))
        status, maximum = self.request("POST", "/api/debug/max-hand", '{"count":31}', headers)
        self.assertEqual((status, len(maximum["hands"][0]), maximum["winner"]), (200, 31, 1))
        self.assertEqual(self.request("GET", "/design-system/components.css")[0], 200)
        self.assertEqual(self.request("GET", "/free-playing-cards/svg%20cards/card%20fronts/hearts/7%20of%20hearts.svg")[0], 200)
        for path in ("/.git/config", "/server.py", "/RULES.md", "/design-system/../server.py",
                     "/web/%2e%2e/.git/config", "/design-system/", "/design-system/README.md"):
            self.assertEqual(self.request("GET", path)[0], 404, path)

    def test_bad_origin_host_json_and_fields_leave_state_unchanged(self):
        before = self.request("GET", "/api/state")[1]
        bad_requests = [
            ("/api/new", "{}", {"Origin": "https://example.com"}),
            ("/api/new", "{}", {"Host": "example.com"}),
            ("/api/new", "{}", {"Sec-Fetch-Site": "cross-site"}),
            ("/api/new", "{}", {"Content-Type": "text/plain"}),
            ("/api/new", "x" * 4097, {}),
            ("/api/new", "{", {}), ("/api/new", "[]", {}),
            ("/api/new", '{"seed":true}', {}), ("/api/new", '{"seed":null}', {}),
            ("/api/new", '{"seed":1,"dealer":0}', {}),
            ("/api/debug/max-hand", '{"count":true}', {}),
            ("/api/debug/max-hand", '{"count":32}', {}),
            ("/api/debug/max-hand", '{"count":30,"state":{}}', {}),
            ("/api/move", '{"version":true,"move_id":0}', {}),
            ("/api/move", '{"move_id":0}', {}),
            ("/api/move", json.dumps({"version": before["version"], "move_id": -1}), {}),
        ]
        for path, body, extra in bad_requests:
            with self.subTest(path=path, body=body[:50], headers=extra):
                status, data = self.request("POST", path, body, {"Content-Type": "application/json", **extra})
                self.assertEqual(status, 400)
                self.assertEqual(set(data), {"error"})
                self.assertEqual(self.request("GET", "/api/state")[1], before)
        self.assertEqual(self.request("GET", "relative-path")[0], 400)
        self.assertEqual(self.request("GET", "/api/state", headers={"Host": "evil.example"})[0], 400)
        host = f"127.0.0.1:{self.server.server_port}"
        self.assertEqual(self.request("POST", "/api/new", "{}",
                         {"Content-Type": "application/json", "Origin": f"http://{host}"})[0], 200)


if __name__ == "__main__":
    unittest.main()
