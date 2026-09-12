"""Blind grid contract tests; no tournament or implementation inspection."""

from itertools import product
import unittest

from src.engines.baseline import MixedGreedy, mixed_grid


class GridTests(unittest.TestCase):
    def test_full_grid_has_exact_unique_configurations_in_lexicographic_order(self):
        grid = list(mixed_grid())
        expected = list(product(range(0, 101, 10), repeat=3))
        self.assertEqual(len(grid), 1331)
        self.assertTrue(all(isinstance(bot, MixedGreedy) for bot in grid))
        self.assertTrue(all(callable(bot) for bot in grid))
        actual = [(bot.bluff, bot.no_truth_bluff, bot.challenge) for bot in grid]
        self.assertEqual(actual, expected)
        self.assertEqual(len(set(actual)), 1331)
        self.assertEqual(actual[0], (0, 0, 0))
        self.assertEqual(actual[10], (0, 0, 100))
        self.assertEqual(actual[11], (0, 10, 0))
        self.assertEqual(actual[121], (10, 0, 0))
        self.assertEqual(actual[-1], (100, 100, 100))
        names = [bot.name for bot in grid]
        self.assertEqual(names, ["MixedGreedy[B%d-N%d-C%d]" % values for values in expected])
        self.assertEqual(len(set(names)), 1331)

    def test_grid_is_repeatable_and_contains_the_default_configuration_once(self):
        first, second = list(mixed_grid()), list(mixed_grid())
        for grid in (first, second):
            self.assertEqual(sum(bot.name == MixedGreedy().name for bot in grid), 1)
        self.assertEqual([bot.name for bot in first], [bot.name for bot in second])
        self.assertEqual([(bot.bluff, bot.no_truth_bluff, bot.challenge) for bot in first],
                         [(bot.bluff, bot.no_truth_bluff, bot.challenge) for bot in second])


if __name__ == "__main__":
    unittest.main()
