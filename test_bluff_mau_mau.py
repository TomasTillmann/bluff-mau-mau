"""Executable examples from RULES.md; run with python -m unittest -v."""

from copy import deepcopy
from dataclasses import FrozenInstanceError, replace
from itertools import product
import random
import unittest

from bluff_mau_mau import (
    Accept, CARDS, Card, Challenge, Draw, GameState, MoveGenerator, NewGame,
    Play, PlayCard, Skip,
)


def card(code):
    return Card(code[:-1], code[-1])


def state(top="9H", hands=(("JC", "8C"), ("10D", "AD")), pile=None,
          deck_first=(), **fields):
    hands = tuple(tuple(map(card, hand)) for hand in hands)
    pile = tuple(map(card, pile if pile is not None else (top,)))
    used = set(pile + hands[0] + hands[1])
    first = tuple(map(card, deck_first))
    deck = first + tuple(c for c in CARDS if c not in used and c not in first)
    return GameState(deck=deck, pile=pile, hands=hands, turn=0,
                     top=card(top), **fields)


def play(s, actual, declared=None, suit=None):
    return Play(s, PlayCard(card(actual), card(declared or actual), suit))


def declarations(s):
    return {m.declared_card for m in MoveGenerator(s) if isinstance(m, PlayCard)}


def finish_first(opponent=("7H", "8C")):
    return Play(play(state(top="9S", hands=(("9H",), opponent)), "9H"), Accept())


def mirror(s):
    return replace(s, hands=s.hands[::-1], turn=1 - s.turn,
                   provisional_winner=None if s.provisional_winner is None else 1 - s.provisional_winner,
                   winner=None if s.winner is None else 1 - s.winner)


def identity_state(actual, top, physical_top=None, **fields):
    physical_top = physical_top or top
    assert actual != physical_top
    rest = tuple(c for c in CARDS if c not in (actual, physical_top))
    return GameState(deck=rest[3:], pile=(physical_top,),
                     hands=((actual, rest[0]), rest[1:3]), turn=0, top=top, **fields)


class RulesTests(unittest.TestCase):
    def assert_conserved(self, s):
        cards = s.deck + s.pile + s.hands[0] + s.hands[1]
        self.assertEqual(len(cards), 32)
        self.assertEqual(set(cards), set(CARDS))

    def test_setup_is_seeded_and_initial_special_cards_apply(self):
        seen = set()
        for seed in range(200):
            for dealer in (0, 1):
                s = NewGame(seed=seed, dealer=dealer)
                self.assertEqual(s, NewGame(seed=seed, dealer=dealer))
                self.assertEqual(tuple(map(len, s.hands)), (5, 5))
                self.assertEqual(len(s.deck), 21)
                self.assertEqual(s.turn, 1 - dealer)
                self.assertEqual(s.top, s.pile[-1])
                self.assertEqual(s.phase, "turn")
                self.assertEqual(s.skip_pending, s.top.rank == "A")
                self.assertEqual(s.draw_penalty, 2 if s.top.rank == "7" else
                                 4 if s.top == card("KS") else 0)
                self.assertIsNone(s.chosen_suit)
                self.assert_conserved(s)
                seen.add(s.top.rank)
        self.assertEqual(seen, {"7", "8", "9", "10", "J", "Q", "K", "A"})

    def test_every_actual_card_can_hide_each_legal_declaration(self):
        s = state()
        expected = {c for c in CARDS if c.suit == "H" or c.rank in ("9", "Q")}
        self.assertEqual(declarations(s), expected)
        for actual in s.hands[s.turn]:
            self.assertEqual({m.declared_card for m in MoveGenerator(s)
                              if isinstance(m, PlayCard) and m.actual_card == actual}, expected)
        self.assertIn(Draw(), MoveGenerator(s))
        pending = play(s, "JC", "7H")
        self.assertEqual(set(MoveGenerator(pending)), {Accept(), Challenge()})
        self.assertEqual(pending.turn, 1)
        accepted = Play(pending, Accept())
        self.assertEqual(accepted.top, card("7H"))
        self.assertEqual(accepted.pile[-1], card("JC"))
        self.assertEqual(accepted.draw_penalty, 2)

    def test_invalid_actions_are_rejected(self):
        s = state()
        original = deepcopy(s)
        invalid = [PlayCard(card("JC"), card("KS")),
                   PlayCard(card("KS"), card("7H")),
                   PlayCard(card("JC"), card("QH")),
                   PlayCard(card("JC"), card("QH"), "X"),
                   PlayCard(card("JC"), card("9H"), "H"),
                   Accept(), Challenge(), Skip()]
        for move in invalid:
            with self.subTest(move=move), self.assertRaises(ValueError):
                Play(s, move)
            self.assertEqual(s, original)
        pending = play(s, "JC", "7H")
        with self.assertRaises(ValueError):
            Play(pending, Draw())

    def test_queen_suit_is_chosen_before_response(self):
        s = state()
        queens = [m for m in MoveGenerator(s) if isinstance(m, PlayCard)
                  and m.actual_card == card("JC") and m.declared_card == card("QS")]
        self.assertEqual({m.chosen_suit for m in queens}, {"H", "D", "C", "S"})
        accepted = Play(play(s, "JC", "QS", "D"), Accept())
        self.assertEqual(accepted.chosen_suit, "D")
        self.assertEqual(declarations(accepted), {c for c in CARDS if c.suit == "D" or c.rank == "Q"})

    def test_unrestricted_queen_survives_draws_by_both_players(self):
        states = [state(top="QH")]
        for actual, declared, chosen in (("QC", "9H", None), ("QC", "QC", "S")):
            s = state(hands=((actual, "8C"), ("10D", "AD")))
            states.append(Play(play(s, actual, declared, chosen), Challenge()))
        for s in states:
            for _ in range(3):
                self.assertEqual(declarations(s), set(CARDS))
                self.assertIsNone(s.chosen_suit)
                s = Play(s, Draw())
            actual = s.hands[s.turn][0]
            s = Play(Play(s, PlayCard(actual, card("9C"))), Accept())
            self.assertEqual(declarations(s), {c for c in CARDS if c.suit == "C" or c.rank in ("9", "Q")})

    def test_ace_can_only_be_skipped_or_countered(self):
        s = state(top="AH", skip_pending=True)
        self.assertEqual(declarations(s), {c for c in CARDS if c.rank == "A"})
        self.assertIn(Skip(), MoveGenerator(s))
        self.assertNotIn(Draw(), MoveGenerator(s))
        skipped = Play(s, Skip())
        self.assertFalse(skipped.skip_pending)
        self.assertEqual(skipped.turn, 1)
        self.assertEqual(skipped.top, card("AH"))
        countered = Play(play(s, "JC", "AS"), Accept())
        self.assertTrue(countered.skip_pending)
        self.assertFalse(Play(countered, Skip()).skip_pending)

    def test_penalty_counters_and_payment(self):
        for top, penalty, counters in (("7H", 2, {c for c in CARDS if c.rank == "7"}),
                                       ("7S", 6, {c for c in CARDS if c.rank == "7"} | {card("KS")}),
                                       ("KS", 8, {card("7S")})):
            s = state(top=top, draw_penalty=penalty)
            self.assertEqual(declarations(s), counters)
            self.assertIn(Draw(), MoveGenerator(s))
            self.assertNotIn(Skip(), MoveGenerator(s))
            paid = Play(s, Draw())
            self.assertEqual(len(paid.hands[0]), len(s.hands[0]) + penalty)
            self.assertEqual(paid.draw_penalty, 0)
            self.assertEqual(paid.top, s.top)
            self.assertEqual(paid.turn, 1)
        s = Play(play(state(top="7S", draw_penalty=2), "JC", "KS"), Accept())
        self.assertEqual(s.draw_penalty, 6)
        self.assertEqual(declarations(s), {card("7S")})
        s = Play(play(s, "10D", "7S"), Accept())
        self.assertEqual(s.draw_penalty, 8)

    def test_challenge_uses_declared_contribution_and_reveals_no_new_effect(self):
        for actual, declared, amount, truthful in (("AH", "7S", 4, False),
                                                    ("KS", "10S", 2, False),
                                                    ("8H", "KS", 6, False),
                                                    ("7S", "7S", 4, True),
                                                    ("QS", "QS", 2, True),
                                                    ("AS", "AS", 2, True)):
            with self.subTest(actual=actual, declared=declared):
                s = state(top="9S", hands=((actual, "8C"), ("10D", "AD")))
                resolved = Play(play(s, actual, declared, "H" if declared[0] == "Q" else None), Challenge())
                loser = 1 if truthful else 0
                self.assertEqual(len(resolved.hands[loser]), len(s.hands[loser]) - (loser == 0) + amount)
                self.assertEqual(resolved.turn, 1 - loser)
                self.assertEqual(resolved.top, card(actual))
                self.assertEqual(resolved.draw_penalty, 0)
                self.assertFalse(resolved.skip_pending)
                self.assertIsNone(resolved.chosen_suit)
                self.assert_conserved(resolved)

    def test_challenge_checks_only_latest_of_stacked_bluffs(self):
        for actual, loser in (("10D", 1), ("7D", 0)):
            s = state(hands=(("JC", "8C"), (actual, "AD")))
            s = Play(play(s, "JC", "7H"), Accept())
            resolved = Play(play(s, actual, "7D"), Challenge())
            self.assertEqual(len(resolved.hands[loser]), 7)
            self.assertEqual(resolved.turn, 1 - loser)
            self.assertEqual(resolved.top, card(actual))
            self.assertEqual(resolved.draw_penalty, 0)

    def test_final_play_is_provisional_until_response(self):
        for declared, amount in (("9H", 2), ("7S", 4)):
            for truthful in (True, False):
                actual = declared if truthful else "JC"
                s = state(top="9S", hands=((actual,), ("7H", "8C")))
                pending = play(s, actual, declared)
                self.assertEqual(pending.phase, "response")
                self.assertIsNone(pending.winner)
                accepted = Play(pending, Accept())
                self.assertEqual(accepted.provisional_winner, 0)
                self.assertEqual(accepted.phase, "turn")
                challenged = Play(pending, Challenge())
                self.assertEqual(challenged.winner, 0 if truthful else None)
                loser = 1 if truthful else 0
                self.assertEqual(len(challenged.hands[loser]), amount + (2 if truthful else 0))
                if not truthful:
                    self.assertIsNone(challenged.provisional_winner)
                    self.assertEqual(challenged.turn, 1)

    def test_final_penalty_can_be_paid_or_countered_before_victory(self):
        for declared, counter, contribution, total in (("7H", "7D", 2, 4),
                                                       ("7S", "KS", 2, 6),
                                                       ("KS", "7S", 4, 6)):
            for truthful in (True, False):
                actual = counter if truthful else "8C"
                s = state(top="9" + declared[-1], hands=(("JC",), (actual,)))
                s = Play(play(s, "JC", declared), Accept())
                self.assertEqual(s.provisional_winner, 0)
                self.assertIsNone(s.winner)
                self.assertEqual(s.draw_penalty, contribution)
                self.assertIn(card(counter), declarations(s))
                paid = Play(s, Draw())
                self.assertEqual(len(paid.hands[1]), 1 + contribution)
                self.assertEqual(paid.draw_penalty, 0)
                self.assertEqual(paid.winner, 0)
                pending = play(s, actual, counter)
                accepted = Play(pending, Accept())
                self.assertEqual(len(accepted.hands[0]), total)
                self.assertEqual(accepted.draw_penalty, 0)
                self.assertEqual(accepted.winner, 1)
                challenged = Play(pending, Challenge())
                loser = 0 if truthful else 1
                self.assertEqual(len(challenged.hands[loser]), total + 2)
                self.assertEqual(challenged.draw_penalty, 0)
                self.assertEqual(challenged.winner, 1 - loser)

    def test_return_penalty_can_be_accepted_or_challenged(self):
        for actual, truthful in (("7H", True), ("JC", False)):
            pending = play(finish_first((actual, "8C")), actual, "7H")
            accepted = Play(pending, Accept())
            self.assertEqual(len(accepted.hands[0]), 2)
            self.assertIsNone(accepted.provisional_winner)
            self.assertEqual(accepted.turn, 1)
            challenged = Play(pending, Challenge())
            self.assertEqual(challenged.winner, None if truthful else 0)
            if truthful:
                self.assertEqual(len(challenged.hands[0]), 4)
                self.assertEqual(challenged.turn, 1)

    def test_two_final_cards_resolve_response_before_victory(self):
        for declared in ("7H", "AH", "10H"):
            for truthful in (True, False):
                actual = declared if truthful else "JC"
                pending = play(finish_first((actual,)), actual, declared)
                accepted = Play(pending, Accept())
                self.assertEqual(accepted.winner, 1 if declared == "7H" else 0)
                challenged = Play(pending, Challenge())
                self.assertEqual(challenged.winner, 1 if truthful else 0)
                self.assertEqual(challenged.phase, "finished")
                self.assertEqual(list(MoveGenerator(challenged)), [])

    def test_aces_extend_return_attempt_and_each_gets_a_response(self):
        last_ace = state(hands=(("AH",), ("AS", "KS", "8C")))
        last_ace = Play(play(last_ace, "AH"), Accept())
        for s, aces in ((finish_first(("AH", "AS", "KS", "8C")), ("AH", "AS")),
                        (last_ace, ("AS",))):
            for ace in aces:
                pending = play(s, ace)
                self.assertEqual(set(MoveGenerator(pending)), {Accept(), Challenge()})
                s = Play(pending, Accept())
                self.assertEqual(s.turn, 1)
                self.assertFalse(s.skip_pending)
                self.assertEqual(s.provisional_winner, 0)
            s = Play(play(s, "KS"), Accept())
            self.assertEqual(len(s.hands[0]), 4)
            self.assertIsNone(s.provisional_winner)
            self.assertEqual(s.turn, 1)

    def test_drawing_skipping_or_accepting_ordinary_card_ends_return_chance(self):
        self.assertEqual(Play(finish_first(), Draw()).winner, 0)
        pending = play(finish_first(("10H", "8C")), "10H")
        self.assertEqual(Play(pending, Accept()).winner, 0)
        s = state(hands=(("AH",), ("7H", "8C")))
        s = Play(play(s, "AH"), Accept())
        self.assertEqual(Play(s, Skip()).winner, 0)

    def test_return_cancels_old_victory_priority(self):
        s = state(top="9S", hands=(("9H",), ("7H", "9D")), deck_first=("AH", "AS"))
        s = Play(play(s, "9H"), Accept())
        s = Play(play(s, "7H"), Accept())
        self.assertIsNone(s.provisional_winner)
        s = Play(play(s, "9D", "9H"), Accept())
        self.assertEqual(s.provisional_winner, 1)
        s = Play(play(s, "AH"), Accept())
        s = Play(play(s, "AS"), Accept())
        self.assertEqual(s.winner, 1)

    def test_recycling_is_deterministic_keeps_actual_top_and_conserves_cards(self):
        s = state(top="9H", hands=(("8H", "8C"), ("10D", "AD")),
                  pile=("7D", "QC", "KS", "JC"))
        s = replace(s, hands=(s.hands[0], s.hands[1] + s.deck), deck=())
        a, b = Play(s, Draw()), Play(s, Draw())
        self.assertEqual(a, b)
        self.assertEqual(a.pile, (card("JC"),))
        self.assertEqual(a.top, card("9H"))
        self.assertEqual(set(a.deck + a.hands[0][len(s.hands[0]):]), {card("7D"), card("QC"), card("KS")})
        self.assert_conserved(a)

    def test_partial_penalty_and_challenge_draws_cancel_unpaid_remainder(self):
        s = state(top="7S", pile=("9H", "QC", "7S"), draw_penalty=8)
        keep = s.deck[:1]
        s = replace(s, deck=keep, hands=(s.hands[0], s.hands[1] + s.deck[1:]))
        paid = Play(s, Draw())
        self.assertEqual(len(paid.hands[0]), len(s.hands[0]) + 3)
        self.assertEqual(paid.draw_penalty, 0)
        self.assertEqual(paid.turn, 1)
        self.assert_conserved(paid)
        challenged = Play(play(s, "JC", "7H"), Challenge())
        self.assertEqual(len(challenged.hands[0]), len(s.hands[0]) - 1 + 4)
        self.assertEqual(challenged.draw_penalty, 0)
        self.assertEqual(challenged.turn, 1)
        self.assert_conserved(challenged)

    def test_no_draw_available_requires_play_but_preserves_ace_skip(self):
        for top, fields in (("9H", {}), ("7H", {"draw_penalty": 8}), ("AH", {"skip_pending": True})):
            s = state(top=top, **fields)
            s = replace(s, hands=(s.hands[0], s.hands[1] + s.deck), deck=())
            self.assertNotIn(Draw(), MoveGenerator(s))
            self.assertTrue(declarations(s))
            with self.assertRaises(ValueError):
                Play(s, Draw())
            self.assertEqual(Skip() in MoveGenerator(s), top == "AH")

    def test_generated_moves_apply_without_mutating_input(self):
        global_rng = random.getstate()
        ordinary = state()
        states = [ordinary, state(top="QS", chosen_suit="D"), state(top="QC"),
                  state(top="AS", skip_pending=True), state(top="7S", draw_penalty=6),
                  state(top="KS", draw_penalty=4), play(ordinary, "JC", "7H"),
                  finish_first(), play(finish_first(), "7H"),
                  Play(Play(play(state(top="9S", hands=(("9H",), ("8C",))), "9H"), Accept()), Draw()),
                  NewGame(seed=123)]
        recycling = state(pile=("7D", "QC", "9H"))
        states.append(replace(recycling, deck=(),
                              hands=(recycling.hands[0], recycling.hands[1] + recycling.deck)))
        for s in states:
            original = deepcopy(s)
            self.assertEqual(list(MoveGenerator(s)), list(MoveGenerator(s)))
            moves = list(MoveGenerator(s))
            self.assertEqual(len(moves), len(set(moves)))
            for move in moves:
                result = Play(s, move)
                self.assertEqual(s, original)
                self.assertEqual(result, Play(s, move))
                MoveGenerator(result)
                self.assertIsNot(result, s)
                self.assert_conserved(result)
        self.assertEqual(random.getstate(), global_rng)
        with self.assertRaises(FrozenInstanceError):
            ordinary.turn = 1

    def test_invalid_states_are_rejected_at_api_boundary(self):
        s = state()
        invalid = [replace(s, turn=2), replace(s, deck=s.deck[:-1]),
                   replace(s, deck=s.deck + (s.deck[0],)),
                   replace(s, draw_penalty=-1), replace(s, phase="unknown"),
                   replace(s, chosen_suit="X"), replace(s, winner=0),
                   replace(play(s, "JC", "7H"), draw_penalty=0),
                   state(top="KS", draw_penalty=2),
                   replace(play(state(top="9S"), "JC", "KS"), draw_penalty=2),
                   replace(play(s, "JC", "AH"), skip_pending=False),
                   replace(s, rng_state=s.rng_state[:2] + ([],))]
        for bad in invalid:
            with self.subTest(state=bad), self.assertRaises(ValueError):
                list(MoveGenerator(bad))
            with self.assertRaises(ValueError):
                Play(bad, Draw())

    def test_all_declarations_resolve_each_victory_history_before_awarding_winner(self):
        for declared in CARDS:
            for chosen in (("H", "D", "C", "S") if declared.rank == "Q" else (None,)):
                for previous_empty, last_card, truthful, player in product((False, True), (False, True),
                                                                          (False, True), (0, 1)):
                    with self.subTest(declared=declared, chosen=chosen, prior=previous_empty,
                                      last=last_card, truthful=truthful, player=player):
                        other = 1 - player
                        actual = declared if truthful else card("JC" if declared != card("JC") else "JD")
                        top = Card("8" if declared.rank == "9" else "9", declared.suit)
                        s = identity_state(actual, top)
                        h0 = s.hands[0][:1] if last_card else s.hands[0]
                        h1 = () if previous_empty else s.hands[1]
                        released = s.hands[0][len(h0):] + s.hands[1][len(h1):]
                        s = replace(s, deck=released + s.deck, hands=(h0, h1),
                                    provisional_winner=1 if previous_empty else None)
                        if player:
                            s = mirror(s)
                        pending = Play(s, PlayCard(actual, declared, chosen))
                        self.assertEqual(pending.phase, "response")
                        self.assertIsNone(pending.winner)
                        self.assertEqual(pending.provisional_winner,
                                         other if previous_empty else player if last_card else None)
                        amount = 2 if declared.rank == "7" else 4 if declared == card("KS") else 0
                        accepted = Play(pending, Accept())
                        expected_sizes = [len(hand) for hand in pending.hands]
                        if previous_empty and amount:
                            expected_sizes[other] += amount
                            winner = player if last_card else None
                            claim = None
                            next_player = player
                        elif previous_empty and declared.rank == "A" and not last_card:
                            winner, claim, next_player = None, other, player
                        elif previous_empty:
                            winner, claim, next_player = other, None, other
                        else:
                            winner = None
                            claim = player if last_card else None
                            next_player = other
                        self.assertEqual(list(map(len, accepted.hands)), expected_sizes)
                        self.assertEqual(accepted.winner, winner)
                        self.assertEqual(accepted.provisional_winner, claim)
                        self.assertEqual(accepted.turn, next_player)
                        self.assertEqual(accepted.phase, "finished" if winner is not None else "turn")
                        self.assertEqual(accepted.draw_penalty, 0 if previous_empty else amount)
                        self.assertEqual(accepted.skip_pending, not previous_empty and declared.rank == "A")
                        self.assertEqual(accepted.top, declared)
                        self.assertEqual(accepted.chosen_suit, chosen)
                        resolved = Play(pending, Challenge())
                        challenge_winner = player if truthful else other
                        loser = 1 - challenge_winner
                        expected_sizes = [len(hand) for hand in pending.hands]
                        expected_sizes[loser] += amount + 2
                        winner = challenge_winner if not pending.hands[challenge_winner] else None
                        self.assertEqual(list(map(len, resolved.hands)), expected_sizes)
                        self.assertEqual(resolved.winner, winner)
                        self.assertEqual(resolved.turn, challenge_winner)
                        self.assertEqual(resolved.phase, "finished" if winner is not None else "turn")
                        self.assertIsNone(resolved.provisional_winner)
                        self.assertEqual(resolved.top, actual)
                        self.assertIsNone(resolved.chosen_suit)
                        self.assertEqual(resolved.draw_penalty, 0)
                        self.assertFalse(resolved.skip_pending)
                        for result in (accepted, resolved):
                            self.assert_conserved(result)
                            MoveGenerator(result)

    def test_every_penalty_counter_challenge_draws_exact_accumulation_or_available_cards(self):
        sevens = tuple(Card("7", suit) for suit in "HDCS")
        counter_pairs = [(top, declared) for top in sevens for declared in sevens]
        counter_pairs += [(card("7S"), card("KS")), (card("KS"), card("7S"))]
        for top, declared in counter_pairs:
            minimum = 4 if top == card("KS") else 2
            contribution = 4 if declared == card("KS") else 2
            for extra, physical_kind, last_card, player in product((0, 2, 10, 40),
                                                                   (None, "AH", "QC", "KS"),
                                                                   (False, True), (0, 1)):
                with self.subTest(top=top, declared=declared, extra=extra,
                                  physical=physical_kind, last=last_card, player=player):
                    actual = declared if physical_kind is None else card(physical_kind)
                    if physical_kind is not None and actual == declared:
                        actual = card("JS")
                    truthful = actual == declared
                    s = identity_state(actual, top, physical_top=card("9H"),
                                       draw_penalty=minimum + extra)
                    if last_card:
                        s = replace(s, hands=(s.hands[0][:1], s.hands[1]),
                                    deck=s.hands[0][1:] + s.deck)
                    if player:
                        s = mirror(s)
                    pending = Play(s, PlayCard(actual, declared))
                    self.assertEqual(pending.draw_penalty, minimum + extra + contribution)
                    loser = 1 - player if truthful else player
                    available = len(pending.deck) + len(pending.pile) - 1
                    drawn = min(minimum + extra + contribution + 2, available)
                    resolved = Play(pending, Challenge())
                    self.assertEqual(len(resolved.hands[loser]), len(pending.hands[loser]) + drawn)
                    self.assertEqual(resolved.hands[1 - loser], pending.hands[1 - loser])
                    self.assertEqual(resolved.turn, 1 - loser)
                    self.assertEqual(resolved.winner, player if truthful and last_card else None)
                    self.assertIsNone(resolved.provisional_winner)
                    self.assertEqual(resolved.top, actual)
                    self.assertEqual(resolved.draw_penalty, 0)
                    self.assertFalse(resolved.skip_pending)
                    self.assertIsNone(resolved.chosen_suit)
                    self.assert_conserved(resolved)
                    MoveGenerator(resolved)

    def test_partial_return_draws_remove_old_priority_even_with_only_one_card_available(self):
        for older_count, deck_count, truthful, player in product((1, 2, 4), range(9),
                                                                 (False, True), (0, 1)):
            with self.subTest(discards=older_count, deck=deck_count, truthful=truthful, player=player):
                actual = card("7H" if truthful else "JC")
                rest = tuple(c for c in CARDS if c not in (actual, card("7S")))
                pile = rest[:older_count - 1] + (card("7S"),)
                deck = rest[older_count - 1:older_count - 1 + deck_count]
                remaining = rest[older_count - 1 + deck_count:]
                s = GameState(deck=deck, pile=pile, hands=((actual,) + remaining, ()),
                              turn=0, top=card("7S"), draw_penalty=6, provisional_winner=1)
                if player:
                    s = mirror(s)
                other = 1 - player
                pending = Play(s, PlayCard(actual, card("7H")))
                available = older_count + deck_count
                accepted = Play(pending, Accept())
                self.assertEqual(len(accepted.hands[other]), min(8, available))
                self.assertEqual(accepted.hands[player], pending.hands[player])
                self.assertEqual(accepted.turn, player)
                self.assertIsNone(accepted.provisional_winner)
                self.assertIsNone(accepted.winner)
                self.assertEqual(accepted.draw_penalty, 0)
                resolved = Play(pending, Challenge())
                loser = other if truthful else player
                self.assertEqual(len(resolved.hands[loser]),
                                 len(pending.hands[loser]) + min(10, available))
                self.assertEqual(resolved.winner, None if truthful else other)
                self.assertEqual(resolved.turn, player if truthful else other)
                self.assertIsNone(resolved.provisional_winner)
                self.assertEqual(resolved.draw_penalty, 0)
                for result in (accepted, resolved):
                    self.assert_conserved(result)
                    MoveGenerator(result)

    def test_ace_chains_allow_a_separate_truth_check_at_every_link(self):
        for length in (1, 2, 3):
            for suits in product("HDCS", repeat=length):
                for player in (0, 1):
                    with self.subTest(suits=suits, player=player):
                        physical = []
                        for suit in suits:
                            actual = Card("A", suit)
                            if actual in physical:
                                actual = next(Card("9", s) for s in "HDCS" if Card("9", s) not in physical)
                            physical.append(actual)
                        return_card = Card("7", suits[-1])
                        hand = tuple(physical) + (return_card, card("8C"))
                        pile = (card("QH"),)
                        s = GameState(deck=tuple(c for c in CARDS if c not in hand + pile),
                                      pile=pile, hands=(hand, ()), turn=0, top=card("AH"),
                                      skip_pending=True, provisional_winner=1)
                        if player:
                            s = mirror(s)
                        other = 1 - player
                        for actual, suit in zip(physical, suits):
                            pending = Play(s, PlayCard(actual, Card("A", suit)))
                            self.assertEqual(set(MoveGenerator(pending)), {Accept(), Challenge()})
                            challenged = Play(pending, Challenge())
                            truthful = actual == Card("A", suit)
                            loser = other if truthful else player
                            self.assertEqual(len(challenged.hands[loser]), len(pending.hands[loser]) + 2)
                            self.assertEqual(challenged.winner, None if truthful else other)
                            self.assertEqual(challenged.turn, player if truthful else other)
                            self.assertIsNone(challenged.provisional_winner)
                            s = Play(pending, Accept())
                            self.assertEqual(s.turn, player)
                            self.assertFalse(s.skip_pending)
                            self.assertEqual(s.provisional_winner, other)
                            self.assertEqual(s.top, Card("A", suit))
                            self.assert_conserved(s)
                            MoveGenerator(challenged)
                        pending = Play(s, PlayCard(return_card, return_card))
                        accepted = Play(pending, Accept())
                        challenged = Play(pending, Challenge())
                        self.assertEqual(len(accepted.hands[other]), 2)
                        self.assertEqual(len(challenged.hands[other]), 4)
                        for result in (accepted, challenged):
                            self.assertIsNone(result.winner)
                            self.assertIsNone(result.provisional_winner)
                            self.assertEqual(result.turn, player)
                            self.assert_conserved(result)
                            MoveGenerator(result)

    def test_returned_players_cannot_reuse_old_victory_priority_in_any_suit(self):
        for first_suit, second_suit, truthful_return, player in product("HDCS", "HDCS", (False, True), (0, 1)):
            with self.subTest(first=first_suit, second=second_suit, truthful=truthful_return, player=player):
                return_actual = ("7" if truthful_return else "J") + first_suit
                final_actual = ("A" if second_suit != first_suit else "10") + second_suit
                s = state(top="8" + first_suit,
                          hands=(("9" + first_suit,), (return_actual, "QC")),
                          deck_first=("A" + first_suit, final_actual))
                if player:
                    s = mirror(s)
                other = 1 - player
                s = Play(play(s, "9" + first_suit), Accept())
                self.assertEqual(s.provisional_winner, player)
                s = Play(play(s, return_actual, "7" + first_suit), Accept())
                self.assertIsNone(s.provisional_winner)
                self.assertEqual(s.turn, other)
                s = Play(play(s, "QC", "9" + first_suit), Accept())
                self.assertEqual(s.provisional_winner, other)
                s = Play(play(s, "A" + first_suit), Accept())
                self.assertEqual(s.turn, player)
                final = play(s, final_actual, "A" + second_suit)
                self.assertEqual(Play(final, Accept()).winner, other)
                self.assertEqual(Play(final, Challenge()).winner,
                                 player if second_suit != first_suit else other)

    def test_exhausted_queen_still_requires_a_legal_bluff_in_every_chosen_suit(self):
        for printed_suit, chosen, player in product("HDCS", (None, "H", "D", "C", "S"), (0, 1)):
            with self.subTest(printed=printed_suit, chosen=chosen, player=player):
                top, actual = Card("Q", printed_suit), card("8C")
                others = tuple(c for c in CARDS if c not in (top, actual))
                s = GameState(deck=(), pile=(top,), hands=((actual,), others),
                              turn=0, top=top, chosen_suit=chosen)
                if player:
                    s = mirror(s)
                self.assertNotIn(Draw(), MoveGenerator(s))
                self.assertNotIn(Skip(), MoveGenerator(s))
                expected = set(CARDS) if chosen is None else {c for c in CARDS if c.suit == chosen or c.rank == "Q"}
                self.assertEqual(declarations(s), expected)
                with self.assertRaises(ValueError):
                    Play(s, Draw())
                pending = Play(s, PlayCard(actual, Card("9", chosen or "H")))
                accepted = Play(pending, Accept())
                self.assertEqual(accepted.provisional_winner, player)
                self.assertIn(Draw(), MoveGenerator(accepted))
                result = Play(accepted, Draw())
                self.assertEqual(result.winner, player)
                self.assertIn(top, result.hands[1 - player])
                self.assert_conserved(result)


if __name__ == "__main__":
    unittest.main()
