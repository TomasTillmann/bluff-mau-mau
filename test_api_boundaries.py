"""Public input, immutability, and replay contracts; standard-library tests."""

from copy import copy, deepcopy
from collections import UserString, namedtuple
from dataclasses import FrozenInstanceError, asdict, fields, replace
from decimal import Decimal
import pickle
import random
import unittest
from unittest.mock import ANY, MagicMock, Mock

from bluff_mau_mau import (
    Accept, CARDS, Card, Challenge, Draw, GameState, MoveGenerator, NewGame,
    Play, PlayCard, RANKS, SUITS, Skip,
)
from test_bluff_mau_mau import card, finish_first, mirror, play, state


class LabeledCard(Card):
    pass


class PublicBoundaryTests(unittest.TestCase):
    def assert_rejected(self, bad):
        before = deepcopy(bad)
        for operation in (MoveGenerator, lambda value: Play(value, Draw())):
            with self.subTest(operation=operation), self.assertRaises(ValueError):
                operation(bad)
        self.assertEqual(bad, before)

    def test_card_alphabet_is_exact(self):
        self.assertEqual(len(CARDS), 32)
        self.assertEqual(set(CARDS), {Card(rank, suit) for rank in RANKS for suit in SUITS})
        for rank in (None, True, 7, 7.0, "", "6", "11", "a", "Queen", " Q", "Q ", [], {},
                     ("Q",), UserString("Q")):
            for suit in SUITS:
                with self.subTest(rank=rank, suit=suit), self.assertRaises(ValueError):
                    Card(rank, suit)
        for suit in (None, False, 0, "", "h", "♥", "hearts", " H", "H ", [], {},
                     ("H",), UserString("H")):
            for rank in RANKS:
                with self.subTest(rank=rank, suit=suit), self.assertRaises(ValueError):
                    Card(rank, suit)

    def test_setup_rejects_wrong_seed_and_dealer_types(self):
        for seed in (None, True, False, 1.0, "1", b"1", [], {}, (), random.Random(1)):
            with self.subTest(seed=seed), self.assertRaises(ValueError):
                NewGame(seed=seed)
        for dealer in (None, True, False, -1, 2, 1.0, "0", b"0", [], {}, ()):
            with self.subTest(dealer=dealer), self.assertRaises(ValueError):
                NewGame(dealer=dealer)

    def test_setup_accepts_large_and_negative_integer_seeds(self):
        for seed in (-2**1024, -2**64, -1, 0, 1, 2**64, 2**1024):
            with self.subTest(seed=seed):
                first, second = NewGame(seed, 0), NewGame(seed, 1)
                self.assertEqual(first, NewGame(seed, 0))
                self.assertEqual(first.hands, second.hands[::-1])
                self.assertEqual(first.deck, second.deck)
                self.assertEqual(first.pile, second.pile)
                self.assertEqual(first.rng_state, second.rng_state)
                self.assertEqual((first.turn, second.turn), (1, 0))

    def test_setup_deal_and_rng_follow_documented_shuffle_convention(self):
        for seed in (0, 1, -72, 999_999):
            shuffled = list(CARDS)
            rng = random.Random(seed)
            rng.shuffle(shuffled)
            for dealer in (0, 1):
                with self.subTest(seed=seed, dealer=dealer):
                    result = NewGame(seed, dealer)
                    self.assertEqual(result.hands[1 - dealer], tuple(shuffled[:10:2]))
                    self.assertEqual(result.hands[dealer], tuple(shuffled[1:10:2]))
                    self.assertEqual(result.pile, (shuffled[10],))
                    self.assertEqual(result.deck, tuple(shuffled[11:]))
                    self.assertEqual(result.rng_state, rng.getstate())

    def test_only_game_states_are_accepted(self):
        for bad in (None, True, 0, "state", (), [], {}, asdict(NewGame()), Draw()):
            with self.subTest(value=bad):
                self.assert_rejected(bad)

    def test_mock_states_are_rejected_before_accessing_fields(self):
        normal = state()
        response = play(normal, "JC", "7H")
        finished = Play(play(state(top="9S", hands=(("9H",), ("8C",))), "9H"), Challenge())
        for s in (normal, state(top="AH", skip_pending=True), response, finished):
            before = pickle.dumps(s)
            for mock_type in (Mock, MagicMock):
                bad = mock_type(spec=GameState)
                for field in fields(s):
                    setattr(bad, field.name, getattr(s, field.name))
                for operation in (MoveGenerator, lambda value: Play(value, Draw())):
                    with self.subTest(phase=s.phase, mock=mock_type, operation=operation):
                        with self.assertRaises(ValueError):
                            operation(bad)
                self.assertEqual(bad.mock_calls, [])
                for field in fields(s):
                    self.assertIs(getattr(bad, field.name), getattr(s, field.name))
            self.assertEqual(pickle.dumps(s), before)

    def test_mock_tuples_are_rejected_before_invoking_container_methods(self):
        normal = state()
        response = play(normal, "JC", "7H")
        finished = Play(play(state(top="9S", hands=(("9H",), ("8C",))), "9H"), Challenge())
        for s in (normal, state(top="AH", skip_pending=True), response, finished):
            before = pickle.dumps(s)
            for mock_type in (Mock, MagicMock):
                for zone in ("deck", "pile", "hands", 0, 1, "rng_state", "rng_words"):
                    fake = mock_type(spec=tuple)
                    if type(zone) is int:
                        changes = {"hands": tuple(fake if i == zone else hand
                                                  for i, hand in enumerate(s.hands))}
                    elif zone == "rng_words":
                        changes = {"rng_state": (s.rng_state[0], fake, s.rng_state[2])}
                    else:
                        changes = {zone: fake}
                    bad = replace(s, **changes)
                    for operation in (MoveGenerator, lambda value: Play(value, Draw())):
                        with self.subTest(phase=s.phase, mock=mock_type, zone=zone, operation=operation):
                            with self.assertRaises(ValueError):
                                operation(bad)
                    self.assertEqual(fake.mock_calls, [])
            self.assertEqual(pickle.dumps(s), before)

    def test_genuine_tuple_wrappers_preserve_moves_and_transitions(self):
        class TupleSubclass(tuple):
            pass

        def named(values):
            return namedtuple("Wrapped", (f"v{i}" for i in range(len(values))))(*values)

        normal = state()
        response = play(normal, "JC", "7H")
        finished = Play(play(state(top="9S", hands=(("9H",), ("8C",))), "9H"), Challenge())
        for s in (normal, state(top="AH", skip_pending=True), response, finished):
            for wrap in (TupleSubclass, named):
                wrapped = replace(s, deck=wrap(s.deck), pile=wrap(s.pile),
                                  hands=wrap(tuple(wrap(hand) for hand in s.hands)),
                                  rng_state=wrap((s.rng_state[0], wrap(s.rng_state[1]), s.rng_state[2])))
                with self.subTest(phase=s.phase, wrapper=wrap):
                    self.assertEqual(MoveGenerator(wrapped), MoveGenerator(s))
                    for move in MoveGenerator(s):
                        self.assertEqual(Play(wrapped, move), Play(s, move))

    def test_mutable_or_misshaped_card_zones_are_rejected(self):
        s = state()
        bad_fields = (
            {"deck": list(s.deck)}, {"deck": None}, {"deck": {}},
            {"pile": list(s.pile)}, {"pile": None}, {"pile": {}},
            {"hands": list(s.hands)}, {"hands": None}, {"hands": {}},
            {"hands": ()}, {"hands": (s.hands[0],)},
            {"hands": s.hands + ((),)}, {"hands": (list(s.hands[0]), s.hands[1])},
            {"hands": (s.hands[0], list(s.hands[1]))},
            {"hands": (None, s.hands[1])}, {"hands": (s.hands[0], None)},
        )
        for changes in bad_fields:
            with self.subTest(changes=changes):
                self.assert_rejected(replace(s, **changes))

    def test_card_conservation_is_checked_in_every_zone(self):
        s = state()
        invalid = (
            replace(s, deck=s.deck[:-1]),
            replace(s, deck=s.deck + (s.deck[0],)),
            replace(s, deck=(s.pile[0],) + s.deck[1:]),
            replace(s, pile=()),
            replace(s, pile=(s.hands[0][0],)),
            replace(s, hands=(s.hands[0][:-1], s.hands[1])),
            replace(s, hands=(s.hands[0], s.hands[0])),
        )
        for bad in invalid:
            with self.subTest(state=bad):
                self.assert_rejected(bad)

    def test_noncard_zone_contents_are_rejected_without_type_errors(self):
        s = state()
        for value in (None, False, 7, "7H", ("7", "H"), [], {}, {"rank": "7", "suit": "H"}):
            for bad in (
                replace(s, deck=(value,) + s.deck[1:]),
                replace(s, pile=(value,)),
                replace(s, hands=((value,) + s.hands[0][1:], s.hands[1])),
                replace(s, hands=(s.hands[0], (value,) + s.hands[1][1:])),
            ):
                with self.subTest(value=value, state=bad):
                    self.assert_rejected(bad)

    def test_scalar_state_fields_reject_ordinary_malformed_values(self):
        s = state()
        cases = {
            "turn": (None, False, True, -1, 2, 0.0, 1.0, "0", [], {}),
            "top": (None, True, 7, "9H", ("9", "H"), [], {}),
            "draw_penalty": (None, False, True, -2, -1, 1, 3, 2.0, "2", [], {}),
            "skip_pending": (None, 0, 1, "false", [], {}),
            "phase": (None, False, 0, "", "TURN", "pending", b"turn", [], {}, UserString("turn")),
            "provisional_winner": (False, True, -1, 2, 0.0, "0", [], {}),
            "winner": (False, True, -1, 2, 0.0, "0", [], {}),
        }
        for field, values in cases.items():
            for value in values:
                with self.subTest(field=field, value=value):
                    self.assert_rejected(replace(s, **{field: value}))

    def test_card_subclasses_are_rejected_in_physical_zones_and_moves(self):
        s = state()
        for field in ("deck", "pile", "hands"):
            zones = s.hands if field == "hands" else (getattr(s, field),)
            for index, zone in enumerate(zones):
                labeled = LabeledCard(zone[0].rank, zone[0].suit)
                replaced = (labeled,) + zone[1:]
                value = tuple(replaced if i == index else hand
                              for i, hand in enumerate(s.hands)) if field == "hands" else replaced
                with self.subTest(field=field, index=index):
                    self.assert_rejected(replace(s, **{field: value}))
        move = PlayCard(card("JC"), card("9H"))
        for field in ("actual_card", "declared_card"):
            base = getattr(move, field)
            labeled = LabeledCard(base.rank, base.suit)
            with self.subTest(field=field), self.assertRaises(ValueError):
                Play(s, replace(move, **{field: labeled}))

    def test_card_subclass_top_is_rejected_before_penalty_counter_generation(self):
        for top, penalty, counter in (("7S", 2, "KS"), ("KS", 4, "7S")):
            initial = state(top=top, draw_penalty=penalty)
            for s in (initial, mirror(initial)):
                move = PlayCard(s.hands[s.turn][0], card(counter))
                with self.subTest(top=top, player=s.turn):
                    self.assertIn(move, MoveGenerator(s))
                    self.assertEqual(Play(s, move).draw_penalty, 6)
                    bad = replace(s, top=LabeledCard(s.top.rank, s.top.suit))
                    self.assert_rejected(bad)
                    with self.assertRaises(ValueError):
                        Play(bad, move)

    def test_card_subclass_top_is_rejected_before_truthful_final_challenge(self):
        initial = state(top="9S", hands=(("9H",), ("8C",)))
        for s in (initial, mirror(initial)):
            pending = play(s, "9H")
            with self.subTest(player=s.turn):
                self.assertEqual(MoveGenerator(pending), [Accept(), Challenge()])
                result = Play(pending, Challenge())
                self.assertEqual(result.winner, s.turn)
                self.assertEqual(result.hands[s.turn], ())
                self.assertEqual(len(result.hands[1 - s.turn]), 3)
                bad = replace(pending, top=LabeledCard("9", "H"))
                self.assert_rejected(bad)
                with self.assertRaises(ValueError):
                    Play(bad, Challenge())

    def test_queen_continuing_suit_must_be_valid_and_belong_to_a_queen(self):
        for top in ("9H", "7H", "AH", "KS", "QC"):
            s = state(top=top)
            values = ("H", "D", "C", "S") if top != "QC" else ()
            values += (True, 0, "", "h", "♥", "hearts", [], {}, UserString("H"))
            for value in values:
                with self.subTest(top=top, chosen_suit=value):
                    self.assert_rejected(replace(s, chosen_suit=value))
        for value in (None, *SUITS):
            self.assertTrue(MoveGenerator(state(top="QC", chosen_suit=value)))

    def test_effect_fields_cannot_contradict_effective_top(self):
        for top, penalty, skip in (
            ("9H", 2, False), ("AH", 2, False), ("QH", 2, False),
            ("KH", 4, False), ("KS", 2, False), ("7H", 1, False),
            ("9H", 0, True), ("QH", 0, True), ("7H", 0, True),
            ("AH", 2, True), ("7H", 2, True),
        ):
            with self.subTest(top=top, penalty=penalty, skip=skip):
                self.assert_rejected(state(top=top, draw_penalty=penalty, skip_pending=skip))

    def test_revealed_special_cards_may_have_no_pending_effect(self):
        for top in ("7H", "7S", "KS", "AH", "AS", "QH"):
            with self.subTest(top=top):
                moves = MoveGenerator(state(top=top))
                self.assertIn(Draw(), moves)
                self.assertNotIn(Skip(), moves)

    def test_empty_hands_and_claims_must_agree(self):
        ordinary = state()
        provisional = finish_first()
        invalid = (
            replace(ordinary, provisional_winner=0),
            replace(ordinary, provisional_winner=1),
            replace(provisional, provisional_winner=None),
            replace(provisional, provisional_winner=1),
            replace(provisional, turn=0),
            replace(provisional, winner=0),
            replace(ordinary, phase="finished"),
            replace(ordinary, phase="finished", winner=0),
        )
        for bad in invalid:
            with self.subTest(state=bad):
                self.assert_rejected(bad)

    def test_both_empty_response_requires_the_responders_victory_claim(self):
        for final_card in ("10H", "AH", "7H"):
            pending = play(finish_first((final_card,)), final_card)
            for s in (pending, mirror(pending)):
                with self.subTest(card=final_card, responder=s.turn):
                    self.assertEqual(s.hands, ((), ()))
                    self.assertEqual(s.provisional_winner, s.turn)
                    self.assertEqual(MoveGenerator(s), [Accept(), Challenge()])
                    for move in (Accept(), Challenge()):
                        winner = 1 - s.turn if final_card == "7H" or move == Challenge() else s.turn
                        self.assertEqual(Play(s, move).winner, winner)
                    bad = replace(s, provisional_winner=1 - s.turn)
                    for operation in (MoveGenerator, lambda value: Play(value, Accept()),
                                      lambda value: Play(value, Challenge())):
                        with self.subTest(operation=operation), self.assertRaises(ValueError):
                            operation(bad)

    def test_response_requires_a_played_card_and_complete_declaration(self):
        s = state()
        invalid = [replace(s, phase="response")]
        for declared, suit in (("7H", None), ("KS", None), ("AH", None), ("QH", "S")):
            initial = state(top="9S" if declared == "KS" else "9H")
            pending = play(initial, "JC", declared, suit)
            if declared.startswith("Q"):
                invalid.append(replace(pending, chosen_suit=None))
            elif declared.startswith("A"):
                invalid.append(replace(pending, skip_pending=False))
            else:
                invalid.append(replace(pending, draw_penalty=0))
        for bad in invalid:
            with self.subTest(state=bad):
                self.assert_rejected(bad)

    def test_rng_state_rejects_mutable_containers_and_malformed_payloads(self):
        s = state()
        version, internal, gaussian = s.rng_state
        invalid = (
            None, [], {}, (), (version,), (version, internal),
            list(s.rng_state), (version, list(internal), gaussian),
            (version, internal, []), (version, internal, {}),
            (version, internal, True), (version, internal, "0.0"),
            (version, internal, float("nan")), (version, internal, float("inf")),
            (version, internal, float("-inf")),
            (99, internal, gaussian), (version, (), gaussian),
            (version, internal[:-1], gaussian),
            (version, internal[:-1] + (-1,), gaussian),
            (version, internal[:-1] + (625,), gaussian),
            (version, ([],) + internal[1:], gaussian),
            (version, (None,) + internal[1:], gaussian),
        )
        for index, value in enumerate(invalid):
            with self.subTest(case=index):
                self.assert_rejected(replace(s, rng_state=value))

    def test_rng_state_with_cached_gaussian_is_valid_and_preserved(self):
        rng = random.Random(71)
        rng.gauss(0, 1)
        self.assertIsInstance(rng.getstate()[2], float)
        s = replace(state(), rng_state=rng.getstate())
        for move in (Draw(), PlayCard(card("JC"), card("7H"))):
            with self.subTest(move=move):
                self.assertEqual(Play(s, move).rng_state, s.rng_state)

    def test_rng_numeric_fields_require_primitive_integers(self):
        s = state()
        _, internal, gaussian = s.rng_state
        for value in (False, True, 3.0, Decimal(3), Decimal("sNaN")):
            with self.subTest(version=value):
                self.assert_rejected(replace(s, rng_state=(value, internal, gaussian)))
        for version in (2, 3):
            for value in (False, 0.0, Decimal(0), Decimal("sNaN")):
                for index in (0, len(internal) // 2, len(internal) - 1):
                    words = internal[:index] + (value,) + internal[index + 1:]
                    with self.subTest(version=version, value=value, index=index):
                        self.assert_rejected(replace(s, rng_state=(version, words, gaussian)))

    def test_malformed_moves_raise_value_error_without_changing_state(self):
        s = state()
        bad_moves = [None, True, 0, "Draw", [], {}, (), Draw, PlayCard]
        for value in (None, False, 7, "JC", ("J", "C"), [], {}):
            bad_moves.extend((PlayCard(value, card("9H")), PlayCard(card("JC"), value)))
        for value in (True, 0, "", "X", "h", [], {}, UserString("H")):
            bad_moves.append(PlayCard(card("JC"), card("QH"), value))
        before = pickle.dumps(s)
        for move in bad_moves:
            with self.subTest(move=move), self.assertRaises(ValueError):
                Play(s, move)
            self.assertEqual(pickle.dumps(s), before)

    def test_wildcard_moves_and_card_fields_are_rejected_before_equality(self):
        normal = state()
        cases = (normal, state(top="AH", skip_pending=True),
                 state(top="7S", draw_penalty=6), state(top="KS", draw_penalty=4),
                 play(normal, "JC", "9H"),
                 Play(play(state(top="9S", hands=(("9H",), ("8C",))), "9H"), Challenge()))
        for initial in cases:
            for s in (initial, mirror(initial)):
                before = pickle.dumps(s)
                moves = MoveGenerator(s)
                bad_moves = [ANY]
                bad_moves.extend(mock(spec=kind) for mock in (Mock, MagicMock)
                                 for kind in (PlayCard, Draw, Skip, Accept, Challenge))
                card_moves = [move for move in moves if type(move) is PlayCard]
                for move in card_moves or [PlayCard(card("JC"), card("9H"))]:
                    bad_moves.extend((replace(move, actual_card=ANY),
                                      replace(move, declared_card=ANY)))
                for move in bad_moves:
                    with self.subTest(phase=s.phase, top=s.top, player=s.turn, move=move):
                        with self.assertRaises(ValueError):
                            Play(s, move)
                    self.assertEqual(pickle.dumps(s), before)
                self.assertEqual(MoveGenerator(s), moves)
                for move in moves:
                    MoveGenerator(Play(s, move))

    def test_unavailable_moves_are_rejected_in_every_phase(self):
        normal = state()
        response = play(normal, "JC", "9H")
        ace = state(top="AH", skip_pending=True)
        seven = state(top="7H", draw_penalty=2)
        king = state(top="KS", draw_penalty=4)
        invalid_cases = (
            (normal, (Accept(), Challenge(), Skip(), PlayCard(card("JC"), card("8D")),
                      PlayCard(card("10D"), card("9H")),
                      PlayCard(card("JC"), card("QH")),
                      PlayCard(card("JC"), card("9H"), "H"))),
            (response, (Draw(), Skip(), PlayCard(card("10D"), card("9H")))),
            (ace, (Draw(), Accept(), Challenge(), PlayCard(card("JC"), card("QH"), "H"))),
            (seven, (Skip(), Accept(), Challenge(), PlayCard(card("JC"), card("KS")),
                     PlayCard(card("JC"), card("QH"), "H"))),
            (king, (Skip(), Accept(), Challenge(), PlayCard(card("JC"), card("7H")))),
        )
        for s, moves in invalid_cases:
            before = pickle.dumps(s)
            for move in moves:
                with self.subTest(phase=s.phase, top=s.top, move=move), self.assertRaises(ValueError):
                    Play(s, move)
                self.assertEqual(pickle.dumps(s), before)

    def test_terminal_states_refuse_every_action(self):
        finished = Play(play(state(top="9S", hands=(("9H",), ("8C",))), "9H"), Challenge())
        self.assertEqual(finished.winner, 0)
        self.assertEqual(finished.turn, finished.winner)
        self.assertEqual(MoveGenerator(finished), [])
        before = pickle.dumps(finished)
        moves = [Accept(), Challenge(), Draw(), Skip(), None, {}, []]
        moves.extend(PlayCard(actual, declared, suit)
                     for actual in CARDS for declared in CARDS
                     for suit in SUITS if declared.rank == "Q")
        moves.extend(PlayCard(actual, declared)
                     for actual in CARDS for declared in CARDS if declared.rank != "Q")
        for move in moves:
            with self.subTest(move=move), self.assertRaises(ValueError):
                Play(finished, move)
        self.assertEqual(pickle.dumps(finished), before)

    def test_terminal_turn_must_identify_winner(self):
        finished = Play(play(state(top="9S", hands=(("9H",), ("8C",))), "9H"), Challenge())
        for winner in (0, 1):
            terminal = replace(finished, hands=finished.hands[::1 if winner == 0 else -1],
                               turn=winner, winner=winner)
            self.assertEqual(MoveGenerator(terminal), [])
            self.assert_rejected(replace(terminal, turn=1 - winner))

    def test_terminal_states_clear_pending_effects_and_claim(self):
        finished = Play(play(state(top="9S", hands=(("9H",), ("8C",))), "9H"), Challenge())
        for changes in (
            {"provisional_winner": 0},
            {"top": card("7H"), "draw_penalty": 2},
            {"top": card("AH"), "skip_pending": True},
            {"phase": "turn"}, {"phase": "response"}, {"winner": None},
        ):
            with self.subTest(changes=changes):
                self.assert_rejected(replace(finished, **changes))


class ImmutableReplayTests(unittest.TestCase):
    def test_mutable_text_cannot_alias_transition_results(self):
        initial = state(top="QC", chosen_suit="H")
        pending = play(initial, "JC", "QH", "H")
        finished = Play(play(state(top="9S", hands=(("9H",), ("8C",))), "9H"), Challenge())
        for s in (initial, pending, finished):
            before = pickle.dumps(s)
            for field in ("phase", "chosen_suit") if s.chosen_suit else ("phase",):
                alias = UserString(getattr(s, field))
                malformed = replace(s, **{field: alias})
                for operation in (MoveGenerator, lambda value: Play(value, Draw())):
                    with self.subTest(field=field, phase=s.phase), self.assertRaises(ValueError):
                        operation(malformed)
                alias.data = "changed"
                self.assertEqual(pickle.dumps(s), before)
        for suit in SUITS:
            alias = UserString(suit)
            move = PlayCard(card("JC"), card("QH"), alias)
            with self.subTest(suit=suit), self.assertRaises(ValueError):
                Play(initial, move)
            result = Play(initial, replace(move, chosen_suit=suit))
            before = pickle.dumps(result)
            alias.data = "changed"
            self.assertEqual(pickle.dumps(result), before)
            self.assertIs(type(result.chosen_suit), str)
            self.assertIs(type(result.phase), str)
            for c in (*result.deck, *result.pile, *result.hands[0], *result.hands[1], result.top):
                self.assertIs(type(c.rank), str)
                self.assertIs(type(c.suit), str)

    def test_all_public_value_objects_are_frozen(self):
        objects = (card("7H"), PlayCard(card("JC"), card("QH"), "S"),
                   Draw(), Skip(), Accept(), Challenge(), state())
        for value in objects:
            for field in fields(value):
                with self.subTest(type=type(value).__name__, field=field.name):
                    with self.assertRaises(FrozenInstanceError):
                        setattr(value, field.name, getattr(value, field.name))
                    with self.assertRaises(FrozenInstanceError):
                        delattr(value, field.name)
            self.assertFalse(hasattr(value, "__dict__"))

    def test_state_containers_and_nested_rng_are_immutable(self):
        s = NewGame(89)
        for container in (s.deck, s.pile, s.hands, *s.hands, s.rng_state, s.rng_state[1]):
            with self.subTest(container_type=type(container)):
                self.assertIsInstance(container, tuple)
                with self.assertRaises(TypeError):
                    container[0] = container[0]
        self.assertTrue(all(type(word) is int for word in s.rng_state[1]))
        self.assertIsNone(s.rng_state[2])

    def test_legal_move_lists_are_independent_caller_owned_containers(self):
        for s in (state(), play(state(), "JC", "9H")):
            first, second = MoveGenerator(s), MoveGenerator(s)
            self.assertIsInstance(first, list)
            self.assertIsNot(first, second)
            first.reverse()
            first.append(None)
            first.clear()
            self.assertEqual(MoveGenerator(s), second)

    def test_recycling_matches_local_rng_and_preserves_actual_hidden_top(self):
        s = state(top="9H", hands=(("8H", "8C"), ("10D", "AD")),
                  pile=("7D", "QC", "KS", "8S", "JC"))
        s = replace(s, deck=(), hands=(s.hands[0], s.hands[1] + s.deck))
        expected = list(s.pile[:-1])
        rng = random.Random()
        rng.setstate(s.rng_state)
        rng.shuffle(expected)
        before = pickle.dumps(s)
        result = Play(s, Draw())
        self.assertEqual(result.hands[0], s.hands[0] + (expected[0],))
        self.assertEqual(result.deck, tuple(expected[1:]))
        self.assertEqual(result.pile, (card("JC"),))
        self.assertEqual(result.top, card("9H"))
        self.assertEqual(result.rng_state, rng.getstate())
        self.assertNotEqual(result.rng_state, s.rng_state)
        self.assertEqual(pickle.dumps(s), before)

    def test_global_random_state_is_isolated_for_setup_validation_and_recycling(self):
        global_before = random.getstate()
        try:
            for seed in (2, 933, -4):
                random.seed(seed)
                expected = random.getstate()
                new = NewGame(seed)
                MoveGenerator(new)
                s = state(pile=("7D", "QC", "9H"))
                s = replace(s, deck=(), hands=(s.hands[0], s.hands[1] + s.deck))
                drawn = Play(s, Draw())
                challenged = Play(play(s, "JC", "7H"), Challenge())
                with self.assertRaises(ValueError):
                    Play(s, Skip())
                self.assertEqual(random.getstate(), expected)
                random.seed(seed + 100)
                self.assertEqual(drawn, Play(s, Draw()))
                self.assertEqual(challenged, Play(play(s, "JC", "7H"), Challenge()))
        finally:
            random.setstate(global_before)

    def test_copied_states_and_branched_transitions_do_not_interfere(self):
        s = state(pile=("7D", "QC", "9H"))
        s = replace(s, deck=(), hands=(s.hands[0], s.hands[1] + s.deck))
        before = deepcopy(s)
        draw_branch = Play(copy(s), Draw())
        pending_branch = Play(deepcopy(s), PlayCard(card("JC"), card("7H")))
        accepted = Play(pending_branch, Accept())
        challenged = Play(pending_branch, Challenge())
        self.assertEqual(s, before)
        self.assertEqual(draw_branch, Play(s, Draw()))
        self.assertEqual(pending_branch, play(s, "JC", "7H"))
        self.assertEqual(accepted, Play(pending_branch, Accept()))
        self.assertEqual(challenged, Play(pending_branch, Challenge()))
        self.assertNotEqual(accepted, challenged)
        self.assertNotEqual(draw_branch.rng_state, s.rng_state)
        self.assertEqual(pending_branch.rng_state, s.rng_state)

    def test_pickling_preserves_state_moves_and_future_transitions(self):
        initial = state(pile=("7D", "QC", "9H"))
        initial = replace(initial, deck=(), hands=(initial.hands[0], initial.hands[1] + initial.deck))
        pending = play(initial, "JC", "7H")
        final = Play(play(state(top="9S", hands=(("9H",), ("8C",))), "9H"), Challenge())
        for protocol in (0, 2, pickle.HIGHEST_PROTOCOL):
            for s in (NewGame(6), initial, pending, Play(pending, Accept()), final):
                with self.subTest(protocol=protocol, phase=s.phase):
                    restored = pickle.loads(pickle.dumps(s, protocol=protocol))
                    self.assertEqual(restored, s)
                    self.assertEqual(MoveGenerator(restored), MoveGenerator(s))
                    for move in MoveGenerator(s)[:5]:
                        restored_move = pickle.loads(pickle.dumps(move, protocol=protocol))
                        self.assertEqual(move, restored_move)
                        self.assertEqual(Play(restored, restored_move), Play(s, move))

    def test_explicit_move_sequence_replays_after_every_serialization_boundary(self):
        initial = state(top="9H", hands=(("JC", "AH", "8C"), ("7D", "10D", "AD")))
        moves = (
            PlayCard(card("JC"), card("7H")), Accept(),
            PlayCard(card("7D"), card("7D")), Challenge(),
            PlayCard(card("10D"), card("QD"), "S"), Accept(), Draw(),
        )
        live = restored = initial
        original = pickle.dumps(initial)
        for move in moves:
            live = Play(live, move)
            restored = pickle.loads(pickle.dumps(Play(restored, move)))
            self.assertEqual(live, restored)
            self.assertEqual(MoveGenerator(live), MoveGenerator(restored))
        self.assertEqual(pickle.dumps(initial), original)

    def test_obviously_false_declarations_are_never_pruned(self):
        s = state(top="9H", hands=(("JC", "QH", "9C"), ("7H", "AH")),
                  pile=("KH", "9H"))
        moves = MoveGenerator(s)
        declarations_known_elsewhere = ("7H", "AH", "KH", "9H", "9C")
        for declared in declarations_known_elsewhere:
            for actual in s.hands[0]:
                move = PlayCard(actual, card(declared))
                with self.subTest(actual=actual, declared=declared):
                    self.assertIn(move, moves)
                    pending = Play(s, move)
                    self.assertEqual(pending.pile[-1], actual)
                    self.assertEqual(pending.top, card(declared))
        for actual in s.hands[0]:
            for queen in (card("QH"), card("QD"), card("QC"), card("QS")):
                for suit in SUITS:
                    self.assertIn(PlayCard(actual, queen, suit), moves)

    def test_bluff_completeness_survives_penalties_empty_deck_and_last_card(self):
        cases = (
            ("KS", 4, False, ("7S",)),
            ("7H", 2, False, ("7H", "7D", "7C", "7S")),
            ("7S", 6, False, ("7H", "7D", "7C", "7S", "KS")),
            ("AH", 0, True, ("AH", "AD", "AC", "AS")),
        )
        for top, penalty, skip, declarations in cases:
            s = state(top=top, hands=(("JC",), ("8C",)),
                      draw_penalty=penalty, skip_pending=skip)
            s = replace(s, deck=(), hands=(s.hands[0], s.hands[1] + s.deck))
            moves = MoveGenerator(s)
            expected = {PlayCard(card("JC"), card(declared)) for declared in declarations}
            with self.subTest(top=top):
                self.assertEqual({m for m in moves if isinstance(m, PlayCard)}, expected)
                self.assertNotIn(Draw(), moves)
                for move in expected:
                    pending = Play(s, move)
                    self.assertEqual(pending.hands[0], ())
                    self.assertIsNone(pending.winner)
                    self.assertEqual(MoveGenerator(pending), [Accept(), Challenge()])


if __name__ == "__main__":
    unittest.main()
