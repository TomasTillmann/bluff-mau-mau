"""Blind runner/evaluation tests using independent scripted policies."""
import json
import random
import subprocess
import sys
import unittest
from dataclasses import replace
from random import Random

from bluff_mau_mau import Card, CARDS, GameState, NewGame, MoveGenerator, Play, PlayCard, Draw, Skip, Accept, Challenge
from src.engines import new_knowledge, observe
from src.engines.baseline import RandomLegal
from src.engines.matches import run_match, round_robin, evaluate_grid


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


class Spy:
    def __init__(self, name='spy', moves=(), burn=0):
        self.name, self.moves, self.burn = name, list(moves), burn
        self.calls, self.random_values = [], []

    def __call__(self, observation, legal_moves, rng):
        self.calls.append((observation, legal_moves))
        self.random_values.append(rng.random())
        for _ in range(self.burn):
            rng.random()
        return self.moves.pop(0) if self.moves else legal_moves[0]


def always_shed(observation, legal_moves, rng):
    if observation.phase == 'response':
        return Accept()
    if observation.draw_penalty or observation.skip_pending:
        return legal_moves[0]
    return next(move for move in legal_moves if isinstance(move, PlayCard)
                and move.declared_card.rank not in ('7', 'Q', 'A')
                and move.declared_card != card('KS'))


COUNTS = ('plays', 'bluffs', 'responses', 'challenges', 'correct_challenges')
ROW_KEYS = {'games', 'wins', 'losses', 'truncated', 'total_game_decisions', *COUNTS,
            'mean_game_length', 'bluff_rate', 'challenge_rate', 'challenge_success_rate'}


def result_snapshot(result):
    return (result.final_state, result.knowledge, result.decisions, result.winner,
            result.truncated, *(getattr(result, key) for key in COUNTS))


def aggregate(games):
    """Independent arithmetic oracle, with each game paired to the measured seat."""
    row = dict.fromkeys(('games', 'wins', 'losses', 'truncated', 'total_game_decisions', *COUNTS), 0)
    for result, seat in games:
        row['games'] += 1
        row['wins'] += result.winner == seat
        row['losses'] += result.winner == 1 - seat
        row['truncated'] += result.truncated
        row['total_game_decisions'] += result.decisions
        for key in COUNTS:
            row[key] += getattr(result, key)[seat]
    row['mean_game_length'] = row['total_game_decisions'] / row['games']
    for rate, numerator, denominator in (
        ('bluff_rate', 'bluffs', 'plays'), ('challenge_rate', 'challenges', 'responses'),
        ('challenge_success_rate', 'correct_challenges', 'challenges'),
    ):
        row[rate] = row[numerator] / row[denominator] if row[denominator] else None
    return row


class MatchContractTests(unittest.TestCase):
    def test_dispatch_uses_actor_complete_moves_and_personalized_knowledge(self):
        start = state(['8H', '10C'], ['9D', 'AC'], ['9H'])
        played = PlayCard(card('8H'), card('9D'))
        pending = Play(start, played)
        bots = (Spy(moves=[played]), Spy(moves=[Accept()]))
        known = new_knowledge(card('9H'))
        result = run_match(bots, initial_state=start, initial_knowledge=known, max_decisions=2)
        self.assertEqual(result.final_state, Play(pending, Accept()))
        self.assertEqual(result.decisions, 2)
        for player, expected_state in enumerate((start, pending)):
            obs, moves = bots[player].calls[0]
            self.assertEqual(obs.player, player)
            self.assertEqual(obs.hand, expected_state.hands[player])
            self.assertIsInstance(moves, tuple)
            self.assertEqual(moves, tuple(MoveGenerator(expected_state)))
            self.assertEqual(obs.known_pile_cards, known[player])
        # Seat 1 holds the declared 9D, but the generic runner must honor its custom Accept.
        self.assertIn(card('9D'), bots[1].calls[0][0].hand)
        self.assertEqual(result.knowledge, (frozenset(map(card, ['9H', '8H'])), known[1]))
        self.assertEqual(result.plays, (1, 0))
        self.assertEqual(result.bluffs, (1, 0))
        self.assertEqual(result.responses, (0, 1))
        self.assertEqual(result.challenges, (0, 0))
        self.assertEqual(result.correct_challenges, (0, 0))
        self.assertTrue(result.truncated)
        self.assertIsNone(result.winner)

    def test_empty_hand_still_chooses_response_and_acceptance_resolves_return(self):
        start = state([], ['7H'], ['9H'], turn=1, provisional_winner=0)
        played = PlayCard(card('7H'), card('7H'))
        bots = (Spy(moves=[Accept()]), Spy(moves=[played]))
        result = run_match(bots, initial_state=start, max_decisions=10)
        self.assertEqual(len(bots[0].calls), 1)
        self.assertEqual(bots[0].calls[0][0].hand, ())
        self.assertEqual(bots[0].calls[0][0].phase, 'response')
        self.assertEqual(bots[0].calls[0][1], (Accept(), Challenge()))
        self.assertEqual(result.final_state, Play(Play(start, played), Accept()))
        self.assertEqual(result.decisions, 2)
        self.assertEqual(result.winner, 1)
        self.assertFalse(result.truncated)
        self.assertEqual(result.plays, (0, 1))
        self.assertEqual(result.responses, (1, 0))
        self.assertEqual(result.bluffs, (0, 0))

    def test_challenge_counts_truth_and_bluff_and_resume_counts_only_new_actions(self):
        for actual, correct in (('9H', 0), ('10H', 1)):
            with self.subTest(actual=actual):
                start = state([actual, '10D'], ['8D', '7D'], ['9C'])
                played = PlayCard(card(actual), card('9H'))
                pending = Play(start, played)
                bots = (Spy(), Spy(moves=[Challenge()]))
                result = run_match(bots, initial_state=pending, max_decisions=1)
                self.assertEqual(result.final_state, Play(pending, Challenge()))
                self.assertEqual(result.plays, (0, 0))
                self.assertEqual(result.bluffs, (0, 0))
                self.assertEqual(result.responses, (0, 1))
                self.assertEqual(result.challenges, (0, 1))
                self.assertEqual(result.correct_challenges, (0, correct))
                self.assertEqual(result.knowledge, (frozenset({card(actual)}),) * 2)
                self.assertEqual(bots[0].calls, [])

    def test_draw_and_skip_are_not_plays_or_responses(self):
        for start, move in ((state(['8D'], ['AS'], ['9H']), Draw()),
                            (state(['8D'], ['AS'], ['AH'], skip_pending=True), Skip())):
            with self.subTest(move=move):
                result = run_match((Spy(moves=[move]), Spy()), initial_state=start, max_decisions=1)
                self.assertEqual(result.final_state, Play(start, move))
                for key in COUNTS:
                    self.assertEqual(getattr(result, key), (0, 0))

    def test_new_game_starting_knowledge_and_conservative_resume(self):
        start = NewGame(26)
        bots = (Spy(), Spy())
        fresh = run_match(bots, seed=26, max_decisions=1)
        obs, moves = bots[start.turn].calls[0]
        self.assertEqual(obs, observe(start, new_knowledge(start.top)))
        self.assertEqual(moves, tuple(MoveGenerator(start)))
        self.assertEqual(fresh.knowledge, new_knowledge(start.top))
        resumed = state(['8D', '7C'], ['AS'], ['9H', 'JC', '10D'])
        for supplied in (None, (frozenset({card('9H')}), frozenset({card('JC')}))):
            with self.subTest(knowledge=supplied):
                bots = (Spy(moves=[Draw()]), Spy())
                result = run_match(bots, initial_state=resumed, initial_knowledge=supplied, max_decisions=1)
                expected = supplied if supplied is not None else (frozenset(), frozenset())
                self.assertEqual(bots[0].calls[0][0].known_pile_cards, expected[0])
                self.assertEqual(result.knowledge, expected)

    def test_runner_recycling_drops_known_cards_and_reveals_top_atomically(self):
        start = state(['10C', '7D'], ['AS'], ['9H', 'JC', '8H'],
                      turn=1, phase='response', top=card('9D'))
        start = replace(start, deck=(), hands=(start.hands[0], start.hands[1] + start.deck))
        known = (frozenset(map(card, ['9H', '8H'])), frozenset(map(card, ['9H', 'JC'])))
        result = run_match((Spy(), Spy(moves=[Challenge()])), initial_state=start,
                           initial_knowledge=known, max_decisions=1)
        self.assertEqual(result.knowledge, (frozenset({card('8H')}),) * 2)
        self.assertEqual(result.final_state.pile, (card('8H'),))
        self.assertEqual(result.correct_challenges, (0, 1))

    def test_rngs_are_private_independent_and_use_the_requested_seeds(self):
        start = state(['10D', '7C'], ['AS'], ['9H', '8C', 'JD', '10S'])
        start = replace(start, deck=(), hands=(start.hands[0], start.hands[1] + start.deck))
        global_before = random.getstate()
        quiet = (Spy(), Spy())
        noisy = (Spy(burn=97), Spy(burn=31))
        a = run_match(quiet, initial_state=start, bot_seeds=(101, 202), max_decisions=2)
        b = run_match(noisy, initial_state=start, bot_seeds=(101, 202), max_decisions=2)
        different = run_match((Spy(burn=9), Spy(burn=13)), initial_state=start,
                              bot_seeds=(909, 808), max_decisions=2)
        self.assertEqual(a.final_state, b.final_state)
        self.assertEqual(a.final_state, different.final_state)
        for seat, seed in enumerate((101, 202)):
            self.assertEqual(quiet[seat].random_values, [Random(seed).random()])
            self.assertEqual(noisy[seat].random_values, quiet[seat].random_values)
        self.assertEqual(random.getstate(), global_before)
        # Dealing also uses only the game seed.
        c = run_match((Spy(), Spy()), seed=17, bot_seeds=(3, 4), max_decisions=2)
        d = run_match((Spy(burn=30), Spy(burn=30)), seed=17, bot_seeds=(5, 6), max_decisions=2)
        self.assertEqual(c.final_state, d.final_state)
        self.assertEqual(random.getstate(), global_before)

    def test_replay_is_deterministic_and_results_are_immutable(self):
        args = dict(seed=37, bot_seeds=(71, 83), max_decisions=60)
        first = run_match((RandomLegal(), RandomLegal()), **args)
        second = run_match((RandomLegal(), RandomLegal()), **args)
        self.assertEqual(result_snapshot(first), result_snapshot(second))
        self.assertEqual(first.winner, first.final_state.winner)
        self.assertEqual(first.truncated, first.winner is None)
        self.assertIsInstance(first.knowledge, tuple)
        self.assertTrue(all(isinstance(k, frozenset) for k in first.knowledge))
        for key in COUNTS:
            self.assertIsInstance(getattr(first, key), tuple)
            self.assertEqual(len(getattr(first, key)), 2)
        for key in ('final_state', 'knowledge', 'decisions', *COUNTS, 'winner', 'truncated'):
            with self.subTest(field=key), self.assertRaises((AttributeError, TypeError)):
                setattr(first, key, None)

    def test_terminal_state_dispatches_nothing_and_cutoff_does_not_invent_winner(self):
        terminal = state([], ['8D'], ['9H'], phase='finished', turn=0, winner=0)
        bots = (Spy(), Spy())
        result = run_match(bots, initial_state=terminal, max_decisions=5)
        self.assertEqual(result.final_state, terminal)
        self.assertEqual(result.decisions, 0)
        self.assertEqual(result.winner, 0)
        self.assertFalse(result.truncated)
        self.assertEqual([len(bot.calls) for bot in bots], [0, 0])
        for key in COUNTS:
            self.assertEqual(getattr(result, key), (0, 0))
        unfinished = run_match((Spy(), Spy()), max_decisions=1)
        self.assertEqual(unfinished.decisions, 1)
        self.assertTrue(unfinished.truncated)
        self.assertIsNone(unfinished.final_state.winner)

    def test_illegal_policy_outputs_are_rejected_by_play(self):
        start = state(['8H', '10C'], ['9D', 'AC'], ['9H'])
        pending = Play(start, PlayCard(card('8H'), card('9D')))
        for output in (Draw(), None, object(), PlayCard(card('9H'), card('9H'))):
            with self.subTest(output=output), self.assertRaises(ValueError):
                run_match((Spy(), Spy(moves=[output])), initial_state=pending, max_decisions=1)

    def test_match_validation(self):
        for bad_bots in ((), (Spy(),), (Spy(), Spy(), Spy()), (Spy(), None)):
            with self.subTest(bots=bad_bots), self.assertRaises(ValueError):
                run_match(bad_bots, max_decisions=1)
        settings = [{'seed': value} for value in (True, 1.5, '1', None)]
        settings += [{'bot_seeds': value} for value in ((), (0,), (0, 1, 2), (False, 1), (0, 1.5), (0, '1'))]
        settings += [{'max_decisions': value} for value in (0, -1, True, 1.5, '1')]
        for invalid in settings:
            with self.subTest(settings=invalid), self.assertRaises(ValueError):
                run_match((Spy(), Spy()), **({'max_decisions': 1} | invalid))


class EvaluationContractTests(unittest.TestCase):
    def test_round_robin_reuses_seed_iterable_and_exchanges_seats_for_each_pair(self):
        roster = {name: Spy(name) for name in ('alpha', 'beta', 'gamma')}
        rows = round_robin(roster, iter([3, 7]), max_decisions=1)
        self.assertEqual(set(rows), set(roster))
        initial_hands = [NewGame(seed).hands[1] for seed in (3, 7)]
        for name, bot in roster.items():
            row = rows[name]
            self.assertEqual(set(row), ROW_KEYS)
            self.assertEqual((row['games'], row['wins'], row['losses'], row['truncated']), (8, 0, 0, 8))
            self.assertEqual(row['total_game_decisions'], 8)
            self.assertEqual(row['mean_game_length'], 1)
            self.assertEqual(len(bot.calls), 4)
            self.assertEqual([obs.hand for obs, _ in bot.calls].count(initial_hands[0]), 2)
            self.assertEqual([obs.hand for obs, _ in bot.calls].count(initial_hands[1]), 2)
            self.assertTrue(all(obs.player == 1 for obs, _ in bot.calls))
            for key in COUNTS:
                self.assertEqual(row[key], 0)
            for key in ('bluff_rate', 'challenge_rate', 'challenge_success_rate'):
                self.assertIsNone(row[key])

    def test_roster_statistics_match_independent_seat_aggregation(self):
        roster = {'first': RandomLegal(), 'second': RandomLegal()}
        settings = dict(bot_seeds=(19, 23), max_decisions=40)
        expected = {name: [] for name in roster}
        for seed in (2, 11):
            for names in (('first', 'second'), ('second', 'first')):
                result = run_match(tuple(roster[name] for name in names), seed=seed, **settings)
                for seat, name in enumerate(names):
                    expected[name].append((result, seat))
        rows = round_robin(roster, (2, 11), **settings)
        self.assertEqual(rows, {name: aggregate(games) for name, games in expected.items()})

    def test_completed_roster_games_count_each_win_and_loss_once(self):
        roster = {'alpha': always_shed, 'beta': always_shed}
        games = {name: [] for name in roster}
        for names in (('alpha', 'beta'), ('beta', 'alpha')):
            result = run_match(tuple(roster[name] for name in names), seed=8, max_decisions=50)
            self.assertIsNotNone(result.winner)
            for seat, name in enumerate(names):
                games[name].append((result, seat))
        rows = round_robin(roster, [8], max_decisions=50)
        self.assertEqual(rows, {name: aggregate(results) for name, results in games.items()})
        self.assertEqual(sum(row['wins'] for row in rows.values()), 2)
        self.assertEqual(sum(row['losses'] for row in rows.values()), 2)
        self.assertTrue(all(row['truncated'] == 0 for row in rows.values()))

    def test_grid_candidates_face_only_fixed_opponents_in_both_seats(self):
        candidates = [Spy('candidate-a'), Spy('candidate-b')]
        opponents = {'reference-x': Spy(), 'reference-y': Spy()}
        rows = evaluate_grid(iter([4, 9]), candidates=iter(candidates), opponents=opponents, max_decisions=1)
        self.assertEqual(set(rows), {'candidate-a', 'candidate-b'})
        for candidate in candidates:
            row = rows[candidate.name]
            self.assertEqual(set(row), ROW_KEYS)
            self.assertEqual(row['games'], 8)
            self.assertEqual(row['truncated'], 8)
            self.assertEqual(row['total_game_decisions'], 8)
            self.assertEqual(len(candidate.calls), 4)
        self.assertEqual([len(bot.calls) for bot in opponents.values()], [4, 4])
        only = Spy('single')
        default_rows = evaluate_grid([4], candidates=[only], max_decisions=1)
        self.assertEqual(set(default_rows), {'single'})
        self.assertEqual(default_rows['single']['games'], 4)
        self.assertEqual(len(only.calls), 2)

    def test_grid_statistics_aggregate_only_candidate_games(self):
        candidate = RandomLegal()
        opponents = {'reference': RandomLegal()}
        settings = dict(bot_seeds=(41, 43), max_decisions=30)
        games = []
        for seed in (5, 12):
            for seat in (0, 1):
                bots = (candidate, opponents['reference']) if seat == 0 else (opponents['reference'], candidate)
                games.append((run_match(bots, seed=seed, **settings), seat))
        rows = evaluate_grid([5, 12], candidates=[candidate], opponents=opponents, **settings)
        self.assertEqual(rows, {candidate.name: aggregate(games)})

    def test_evaluation_validation(self):
        for roster in ({}, {'one': Spy()}, {'one': Spy(), 'two': None}):
            with self.subTest(roster=roster), self.assertRaises(ValueError):
                round_robin(roster, [0], max_decisions=1)
        for seeds in ([], [True], [1.5], ['0']):
            for function in (
                lambda: round_robin({'a': Spy(), 'b': Spy()}, seeds, max_decisions=1),
                lambda: evaluate_grid(seeds, candidates=[Spy('a')], opponents={'b': Spy()}, max_decisions=1),
            ):
                with self.subTest(seeds=seeds), self.assertRaises(ValueError):
                    function()
        for candidates, opponents in (([], {'b': Spy()}), ([Spy('a')], {}),
                                      ([Spy('a'), Spy('a')], {'b': Spy()}),
                                      ([Spy('a')], {'b': None})):
            with self.subTest(candidates=candidates, opponents=opponents), self.assertRaises(ValueError):
                evaluate_grid([0], candidates=candidates, opponents=opponents, max_decisions=1)
        for invalid in ({'bot_seeds': (False, 1)}, {'bot_seeds': (0,)}, {'max_decisions': 0},
                        {'max_decisions': True}, {'max_decisions': 1.5}):
            settings = {'max_decisions': 1} | invalid
            with self.subTest(settings=invalid, api='round_robin'), self.assertRaises(ValueError):
                round_robin({'a': Spy(), 'b': Spy()}, [0], **settings)
            with self.subTest(settings=invalid, api='evaluate_grid'), self.assertRaises(ValueError):
                evaluate_grid([0], candidates=[Spy('a')], opponents={'b': Spy()}, **settings)

    def test_cli_defaults_print_repeatable_json_for_three_named_bots(self):
        command = [sys.executable, '-B', '-m', 'src.engines.baseline', '--deals', '1',
                   '--max-decisions', '4', '--seed', '9', '--bot-seed', '13']
        first = subprocess.run(command, capture_output=True, text=True, check=True, timeout=30)
        second = subprocess.run(command, capture_output=True, text=True, check=True, timeout=30)
        self.assertEqual(first.stdout, second.stdout)
        parsed = json.loads(first.stdout)
        self.assertIsInstance(parsed, dict)
        self.assertEqual(set(parsed), {'RandomLegal[uniform]', 'HonestFirst[B0-N0-C0]', 'MixedGreedy[B10-N10-C20]'})
        for row in parsed.values():
            self.assertEqual(set(row), ROW_KEYS)
            self.assertEqual(row['games'], 4)
            self.assertEqual(row['truncated'], 4)


if __name__ == '__main__':
    unittest.main()
