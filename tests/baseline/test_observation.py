"""Blind contract tests. Authored without reading the new engine implementation."""
import unittest
from random import Random
from dataclasses import replace

from bluff_mau_mau import Card, CARDS, GameState, PlayCard, Draw, Accept, Challenge, Play, MoveGenerator
from src.engines import Observation, PileKnowledge, new_knowledge, advance_knowledge, observe


def card(text):
    return Card(text[:-1], text[-1])


def state(hand0, hand1, pile, **changes):
    hand0, hand1, pile = tuple(map(card, hand0)), tuple(map(card, hand1)), tuple(map(card, pile))
    used = hand0 + hand1 + pile
    assert len(used) == len(set(used))
    values = dict(deck=tuple(c for c in CARDS if c not in used), pile=pile,
                  hands=(hand0, hand1), turn=0, top=pile[-1])
    values.update(changes)
    result = GameState(**values)
    MoveGenerator(result)
    return result


def empty_deck(s):
    return replace(s, deck=(), hands=(s.hands[0], s.hands[1] + s.deck))


FIELDS = ('player', 'hand', 'opponent_count', 'top', 'chosen_suit', 'phase',
          'draw_penalty', 'skip_pending', 'provisional_winner', 'deck_count',
          'pile_count', 'known_pile_cards', 'opening_card')


def visible(obs):
    return tuple(getattr(obs, name) for name in FIELDS)


class ObservationContractTests(unittest.TestCase):
    def test_opening_marker_survives_draws_and_clears_on_the_first_play(self):
        initial = state(['7S', '8C'], ['AD'], ['7H'], opening_card=True)
        self.assertTrue(observe(initial).opening_card)
        drawn = Play(initial, Draw())
        self.assertTrue(observe(drawn).opening_card)
        pending = Play(drawn, PlayCard(card('AD'), card('7S')))
        observation = observe(pending)
        self.assertFalse(observation.opening_card)
        self.assertEqual(observation.draw_penalty, 4)

    def test_starting_card_is_known_to_both_and_knowledge_is_immutable(self):
        start = card('9H')
        known = new_knowledge(start)
        self.assertEqual(known, (frozenset({start}), frozenset({start})))
        self.assertIsInstance(known, tuple)
        self.assertTrue(all(isinstance(part, frozenset) for part in known))

    def test_own_actual_is_added_but_acceptance_never_proves_a_declaration(self):
        for actual in ('8H', '9D'):
            with self.subTest(actual=actual):
                before = state([actual, '10C'], ['9C', 'AD'], ['9H'])
                known = new_knowledge(card('9H'))
                original = (before, known)
                move = PlayCard(card(actual), card('9D'))
                pending = Play(before, move)
                played = advance_knowledge(before, move, pending, known)
                self.assertEqual(played[0], frozenset({card('9H'), card(actual)}))
                self.assertEqual(played[1], frozenset({card('9H')}))
                after = Play(pending, Accept())
                accepted = advance_knowledge(pending, Accept(), after, played)
                self.assertEqual(accepted, played)
                self.assertEqual(observe(after, accepted).known_pile_cards, known[1])
                self.assertEqual((before, known), original)

    def test_challenge_reveals_actual_identity_for_truth_and_bluff(self):
        for actual in ('9D', '8H'):
            with self.subTest(actual=actual):
                before = state([actual, '10C'], ['9C', 'AD'], ['9H'])
                move = PlayCard(card(actual), card('9D'))
                pending = Play(before, move)
                known = advance_knowledge(before, move, pending, new_knowledge(card('9H')))
                after = Play(pending, Challenge())
                updated = advance_knowledge(pending, Challenge(), after, known)
                expected = frozenset({card('9H'), card(actual)})
                self.assertEqual(updated, (expected, expected))
                if actual != '9D':
                    self.assertNotIn(card('9D'), updated[0])
                    self.assertNotIn(card('9D'), updated[1])
                self.assertEqual(known[1], frozenset({card('9H')}))

    def test_recycling_forgets_all_older_cards_and_preserves_asymmetric_top(self):
        before = empty_deck(state(['10D', '7C'], ['AS'], ['9H', '8C', 'JD']))
        known = (frozenset(map(card, ['9H', 'JD'])), frozenset(map(card, ['9H', '8C'])))
        after = Play(before, Draw())
        updated = advance_knowledge(before, Draw(), after, known)
        self.assertEqual(after.pile, (card('JD'),))
        self.assertEqual(updated, (frozenset({card('JD')}), frozenset()))
        self.assertEqual(known[1], frozenset(map(card, ['9H', '8C'])))
        self.assertEqual(observe(after, updated).known_pile_cards, frozenset())

    def test_empty_hand_acceptance_recycles_without_revealing_the_hidden_top(self):
        before = empty_deck(state([], ['AS'], ['9H', 'JC', '8D'], turn=0,
                                  phase='response', top=card('7H'), draw_penalty=2,
                                  provisional_winner=0))
        known = (frozenset(map(card, ['9H', 'JC'])), frozenset(map(card, ['9H', '8D'])))
        after = Play(before, Accept())
        updated = advance_knowledge(before, Accept(), after, known)
        self.assertEqual(after.pile, (card('8D'),))
        self.assertEqual(updated, (frozenset(), frozenset({card('8D')})))
        self.assertNotIn(card('7H'), updated[0])
        self.assertNotIn(card('7H'), updated[1])

    def test_challenge_reveal_and_recycle_are_one_knowledge_transition(self):
        for actual in ('9D', '8H'):
            with self.subTest(actual=actual):
                before = empty_deck(state(['10C', '7D'], ['AS'], ['9H', 'JC', actual],
                                          turn=1, phase='response', top=card('9D')))
                known = (frozenset(map(card, ['9H', actual])), frozenset(map(card, ['9H', 'JC'])))
                after = Play(before, Challenge())
                updated = advance_knowledge(before, Challenge(), after, known)
                self.assertEqual(after.pile, (card(actual),))
                self.assertEqual(updated, (frozenset({card(actual)}),) * 2)
                self.assertNotIn(card('9H'), updated[0])
                self.assertNotIn(card('JC'), updated[1])

    def test_recycled_identity_returns_only_after_own_play_or_reveal(self):
        current = empty_deck(state(['10D', '7C'], ['AS'], ['9H', '8H', 'JD'], turn=1))
        known = (frozenset(map(card, ['9H', '8H'])), frozenset(map(card, ['9H', '8H', 'JD'])))
        after = Play(current, Draw())
        drawn = next(c for c in after.hands[1] if c not in current.hands[1])
        known = advance_knowledge(current, Draw(), after, known)
        self.assertNotIn(drawn, known[0])
        self.assertNotIn(drawn, known[1])
        current = after
        for move in (PlayCard(card('10D'), card('JD')), Accept(), PlayCard(drawn, card('JD'))):
            after = Play(current, move)
            known = advance_knowledge(current, move, after, known)
            current = after
        self.assertNotIn(drawn, known[0], 'An opponent hidden play must not restore old knowledge')
        self.assertIn(drawn, known[1], 'The player knows their newly played actual card')
        self.assertNotIn(drawn, observe(current, known).known_pile_cards)
        after = Play(current, Challenge())
        known = advance_knowledge(current, Challenge(), after, known)
        self.assertIn(drawn, known[0])
        self.assertIn(drawn, known[1])
        self.assertEqual(known, (frozenset({drawn}),) * 2)

    def test_snapshot_does_not_reconstruct_past_reveals(self):
        s = state(['8D', '7C'], ['AS'], ['9H', 'JC', '10D'])
        self.assertEqual(observe(s).known_pile_cards, frozenset())
        other = replace(s, turn=1)
        known = (frozenset({card('9H')}), frozenset({card('JC')}))
        self.assertEqual(observe(s, known).known_pile_cards, known[0])
        self.assertEqual(observe(other, known).known_pile_cards, known[1])

    def test_hidden_state_permutations_do_not_change_observation_or_moves(self):
        base = state(['7C', '8H'], ['AS', '10C'], ['9H', 'JC', '8D'], top=card('10H'))
        known = (frozenset({card('9H')}), frozenset())
        for phase in ('turn', 'response'):
            s = replace(base, phase=phase)
            swapped_hand = (s.deck[0],) + s.hands[1][1:]
            variants = [
                replace(s, deck=tuple(reversed(s.deck))),
                replace(s, rng_state=Random(982).getstate()),
                replace(s, hands=(s.hands[0], swapped_hand), deck=(s.hands[1][0],) + s.deck[1:]),
                replace(s, pile=s.pile[:-1] + (s.hands[1][0],),
                        hands=(s.hands[0], (s.pile[-1],) + s.hands[1][1:])),
                replace(s, pile=(s.pile[0], s.deck[0], s.pile[-1]), deck=(s.pile[1],) + s.deck[1:]),
            ]
            for variant in variants:
                with self.subTest(phase=phase, hidden_change=variants.index(variant)):
                    self.assertEqual(tuple(MoveGenerator(variant)), tuple(MoveGenerator(s)))
                    self.assertEqual(visible(observe(variant, known)), visible(observe(s, known)))
            richer_opponent = (known[0], frozenset(s.pile))
            self.assertEqual(visible(observe(s, richer_opponent)), visible(observe(s, known)))

    def test_observation_fields_and_nested_containers_are_immutable(self):
        s = state(['8D', '7C'], ['AS'], ['9H'], turn=1)
        obs = observe(s, new_knowledge(card('9H')))
        self.assertEqual(visible(obs), (1, s.hands[1], 2, card('9H'), None, 'turn',
                                       0, False, None, len(s.deck), 1, frozenset({card('9H')}), False))
        supplied = {card('9H')}
        values = dict(zip(FIELDS, visible(obs)))
        values['known_pile_cards'] = supplied
        copied = Observation(**values)
        supplied.clear()
        self.assertEqual(copied.known_pile_cards, frozenset({card('9H')}))
        self.assertIsInstance(copied.hand, tuple)
        self.assertIsInstance(copied.known_pile_cards, frozenset)
        for name in FIELDS:
            with self.subTest(field=name), self.assertRaises((AttributeError, TypeError)):
                setattr(copied, name, None)
        with self.assertRaises(TypeError):
            copied.hand[0] = card('7H')
        with self.assertRaises((AttributeError, TypeError)):
            copied.hand[0].rank = '7'
        for forbidden in ('hands', 'opponent_hand', 'deck', 'pile', 'seed', 'rng_state', 'rng'):
            self.assertFalse(hasattr(copied, forbidden), forbidden)


if __name__ == '__main__':
    unittest.main()
