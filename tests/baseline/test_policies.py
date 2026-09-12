"""Blind behavioral tests authored only from CONTRACT.md and the game rules."""

from collections import Counter
from dataclasses import replace
from math import nextafter
from random import Random
import random
import unittest

from bluff_mau_mau import (
    Accept, Card, CARDS, Challenge, Draw, GameState, MoveGenerator, Play,
    PlayCard, Skip,
)
from src.engines import Observation
from src.engines.baseline import HonestFirst, MixedGreedy, RandomLegal


def c(identity):
    return Card(identity[:-1], identity[-1])


def position(hand, opponent=(c("8S"), c("10S"), c("JS")), *,
             top=c("9H"), pile=None, phase="turn", draw_penalty=0,
             skip_pending=False, chosen_suit=None, deck_count=None):
    """Allocate every real card exactly once; declarations may be false."""
    hand, opponent = tuple(hand), tuple(opponent)
    pile = (top,) if pile is None else tuple(pile)
    occupied = hand + opponent + pile
    assert len(occupied) == len(set(occupied))
    remaining = tuple(card for card in CARDS if card not in occupied)
    if deck_count is None:
        deck = remaining
    else:
        assert 0 <= deck_count <= len(remaining)
        deck = remaining[:deck_count]
        pile = remaining[deck_count:] + pile
    claimant = 0 if not hand else (1 if not opponent else None)
    state = GameState(deck=deck, pile=pile, hands=(hand, opponent), turn=0,
                      top=top, chosen_suit=chosen_suit, phase=phase,
                      draw_penalty=draw_penalty, skip_pending=skip_pending,
                      provisional_winner=claimant)
    MoveGenerator(state)  # The trusted engine validates the entire fixture.
    return state


def observation(state, known=()):
    player = state.turn
    return Observation(
        player=player, hand=state.hands[player],
        opponent_count=len(state.hands[1 - player]), top=state.top,
        chosen_suit=state.chosen_suit, phase=state.phase,
        draw_penalty=state.draw_penalty, skip_pending=state.skip_pending,
        provisional_winner=state.provisional_winner,
        deck_count=len(state.deck), pile_count=len(state.pile),
        known_pile_cards=frozenset(known),
    )


def policies():
    return (RandomLegal(), HonestFirst(), MixedGreedy(),
            MixedGreedy(0, 0, 0), MixedGreedy(100, 100, 100))


def ordinary(card):
    return card.rank not in ("7", "Q", "A") and card != c("KS")


class BoundaryRandom(Random):
    """Fix the public probability sample while making ties reproducible."""

    def __init__(self, sample):
        super().__init__(0)
        self.sample = sample

    def random(self):
        return self.sample

    def getrandbits(self, count):
        return super().getrandbits(count)


class PolicyTests(unittest.TestCase):
    def decide(self, bot, state, *, rng=None, known=()):
        legal = tuple(MoveGenerator(state))
        view = observation(state, known)
        before = (repr(view), repr(legal), repr(state), bot.name)
        move = bot(view, legal, Random(0) if rng is None else rng)
        self.assertIn(move, legal)
        self.assertEqual((repr(view), repr(legal), repr(state), bot.name), before)
        return move

    def assert_all_choose(self, state, expected, known=()):
        for bot in policies():
            for seed in range(8):
                with self.subTest(bot=bot.name, seed=seed):
                    self.assertEqual(self.decide(bot, state, rng=Random(seed),
                                                 known=known), expected)

    def test_names_defaults_and_direct_non_grid_percentages(self):
        self.assertEqual(RandomLegal().name, "RandomLegal[uniform]")
        self.assertEqual(HonestFirst().name, "HonestFirst[B0-N0-C0]")
        self.assertEqual(MixedGreedy().name, "MixedGreedy[B10-N10-C20]")
        for values in ((0, 0, 0), (100, 100, 100), (1, 37, 99)):
            bot = MixedGreedy(*values)
            self.assertEqual((bot.bluff, bot.no_truth_bluff, bot.challenge), values)
            self.assertEqual(bot.name, "MixedGreedy[B%d-N%d-C%d]" % values)
            self.assertTrue(callable(bot))

    def test_each_percentage_rejects_invalid_types_and_ranges(self):
        for field in ("bluff", "no_truth_bluff", "challenge"):
            for value in (-1, 101, True, False, 10.0, 2.5, "10", None):
                with self.subTest(field=field, value=value):
                    with self.assertRaises(ValueError):
                        MixedGreedy(**{field: value})

    def test_empty_legal_tuple_raises_value_error(self):
        view = observation(position((c("8C"), c("10D"))))
        for bot in policies():
            with self.subTest(bot=bot.name):
                with self.assertRaises(ValueError):
                    bot(view, (), Random(0))

    def test_empty_hand_accepts_an_immediate_win_even_over_known_bluff(self):
        state = position((), top=c("9H"), pile=(c("9H"), c("JC")),
                         phase="response")
        self.assert_all_choose(state, Accept(), known=(c("9H"),))
        self.assertEqual(Play(state, Accept()).winner, 0)

    def test_both_empty_accept_final_ace_or_ordinary_card(self):
        for top, skip in ((c("AH"), True), (c("9H"), False)):
            state = position((), (), top=top, pile=(top, c("JC")),
                             phase="response", skip_pending=skip)
            self.assert_all_choose(state, Accept(), known=(top,))
            self.assertEqual(Play(state, Accept()).winner, 0)

    def test_truthful_last_penalty_returns_an_empty_opponent(self):
        for final, top in ((c("7H"), c("9H")), (c("KS"), c("9S"))):
            state = position((final,), (), top=top)
            expected = PlayCard(final, final)
            self.assert_all_choose(state, expected)
            self.assertEqual(Play(Play(state, expected), Accept()).winner, 0)
            self.assertEqual(Play(Play(state, expected), Challenge()).winner, 0)

    def test_truthful_last_ace_against_one_card_is_a_guaranteed_play(self):
        state = position((c("AH"),), (c("8D"),))
        self.assert_all_choose(state, PlayCard(c("AH"), c("AH")))

    def test_accepts_to_make_a_guaranteed_final_play_before_catching_bluff(self):
        scenarios = (
            position((c("7H"),), (), top=c("7H"),
                     pile=(c("8D"), c("JC")), phase="response", draw_penalty=2),
            position((c("AH"),), (c("8D"),), top=c("AH"),
                     pile=(c("9C"), c("JC")), phase="response", skip_pending=True),
            position((c("KS"),), (), top=c("QD"), chosen_suit="S",
                     pile=(c("QD"), c("JC")), phase="response"),
        )
        for state in scenarios:
            final = state.hands[0][0]
            if state.skip_pending:
                self.assert_all_choose(state, PlayCard(final, final), known=(state.pile[0],))
            else:
                self.assert_all_choose(state, Accept(), known=(state.pile[0],))
                accepted = Play(state, Accept())
                self.assert_all_choose(accepted, PlayCard(final, final))

    def test_an_illegal_truthful_last_penalty_is_not_a_guaranteed_play(self):
        state = position((c("7C"),), (), top=c("9H"),
                         pile=(c("8D"), c("9H")), phase="response")
        self.assertEqual(self.decide(MixedGreedy(0, 0, 100), state), Challenge())

    def test_both_empty_must_challenge_a_return_penalty(self):
        for top, penalty in ((c("7H"), 2), (c("KS"), 4)):
            state = position((), (), top=top, pile=(c("8D"), top),
                             phase="response", draw_penalty=penalty)
            self.assert_all_choose(state, Challenge())

    def test_one_card_against_empty_opponents_ace_is_a_last_chance(self):
        for last in (c("8C"), c("AH")):
            state = position((last,), (), top=c("AD"),
                             pile=(c("9H"), c("AD")), phase="response",
                             skip_pending=True)
            self.assert_all_choose(state, Challenge())

    def test_known_bluff_is_detected_from_hand_or_personal_pile_knowledge(self):
        hand_known = position((c("9H"), c("8C")), top=c("9H"),
                              pile=(c("8D"), c("JC")), phase="response")
        pile_known = position((c("8C"), c("10D")), top=c("9H"),
                              pile=(c("9H"), c("JC")), phase="response")
        self.assert_all_choose(hand_known, Challenge())
        self.assert_all_choose(pile_known, Challenge(), known=(c("9H"),))

    def test_rank_or_suit_match_and_hidden_pile_do_not_prove_a_bluff(self):
        state = position((c("9C"), c("8H")), top=c("9H"),
                         pile=(c("8D"), c("9H"), c("JC")), phase="response")
        for bot in (HonestFirst(), MixedGreedy(100, 100, 0)):
            self.assertEqual(self.decide(bot, state, known=(c("8D"),)), Accept())

    def test_no_actual_seven_does_not_force_a_last_chance_challenge(self):
        for top, penalty, skip in ((c("7H"), 2, False), (c("AH"), 0, True)):
            state = position((c("8C"), c("10D")), (), top=top,
                             pile=(c("9H"), top), phase="response",
                             draw_penalty=penalty, skip_pending=skip)
            for bot in (HonestFirst(), MixedGreedy(0, 0, 0)):
                self.assertEqual(self.decide(bot, state), Accept())

    def test_return_filter_keeps_bluff_penalties_and_nonfinal_aces(self):
        state = position((c("8C"), c("10D")), ())
        self.assertFalse(any(card.rank == "7" for card in state.hands[0]))
        for bot in policies():
            for seed in range(24):
                move = self.decide(bot, state, rng=Random(seed))
                self.assertIsInstance(move, PlayCard)
                self.assertTrue(move.declared_card.rank in ("7", "A")
                                or move.declared_card == c("KS"))
                if move.declared_card.rank == "A":
                    self.assertGreater(len(state.hands[0]) - 1, 0)

    def test_one_card_return_filter_excludes_final_ace_and_passive_moves(self):
        state = position((c("8C"),), ())
        for bot in policies():
            for seed in range(12):
                move = self.decide(bot, state, rng=Random(seed))
                self.assertIsInstance(move, PlayCard)
                self.assertEqual(move.declared_card, c("7H"))

    def test_partial_penalty_draws_and_recycling_still_allow_a_return(self):
        top, one_draw = c("7H"), c("JC")
        hand = tuple(card for card in CARDS if card not in (top, one_draw))
        for deck_count in (0, 1):
            state = position(hand, (), top=top, draw_penalty=8,
                             deck_count=deck_count)
            for bot in policies():
                move = self.decide(bot, state)
                self.assertIsInstance(move, PlayCard)
                self.assertEqual(move.declared_card.rank, "7")
                returned = Play(Play(state, move), Accept())
                self.assertGreater(len(returned.hands[1]), 0)
                self.assertLess(len(returned.hands[1]), 10)

    def test_no_draw_or_recyclable_cards_before_play_does_not_disable_return(self):
        top = c("9H")
        state = position(tuple(card for card in CARDS if card != top), (),
                         top=top, deck_count=0)
        self.assertEqual(len(state.pile), 1)
        for bot in policies():
            move = self.decide(bot, state)
            self.assertIsInstance(move, PlayCard)
            self.assertIn(move.declared_card.rank, ("7", "A"))

    def test_already_lost_one_card_ace_turn_still_returns_a_legal_move(self):
        for last in (c("8C"), c("AD")):
            state = position((last,), (), top=c("AH"), skip_pending=True)
            for bot in policies():
                self.decide(bot, state)

    def test_greedy_policies_never_draw_or_skip_when_truth_survives(self):
        states = (
            position((c("8H"), c("10D"))),
            position((c("AD"), c("8C")), top=c("AH"), skip_pending=True),
        )
        for state in states:
            for bot in (HonestFirst(), MixedGreedy(0, 100, 100),
                        MixedGreedy(100, 0, 0)):
                for seed in range(8):
                    self.assertIsInstance(self.decide(bot, state, rng=Random(seed)), PlayCard)

    def test_honest_prefers_truth_and_accepts_uncertain_responses(self):
        state = position((c("8H"), c("10D")))
        move = self.decide(HonestFirst(), state)
        self.assertEqual(move.actual_card, move.declared_card)
        response = position((c("8C"), c("10D")), top=c("9H"),
                            pile=(c("8D"), c("9H")), phase="response")
        self.assertEqual(self.decide(HonestFirst(), response), Accept())

    def test_honest_draws_or_skips_without_truth(self):
        self.assertEqual(self.decide(HonestFirst(), position((c("8C"), c("10D")))), Draw())
        state = position((c("8C"), c("10D")), top=c("AH"), skip_pending=True)
        self.assertEqual(self.decide(HonestFirst(), state), Skip())

    def test_forced_bluff_ignores_zero_no_truth_percentage(self):
        hand, top = (c("8C"), c("10D")), c("9H")
        opponent = tuple(card for card in CARDS if card not in hand + (top,))
        state = position(hand, opponent, top=top, deck_count=0)
        self.assertFalse(any(isinstance(move, (Draw, Skip)) for move in MoveGenerator(state)))
        for bot in (HonestFirst(), MixedGreedy(0, 0, 0)):
            move = self.decide(bot, state)
            self.assertIsInstance(move, PlayCard)
            self.assertNotEqual(move.actual_card, move.declared_card)

    def test_queen_bluff_is_available_at_b100_against_a_king_penalty(self):
        state = position((c("7S"),), top=c("KS"), draw_penalty=4)
        move = self.decide(MixedGreedy(100, 100, 100), state)
        self.assertEqual(move.actual_card, c("7S"))
        self.assertEqual(move.declared_card.rank, "Q")
        self.assertIn(move.chosen_suit, "HDCS")

    def test_bluff_percentage_has_a_strict_boundary_and_ignores_n(self):
        state = position((c("8H"), c("10D")))
        for percentage in (0, 1, 10, 37, 50, 99, 100):
            samples = [(0.9999999999999999, True)] if percentage == 100 else [
                (percentage / 100, False)]
            if 0 < percentage < 100:
                samples.append((nextafter(percentage / 100, 0), True))
            for n in (0, 100):
                for sample, expected_bluff in samples:
                    with self.subTest(b=percentage, n=n, sample=sample):
                        move = self.decide(MixedGreedy(percentage, n, 0), state,
                                           rng=BoundaryRandom(sample))
                        self.assertIsInstance(move, PlayCard)
                        self.assertEqual(move.actual_card != move.declared_card, expected_bluff)

    def test_no_truth_percentage_has_a_strict_boundary_and_ignores_b(self):
        states = (position((c("8C"), c("10D"))),
                  position((c("8C"), c("10D")), top=c("AH"), skip_pending=True))
        for state in states:
            for percentage in (0, 1, 10, 37, 50, 99, 100):
                samples = [(0.9999999999999999, True)] if percentage == 100 else [
                    (percentage / 100, False)]
                if 0 < percentage < 100:
                    samples.append((nextafter(percentage / 100, 0), True))
                for b in (0, 100):
                    for sample, expected_bluff in samples:
                        with self.subTest(b=b, n=percentage, sample=sample,
                                          skip=state.skip_pending):
                            move = self.decide(MixedGreedy(b, percentage, 0), state,
                                               rng=BoundaryRandom(sample))
                            if expected_bluff:
                                self.assertIsInstance(move, PlayCard)
                                self.assertNotEqual(move.actual_card, move.declared_card)
                            else:
                                self.assertEqual(move, Skip() if state.skip_pending else Draw())

    def test_uncertain_challenge_percentage_has_a_strict_boundary(self):
        state = position((c("8C"), c("10D")), top=c("9H"),
                         pile=(c("8D"), c("9H")), phase="response")
        for percentage in (0, 1, 10, 20, 37, 50, 99, 100):
            samples = [(0.9999999999999999, True)] if percentage == 100 else [
                (percentage / 100, False)]
            if 0 < percentage < 100:
                samples.append((nextafter(percentage / 100, 0), True))
            for sample, challenge in samples:
                with self.subTest(c=percentage, sample=sample):
                    move = self.decide(MixedGreedy(100, 100, percentage), state,
                                       rng=BoundaryRandom(sample))
                    self.assertEqual(move, Challenge() if challenge else Accept())

    def test_bluffs_prefer_a_declared_identity_still_in_hand_first(self):
        state = position((c("7H"), c("AH"), c("8C")))
        for seed in range(16):
            move = self.decide(MixedGreedy(100, 100, 0), state, rng=Random(seed))
            self.assertIn(move.declared_card, state.hands[0])
            self.assertEqual(move.actual_card, c("8C"))

    def test_greedy_declaration_preferences_change_with_opponent_count(self):
        hand = (c("7H"), c("AH"), c("8H"), c("QH"))
        for bot in (HonestFirst(), MixedGreedy(0, 0, 0)):
            quiet = self.decide(bot, position(hand))
            self.assertEqual(quiet, PlayCard(c("8H"), c("8H")))
            pressured = self.decide(bot, position(hand, (c("8S"), c("10S"))))
            self.assertIn(pressured.declared_card.rank, ("7", "A"))
        for count in (2, 3):
            opponent = (c("8S"), c("10S"), c("JS"))[:count]
            state = position((c("8C"), c("10D")), opponent)
            move = self.decide(MixedGreedy(0, 100, 0), state)
            self.assertEqual(ordinary(move.declared_card), count == 3)
            if count == 2:
                self.assertIn(move.declared_card.rank, ("7", "A"))

    def test_greedy_bluffs_discard_ordinary_actual_cards(self):
        state = position((c("7C"), c("AD"), c("QD"), c("8C")),
                         top=c("KS"), draw_penalty=4)
        for bot in (HonestFirst(), MixedGreedy(100, 100, 0)):
            # Draw is disabled by an empty opponent, making bluffing necessary.
            returning = position(state.hands[0], (), top=c("KS"), draw_penalty=4)
            move = self.decide(bot, returning)
            self.assertEqual(move.actual_card, c("8C"))
            self.assertEqual(move.declared_card, c("7S"))

    def test_declared_queen_chooses_most_common_remaining_suit(self):
        hand = (c("QD"), c("8C"), c("10C"), c("KC"), c("8D"))
        state = position(hand)
        for bot in (HonestFirst(), MixedGreedy(0, 0, 0)):
            self.assertEqual(self.decide(bot, state), PlayCard(c("QD"), c("QD"), "C"))
        for seed in range(16):
            move = self.decide(MixedGreedy(100, 100, 0), state, rng=Random(seed))
            self.assertEqual(move.declared_card, c("QD"))
            counts = Counter(card.suit for card in hand if card != move.actual_card)
            self.assertEqual(counts[move.chosen_suit], max(counts.values()))

    def test_greedy_ties_use_the_supplied_rng(self):
        state = position((c("8H"), c("10H")))
        expected = {PlayCard(c("8H"), c("8H")), PlayCard(c("10H"), c("10H"))}
        for bot in (HonestFirst(), MixedGreedy(0, 0, 0)):
            observed = {self.decide(bot, state, rng=Random(seed)) for seed in range(64)}
            self.assertEqual(observed, expected)

    def test_random_legal_is_uniform_over_truth_bluff_and_draw(self):
        state = position((c("7S"), c("8C")), top=c("KS"), draw_penalty=4)
        legal = tuple(MoveGenerator(state))
        self.assertEqual(len(legal), 35)  # Draw, two seven plays, and 32 queen declarations/suits.
        rng, bot = Random(91823), RandomLegal()
        counts = Counter(self.decide(bot, state, rng=rng) for _ in range(3500))
        self.assertEqual(set(counts), set(legal))
        for count in counts.values():
            self.assertGreater(count, 60)
            self.assertLess(count, 140)

    def test_random_legal_is_uniform_after_the_return_filter(self):
        state = position((c("8C"), c("10D")), (), top=c("KS"), draw_penalty=4)
        expected = {PlayCard(card, c("7S")) for card in state.hands[0]}
        rng, bot = Random(519), RandomLegal()
        counts = Counter(self.decide(bot, state, rng=rng) for _ in range(2000))
        self.assertEqual(set(counts), expected)
        for count in counts.values():
            self.assertGreater(count, 880)
            self.assertLess(count, 1120)

    def test_repeatable_legal_outputs_preserve_inputs_and_global_random(self):
        states = (
            position((c("8H"), c("10D"), c("QD"))),
            position((c("8C"), c("10D")), top=c("9H"),
                     pile=(c("8D"), c("9H")), phase="response"),
        )
        global_before = random.getstate()
        for state in states:
            for bot in policies():
                for seed in range(8):
                    first = self.decide(bot, state, rng=Random(seed))
                    self.assertEqual(first, self.decide(bot, state, rng=Random(seed)))
                swapped = replace(state, hands=state.hands[::-1], turn=1,
                                  provisional_winner=None)
                self.decide(bot, swapped)
        self.assertEqual(random.getstate(), global_before)


if __name__ == "__main__":
    unittest.main()
