"""Blind public-contract check: evaluation opponents accepts any mapping."""
import unittest
from collections import UserDict
from types import MappingProxyType

from src.engines.matches import evaluate_grid


class FirstLegal:
    name = 'mapping-candidate'

    def __call__(self, observation, legal_moves, rng):
        return legal_moves[0]


class MappingRosterContractTests(unittest.TestCase):
    def test_non_dict_opponent_mappings_match_dictionary_results(self):
        opponents = {'reference-a': FirstLegal(), 'reference-b': FirstLegal()}
        settings = dict(candidates=[FirstLegal()], bot_seeds=(17, 23), max_decisions=1)
        expected = evaluate_grid([11], opponents=opponents, **settings)
        self.assertEqual(set(expected), {'mapping-candidate'})
        self.assertEqual(expected['mapping-candidate']['games'], 4)
        for mapping in (UserDict(opponents), MappingProxyType(opponents)):
            with self.subTest(mapping_type=type(mapping).__name__):
                actual = evaluate_grid([11], opponents=mapping, **settings)
                self.assertEqual(actual, expected)


if __name__ == '__main__':
    unittest.main()
