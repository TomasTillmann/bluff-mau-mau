"""Blind contract tests; authored without access to move_explain implementation."""
import copy
import re
import unittest

from bluff_mau_mau import (
    CARDS, Accept, Card, Challenge, Draw, GameState, NewGame, Play, PlayCard, Skip,
)
from move_explain import explain_move


def card(code):
    return Card(code[:-1], code[-1])


def position(hand0, hand1, *, pile=("9H",), turn=0, deck_size=None,
             surplus_player=1, **fields):
    """Conserve all 32 cards while constructing a valid engine starting point."""
    hands = [tuple(map(card, hand0)), tuple(map(card, hand1))]
    discards = tuple(map(card, pile))
    used = hands[0] + hands[1] + discards
    assert len(set(used)) == len(used)
    remaining = tuple(c for c in CARDS if c not in used)
    if deck_size is None:
        deck = remaining
    else:
        deck = remaining[:deck_size]
        hands[surplus_player] += remaining[deck_size:]
    return GameState(deck=deck, pile=discards, hands=tuple(hands),
                     top=discards[-1], turn=turn, **fields)


def snapshot(state, status):
    return {
        "phase": state.phase,
        "turn": state.turn,
        "hand_counts": [len(hand) for hand in state.hands],
        "top": state.top.rank + state.top.suit,
        "chosen_suit": state.chosen_suit,
        "draw_penalty": state.draw_penalty,
        "skip_pending": state.skip_pending,
        "provisional_winner": state.provisional_winner,
        "winner": state.winner,
        "top_status": status,
    }


def public_text(result):
    return (result["title"] + ". " + result["detail"]).lower().replace("’", "'")


def role_text(result):
    text = public_text(result)
    text = re.sub(r"\b(?:your opponent|the opponent|opponent)(?:'s)?\b|\b(?:they|them|theirs?)\b",
                  "OTHER", text)
    return re.sub(r"\b(?:you|yours?)\b", "VIEWER", text)


class MoveExplanationContract(unittest.TestCase):
    def transition(self, state, move, kind, *, perspective=0,
                   before_status=None, after_status=None, drawn=None, winner=None):
        after = Play(state, move)
        before_status = before_status or ("Awaiting response" if state.phase == "response" else "Accepted")
        if after_status is None:
            after_status = ("Awaiting response" if isinstance(move, PlayCard)
                            else "Revealed" if isinstance(move, Challenge)
                            else "Accepted" if isinstance(move, Accept)
                            else before_status)
        before_public = snapshot(state, before_status)
        after_public = snapshot(after, after_status)
        originals = copy.deepcopy((before_public, after_public))
        result = explain_move(before_public, after_public, perspective=perspective)
        self.assertIs(type(result), dict, result)
        self.assertEqual(set(result), {"kind", "actor", "drawn", "winner", "title", "detail"}, result)
        self.assertEqual(result["kind"], kind, result)
        self.assertIs(type(result["actor"]), int, result)
        self.assertEqual(result["actor"], state.turn, result)
        expected_drawn = [max(0, len(after.hands[p]) - len(state.hands[p])) for p in (0, 1)]
        if drawn is not None:
            self.assertEqual(expected_drawn, drawn, "Engine fixture does not describe intended draw")
        self.assertIs(type(result["drawn"]), list, result)
        self.assertTrue(all(type(n) is int for n in result["drawn"]), result)
        self.assertEqual(result["drawn"], expected_drawn, result)
        self.assertEqual(after.winner, winner, "Engine fixture has unintended winner")
        self.assertEqual(result["winner"], after.winner, result)
        self.assertTrue(result["winner"] is None or type(result["winner"]) is int, result)
        self.assertIs(type(result["title"]), str, result)
        self.assertTrue(result["title"].strip(), result)
        self.assertIs(type(result["detail"]), str, result)
        self.assertEqual((before_public, after_public), originals, "Input snapshots were mutated")
        self.assertEqual(explain_move(before_public, after_public, perspective=perspective), result,
                         "Identical public inputs must be deterministic")
        self.assertEqual((before_public, after_public), originals, "Repeated call mutated input snapshots")
        for player, count in enumerate(expected_drawn):
            if count:
                self.assert_draw(result, player, count, perspective)
        if winner is not None:
            self.assert_win(result, winner, perspective)
        return after, result

    def assert_role(self, result, player, perspective):
        role = "VIEWER" if player == perspective else "OTHER"
        self.assertIn(role, role_text(result), result)

    def assert_draw(self, result, player, count, perspective):
        self.assert_role(result, player, perspective)
        words = {1: "one|a|an", 2: "two", 3: "three", 4: "four", 5: "five",
                 6: "six", 7: "seven", 8: "eight", 9: "nine", 10: "ten"}
        amount = str(count) + "|" + words.get(count, str(count))
        self.assertRegex(public_text(result), rf"\b(?:{amount})\b", result)
        if count == 1:
            self.assertNotRegex(public_text(result), r"\b(?:1|one|a|an)\s+cards\b", result)
        self.assertRegex(public_text(result),
                         r"\b(?:draw\w*|drew|tak\w*|took|receiv\w*|pick\w*|gain\w*|get|gets|got)\b", result)

    def assert_win(self, result, winner, perspective):
        role = "VIEWER" if winner == perspective else "OTHER"
        other = "OTHER" if role == "VIEWER" else "VIEWER"
        win = r"\b(?:win|wins|won|winner|victory)\b"
        between = rf"(?:(?!{other}).){{0,60}}"
        self.assertRegex(role_text(result), rf"(?:{role}{between}{win}|{win}{between}{role})", result)

    def assert_provisional(self, result):
        self.assertIsNone(result["winner"], result)
        for sentence in re.split(r"[.!?;]", public_text(result)):
            if re.search(r"\b(?:you|your opponent)\s+(?:(?:have|has)\s+)?(?:win|wins|won)\b", sentence):
                self.assertRegex(sentence,
                                 r"\b(?:if|not|yet|provisional|pending|until|await\w*|can|could|may|would|claim\w*|confirm\w*)\b",
                                 result)

    def assert_card(self, result, code):
        ranks = {"7": "7|seven", "8": "8|eight", "9": "9|nine", "10": "10|ten",
                 "J": "j|jack", "Q": "q|queen", "K": "k|king", "A": "a|ace"}
        suits = {"H": "h|hearts?|♥", "D": "d|diamonds?|♦", "C": "c|clubs?|♣", "S": "s|spades?|♠"}
        self.assertRegex(public_text(result),
                         rf"(?<![a-z0-9])(?:{ranks[code[:-1]]})\s*(?:of\s*)?(?:{suits[code[-1]]})(?![a-z0-9])", result)

    def assert_accept(self, result):
        self.assertRegex(public_text(result), r"\b(?:accept\w*|trust\w*|unchallenged|let\b.*\bstand|no challenge|passed on challenging)\b", result)
        self.assertNotRegex(public_text(result), r"\b(?:challenge failed|caught a bluff|caught your bluff)\b", result)

    def test_reset_and_unchanged_snapshots_have_no_result(self):
        start = NewGame(seed=3)
        response = Play(position(("10H",), ("8D",)), PlayCard(card("10H"), card("10H")))
        finished = Play(response, Challenge())
        for state, status in ((start, "Starting card"), (response, "Awaiting response"), (finished, "Revealed")):
            for perspective in (0, 1):
                with self.subTest(phase=state.phase, perspective=perspective):
                    public = snapshot(state, status)
                    original = copy.deepcopy(public)
                    self.assertIsNone(explain_move(None, public, perspective=perspective))
                    self.assertIsNone(explain_move(public, copy.deepcopy(public), perspective=perspective))
                    self.assertEqual(public, original)

    def test_play_declares_public_identity_for_both_actors_and_perspectives(self):
        for actor in (0, 1):
            for perspective in (0, 1):
                with self.subTest(actor=actor, perspective=perspective):
                    state = position(("JC", "8C"), ("JD", "8D"), turn=actor)
                    actual = state.hands[actor][0]
                    _, result = self.transition(state, PlayCard(actual, card("10H")), "play",
                                                perspective=perspective, drawn=[0, 0])
                    self.assert_card(result, "10H")
                    self.assertRegex(public_text(result), r"\b(?:play\w*|declar\w*|announc\w*|claim\w*|put|laid)\b", result)

    def test_queen_declared_identity_is_distinct_from_continuing_suit(self):
        state = position(("8H",), ("JC", "8D"), turn=1)
        for perspective in (0, 1):
            with self.subTest(perspective=perspective):
                _, result = self.transition(state, PlayCard(card("JC"), card("QD"), "S"), "play",
                                            perspective=perspective, drawn=[0, 0])
                self.assert_card(result, "QD")

    def test_publicly_identical_hidden_plays_have_identical_explanations(self):
        state_a = position(("JC", "8C"), ("8D",))
        state_b = position(("JD", "8C"), ("8D",))
        _, a = self.transition(state_a, PlayCard(card("JC"), card("10H")), "play")
        _, b = self.transition(state_b, PlayCard(card("JD"), card("10H")), "play")
        self.assertEqual(a, b, "Hidden actual identities must not affect the public explanation")

    def test_last_play_and_ordinary_acceptance_remain_provisional(self):
        state = position(("JC",), ("8D", "10H"))
        response, played = self.transition(state, PlayCard(card("JC"), card("10H")), "play", drawn=[0, 0])
        self.assertEqual(response.provisional_winner, 0)
        self.assert_provisional(played)
        _, accepted = self.transition(response, Accept(), "accept", drawn=[0, 0])
        self.assert_accept(accepted)
        self.assert_provisional(accepted)

    def test_bluff_caught_draws_to_bluffer_with_stable_perspective(self):
        for actor in (0, 1):
            state = position(("JC", "8C"), ("JD", "8D"), turn=1 - actor)
            response = Play(state, PlayCard(state.hands[state.turn][0], card("10H")))
            expected = [0, 0]
            expected[1 - actor] = 2
            for perspective in (0, 1):
                with self.subTest(challenger=actor, perspective=perspective):
                    _, result = self.transition(response, Challenge(), "bluff_caught",
                                                perspective=perspective, drawn=expected)
                    self.assertRegex(public_text(result), r"\b(?:bluff\w*|false|lie|lied|lying|dishonest|caught)\b", result)

    def test_failed_challenge_draws_to_challenger_and_clears_queen_suit(self):
        state = position(("QH", "8C"), ("JD", "8D"))
        response = Play(state, PlayCard(card("QH"), card("QH"), "S"))
        for perspective in (0, 1):
            with self.subTest(perspective=perspective):
                after, result = self.transition(response, Challenge(), "challenge_failed",
                                                perspective=perspective, drawn=[0, 2])
                self.assertIsNone(after.chosen_suit)
                self.assertRegex(public_text(result),
                                 r"\b(?:wrong|fail\w*|truth\w*|true|honest|correct|match\w*|real|lost)\b", result)

    def stacked_response(self, truthful):
        state = position(("7H", "7S", "8H"), ("KS", "8D"), pile=("QS",))
        state = Play(state, PlayCard(card("7H"), card("7S")))
        state = Play(state, Accept())
        state = Play(state, PlayCard(card("KS"), card("KS")))
        state = Play(state, Accept())
        actual = "7S" if truthful else "8H"
        return Play(state, PlayCard(card(actual), card("7S")))

    def test_stacked_challenges_include_accumulated_penalty(self):
        for truthful in (False, True):
            with self.subTest(truthful=truthful):
                response = self.stacked_response(truthful)
                self.assertEqual(response.draw_penalty, 8)
                expected = [0, 10] if truthful else [10, 0]
                self.transition(response, Challenge(), "challenge_failed" if truthful else "bluff_caught",
                                drawn=expected)

    def test_challenge_shortage_is_actual_one_card_even_after_recycling(self):
        for truthful in (False, True):
            with self.subTest(truthful=truthful):
                state = position(("7H", "8H"), ("8D",), deck_size=0)
                actual = "7H" if truthful else "8H"
                response = Play(state, PlayCard(card(actual), card("7H")))
                self.assertEqual(len(response.deck), 0)
                self.assertEqual(len(response.pile), 2)
                self.transition(response, Challenge(), "challenge_failed" if truthful else "bluff_caught",
                                drawn=[0, 1] if truthful else [1, 0])

    def test_normal_draw_reports_one_for_both_players_and_perspectives(self):
        for actor in (0, 1):
            for perspective in (0, 1):
                with self.subTest(actor=actor, perspective=perspective):
                    expected = [0, 0]
                    expected[actor] = 1
                    self.transition(position(("8C",), ("8D",), turn=actor), Draw(), "draw",
                                    perspective=perspective, drawn=expected)

    def test_recycling_can_grow_deck_while_actual_draw_is_one(self):
        state = position(("8C",), ("8D",), pile=("9H", "10H", "JH", "QH", "KH"), deck_size=0)
        after, _ = self.transition(state, Draw(), "draw", drawn=[1, 0])
        self.assertGreater(len(after.deck), len(state.deck))

    def test_stacked_penalty_draw_and_shortage_use_actual_growth(self):
        state = Play(self.stacked_response(False), Accept())
        self.transition(state, Draw(), "draw", drawn=[0, 8])
        scarce = position(("8C",), ("8D",), pile=("9H", "10H", "JH", "7S"),
                          deck_size=1, draw_penalty=8)
        after, _ = self.transition(scarce, Draw(), "draw", drawn=[4, 0])
        self.assertEqual(after.draw_penalty, 0)

    def test_skip_with_unchanged_hands_is_not_a_draw(self):
        for actor in (0, 1):
            for perspective in (0, 1):
                with self.subTest(actor=actor, perspective=perspective):
                    state = position(("8C",), ("8D",), pile=("AH",), skip_pending=True, turn=actor)
                    _, result = self.transition(state, Skip(), "skip", drawn=[0, 0], perspective=perspective)
                    self.assertRegex(public_text(result), r"\b(?:skip\w*|miss\w*|sit\w*|lose\w*|lost)\b", result)

    def test_accept_return_forces_actual_draw_and_cancels_empty_hand_claim(self):
        for returned in (0, 1):
            for perspective in (0, 1):
                with self.subTest(returned=returned, perspective=perspective):
                    hands = [(), ()]
                    hands[1 - returned] = ("7H", "8D")
                    state = position(*hands, turn=1 - returned, provisional_winner=returned)
                    response = Play(state, PlayCard(card("7H"), card("7H")))
                    expected = [0, 0]
                    expected[returned] = 2
                    after, result = self.transition(response, Accept(), "accept", drawn=expected,
                                                    perspective=perspective)
                    self.assert_accept(result)
                    self.assertIsNone(after.provisional_winner)

    def test_accept_return_shortage_draws_one_instead_of_eight(self):
        state = position((), ("8H", "8D"), pile=("7S",), turn=1, deck_size=0,
                         draw_penalty=6, provisional_winner=0)
        response = Play(state, PlayCard(card("8H"), card("7S")))
        self.assertEqual(response.draw_penalty, 8)
        _, result = self.transition(response, Accept(), "accept", drawn=[1, 0])
        self.assert_accept(result)

    def test_accept_ace_skips_empty_hand_without_declaring_victory(self):
        state = position((), ("AH", "8D"), turn=1, provisional_winner=0)
        response = Play(state, PlayCard(card("AH"), card("AH")))
        after, result = self.transition(response, Accept(), "accept", drawn=[0, 0])
        self.assertEqual(after.turn, 1)
        self.assertFalse(after.skip_pending)
        self.assert_accept(result)
        self.assertRegex(public_text(result), r"\b(?:skip\w*|miss\w*|sit\w*|lose\w*|lost)\b", result)
        self.assert_provisional(result)

    def test_accept_final_ordinary_or_ace_confirms_earlier_player_zero(self):
        for final_card in ("10H", "AH"):
            for perspective in (0, 1):
                with self.subTest(card=final_card, perspective=perspective):
                    state = position((), (final_card,), turn=1, provisional_winner=0)
                    response = Play(state, PlayCard(card(final_card), card(final_card)))
                    _, result = self.transition(response, Accept(), "accept", drawn=[0, 0], winner=0,
                                                perspective=perspective)
                    self.assert_accept(result)

    def test_accept_final_penalty_returns_actor_and_confirms_other_winner(self):
        for returned in (0, 1):
            with self.subTest(returned=returned):
                hands = [(), ()]
                hands[1 - returned] = ("7H",)
                state = position(*hands, turn=1 - returned, provisional_winner=returned)
                response = Play(state, PlayCard(card("7H"), card("7H")))
                expected = [0, 0]
                expected[returned] = 2
                _, result = self.transition(response, Accept(), "accept", drawn=expected, winner=1 - returned)
                self.assert_accept(result)

    def test_challenge_can_confirm_player_zero_or_return_earlier_empty_hand(self):
        state = position(("10H",), ("8D",))
        response = Play(state, PlayCard(card("10H"), card("10H")))
        self.transition(response, Challenge(), "challenge_failed", drawn=[0, 2], winner=0)
        state = position((), ("AH",), turn=1, provisional_winner=0)
        response = Play(state, PlayCard(card("AH"), card("AH")))
        self.transition(response, Challenge(), "challenge_failed", drawn=[2, 0], winner=1)
        state = position((), ("JC", "8D"), turn=1, provisional_winner=0)
        response = Play(state, PlayCard(card("JC"), card("7H")))
        self.transition(response, Challenge(), "bluff_caught", drawn=[0, 4], winner=0)

    def test_draw_and_skip_confirm_opponent_zero_without_changing_actor(self):
        state = position((), ("8D",), turn=1, provisional_winner=0)
        self.transition(state, Draw(), "draw", drawn=[0, 1], winner=0)
        state = position((), ("8D",), pile=("AH",), turn=1, provisional_winner=0, skip_pending=True)
        self.transition(state, Skip(), "skip", drawn=[0, 0], winner=0)

    def test_default_perspective_is_player_zero(self):
        state = position(("8C",), ("8D",), turn=1)
        after = Play(state, Draw())
        before_public, after_public = snapshot(state, "Accepted"), snapshot(after, "Accepted")
        default = explain_move(before_public, after_public)
        explicit = explain_move(before_public, after_public, perspective=0)
        self.assertEqual(default, explicit)
        self.assert_role(default, 1, 0)


if __name__ == "__main__":
    unittest.main()
