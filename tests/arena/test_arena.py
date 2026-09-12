"""Blind contract tests authored without reading or running arena/game/bot code.

Sources: docs/arena.md, RULES.md, and clarification-rules.md only.
Run from the repository root: python3 -B -m unittest tests.arena.test_arena
"""

from contextlib import closing
import csv
import itertools
import json
import math
import os
from pathlib import Path
import random
import sqlite3
import subprocess
import sys
import tempfile
import unittest

from src.engines import arena


ROOT = Path(__file__).resolve().parents[2]
FIELDS = ("rank", "name", "elo", "games", "wins", "draws", "losses", "points", "score_rate")
GAME_FIELDS = ("id", "round_index", "seat0", "seat1", "seed", "bot_seed0", "bot_seed1", "winner", "reason", "decisions")


class ArenaContractTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="blind-arena-tests-")
        self.addCleanup(self.temp.cleanup)
        self.parent = Path(self.temp.name)

    def create(self, **kwargs):
        settings = dict(roster="basic", seed=173, rounds=3, max_decisions=1)
        settings.update(kwargs)
        return arena.create_run(self.parent, **settings)

    def games(self, path):
        with closing(sqlite3.connect((Path(path) / "arena.sqlite3").resolve().as_uri() + "?mode=ro", uri=True)) as db:
            db.row_factory = sqlite3.Row
            return [dict(row) for row in db.execute("SELECT " + ", ".join(GAME_FIELDS) + " FROM games ORDER BY id")]

    def dump(self, path):
        with closing(sqlite3.connect((Path(path) / "arena.sqlite3").resolve().as_uri() + "?mode=ro", uri=True)) as db:
            return tuple(db.iterdump())

    def standings(self, path):
        return [{field: row[field] for field in FIELDS} for row in arena.standings(path)]

    def cli(self, *args):
        env = dict(os.environ, PYTHONDONTWRITEBYTECODE="1")
        return subprocess.run([sys.executable, "-B", "-m", "src.engines.arena", *map(str, args)], cwd=ROOT, env=env, text=True, capture_output=True, timeout=60)

    def assert_csv_matches(self, path, rows):
        with (Path(path) / "leaderboard.csv").open(newline="", encoding="utf-8") as stream:
            exported = list(csv.DictReader(stream))
        self.assertEqual(len(exported), len(rows))
        self.assertEqual([row["name"] for row in exported], [row["name"] for row in rows])
        for actual, expected in zip(exported, rows):
            for field in ("rank", "games", "wins", "draws", "losses"):
                self.assertEqual(int(actual[field]), expected[field])
            for field in ("elo", "points"):
                self.assertAlmostEqual(float(actual[field]), expected[field], places=2)
            if expected["score_rate"] is not None:
                self.assertAlmostEqual(float(actual["score_rate"]), expected["score_rate"], places=2)

    def assert_ledger_accounts(self, path):
        rows = self.standings(path)
        ledger = self.games(path)
        by_name = {row["name"]: dict(games=0, wins=0, draws=0, losses=0, elo=1000.0) for row in rows}
        self.assertEqual([game["id"] for game in ledger], list(range(len(ledger))))
        self.assertEqual(len(ledger) % 2, 0)
        pair_seeds = []
        schedule_rounds = {}
        for offset in range(0, len(ledger), 2):
            first, second = ledger[offset:offset + 2]
            self.assertEqual(first["round_index"], second["round_index"])
            self.assertEqual((first["seat0"], first["seat1"]), (second["seat1"], second["seat0"]))
            self.assertEqual(first["seed"], second["seed"])
            self.assertEqual(first["bot_seed0"], second["bot_seed0"])
            self.assertEqual(first["bot_seed1"], second["bot_seed1"])
            pair_seeds.append(first["seed"])
            schedule_rounds.setdefault(first["round_index"], []).append((first["seat0"], first["seat1"]))
        self.assertEqual(len(set(pair_seeds)), len(pair_seeds))
        for pairs in schedule_rounds.values():
            participants = list(itertools.chain.from_iterable(pairs))
            self.assertEqual(len(set(participants)), len(participants))
        for game in ledger:
            self.assertIn(game["seat0"], by_name)
            self.assertIn(game["seat1"], by_name)
            self.assertNotEqual(game["seat0"], game["seat1"])
            self.assertIsInstance(game["decisions"], int)
            self.assertGreater(game["decisions"], 0)
            if game["reason"] == "max_decisions":
                self.assertIsNone(game["winner"])
                score = 0.5
            else:
                self.assertEqual(game["reason"], "win")
                self.assertIn(game["winner"], (0, 1))
                score = 1.0 if game["winner"] == 0 else 0.0
            a, b = by_name[game["seat0"]], by_name[game["seat1"]]
            # Independent replay of the mathematical rule, never arena.elo_update.
            delta = 20 * (score - 1 / (1 + 10 ** ((b["elo"] - a["elo"]) / 400)))
            a["elo"], b["elo"] = a["elo"] + delta, b["elo"] - delta
            for player, result in ((a, score), (b, 1 - score)):
                player["games"] += 1
                player["draws" if result == 0.5 else "wins" if result == 1 else "losses"] += 1
        for row in rows:
            expected = by_name[row["name"]]
            for field in ("games", "wins", "draws", "losses"):
                self.assertEqual(row[field], expected[field])
            self.assertAlmostEqual(row["elo"], expected["elo"], places=10)
            points = expected["wins"] + expected["draws"] / 2
            self.assertEqual(row["points"], points)
            if expected["games"]:
                self.assertAlmostEqual(row["score_rate"], points / expected["games"], places=12)
            else:
                self.assertIsNone(row["score_rate"])
        expected_order = sorted(rows, key=lambda row: (row["games"] == 0, -(row["score_rate"] or 0), -row["games"], -row["wins"], row["name"]))
        self.assertEqual(rows, expected_order)
        self.assertEqual([row["rank"] for row in rows], list(range(1, len(rows) + 1)))
        self.assertAlmostEqual(sum(row["elo"] for row in rows), 1000 * len(rows), places=8)
        self.assertEqual(sum(row["games"] for row in rows), 2 * len(ledger))
        self.assert_csv_matches(path, rows)
        return rows, ledger

    def test_rosters_preserve_distinct_parameter_names(self):
        basic = arena.baseline_roster("basic")
        all_bots = arena.baseline_roster()
        self.assertIsInstance(basic, dict)
        self.assertIsInstance(all_bots, dict)
        self.assertEqual(len(basic), 3)
        self.assertEqual(len(all_bots), 1333)
        self.assertTrue(all(isinstance(name, str) and name for name in all_bots))
        self.assertTrue(set(basic) <= set(all_bots))
        for roster in (basic, all_bots):
            self.assertEqual(sum("RandomLegal" in name for name in roster), 1)
            self.assertEqual(sum("HonestFirst" in name for name in roster), 1)
        mixed_names = [name for name in all_bots if "MixedGreedy" in name]
        self.assertEqual(len(mixed_names), 1331)
        self.assertTrue(all(name != "MixedGreedy" for name in mixed_names))
        with self.assertRaises(ValueError):
            arena.baseline_roster("unknown")

    def test_elo_known_values_and_simultaneous_updates(self):
        for score, expected in ((1, (1010, 990)), (0, (990, 1010)), (0.5, (1000, 1000))):
            self.assertEqual(arena.elo_update(1000, 1000, score), expected)
        a, b = arena.elo_update(1200, 1000, 1)
        self.assertAlmostEqual(a, 1204.8050614670408, places=10)
        self.assertAlmostEqual(b, 995.1949385329592, places=10)
        self.assertEqual(a + b, 2200)
        self.assertEqual(arena.elo_update(1000, 1000, 1, k=40), (1020, 980))
        for ra, rb in ((-500, 900), (1333.3, 1234.5), (1e6, -1e6)):
            for score in (0, 0.5, 1):
                a, b = arena.elo_update(ra, rb, score)
                reverse = arena.elo_update(rb, ra, 1 - score)
                self.assertAlmostEqual(a, reverse[1], places=9)
                self.assertAlmostEqual(b, reverse[0], places=9)
                self.assertAlmostEqual(a + b, ra + rb, places=9)

    def test_elo_extreme_finite_differences(self):
        self.assertEqual(arena.elo_update(1e6, -1e6, 0), (999980, -999980))
        self.assertEqual(arena.elo_update(-1e6, 1e6, 1), (-999980, 999980))
        for ra, rb in ((sys.float_info.max, -sys.float_info.max), (-sys.float_info.max, sys.float_info.max)):
            for score in (0, 0.5, 1):
                result = arena.elo_update(ra, rb, score)
                self.assertTrue(all(math.isfinite(value) for value in result))
                self.assertEqual(result, (ra, rb))

    def test_elo_rejects_invalid_inputs(self):
        for value in (True, False, None, "1000", complex(1, 1), math.nan, math.inf, -math.inf):
            for args in ((value, 1000, 1), (1000, value, 1)):
                with self.subTest(args=args), self.assertRaises(ValueError):
                    arena.elo_update(*args)
        for value in (-1, 0.1, 2, None, "1", math.nan, math.inf):
            with self.subTest(score=value), self.assertRaises(ValueError):
                arena.elo_update(1000, 1000, value)
        for value in (0, -1, None, "20", complex(1, 1), math.nan, math.inf):
            with self.subTest(k=value), self.assertRaises(ValueError):
                arena.elo_update(1000, 1000, 1, k=value)

    def test_circle_cycles_have_every_opponent_once_and_balanced_byes(self):
        for count in (2, 3, 4, 5, 8, 9):
            names = [f"entrant-{index}" for index in range(count)]
            cycle_length = count if count % 2 else count - 1
            for cycle in range(2):
                opponents, byes = [], {name: 0 for name in names}
                for round_index in range(cycle * cycle_length, (cycle + 1) * cycle_length):
                    pairs = arena.round_pairs(names, -917, round_index)
                    self.assertIsInstance(pairs, tuple)
                    self.assertTrue(all(isinstance(pair, tuple) and len(pair) == 2 for pair in pairs))
                    self.assertEqual(len(pairs), count // 2)
                    present = list(itertools.chain.from_iterable(pairs))
                    self.assertEqual(len(set(present)), len(present))
                    self.assertTrue(set(present) <= set(names))
                    opponents.extend(frozenset(pair) for pair in pairs)
                    for name in set(names) - set(present):
                        byes[name] += 1
                self.assertEqual(len(opponents), count * (count - 1) // 2)
                self.assertEqual(set(opponents), {frozenset(pair) for pair in itertools.combinations(names, 2)})
                self.assertEqual(set(byes.values()), {count % 2})

    def test_schedule_is_stable_and_does_not_touch_inputs_or_global_rng(self):
        names = ["δ", "A", "B", "C", "D"]
        original = list(names)
        random.seed(919)
        before = random.getstate()
        expected = [arena.round_pairs(names, 37, index) for index in range(15)]
        self.assertEqual(random.getstate(), before)
        self.assertEqual(names, original)
        self.assertEqual(expected, [arena.round_pairs(names, 37, index) for index in range(15)])
        variants = {repr([arena.round_pairs(names, seed, index) for index in range(5)]) for seed in range(6)}
        self.assertGreater(len(variants), 1)
        script = "import json; from src.engines.arena import round_pairs; print(json.dumps([round_pairs(['δ', 'A', 'B', 'C', 'D'], 37, i) for i in range(15)]))"
        for hash_seed in ("11", "123"):
            result = subprocess.run([sys.executable, "-B", "-c", script], cwd=ROOT, env=dict(os.environ, PYTHONHASHSEED=hash_seed), text=True, capture_output=True, timeout=30)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout), json.loads(json.dumps(expected)))

    def test_schedule_validation(self):
        for names in ([], ["A"], ["A", "A"], ["A", ""], ["A", None], ["A", 1]):
            with self.subTest(names=names), self.assertRaises(ValueError):
                arena.round_pairs(names, 0, 0)
        for seed in (True, 1.5, "0", None):
            with self.subTest(seed=seed), self.assertRaises(ValueError):
                arena.round_pairs(["A", "B"], seed, 0)
        for index in (-1, True, 0.5, "0", None):
            with self.subTest(index=index), self.assertRaises(ValueError):
                arena.round_pairs(["A", "B"], 0, index)

    def test_all_roster_fifty_round_game_counts(self):
        names = list(arena.baseline_roster())
        counts = dict.fromkeys(names, 0)
        opponents = set()
        for index in range(50):
            pairs = arena.round_pairs(names, 0, index)
            self.assertEqual(len(pairs), 666)
            present = list(itertools.chain.from_iterable(pairs))
            self.assertEqual(len(set(present)), 1332)
            for pair in pairs:
                key = frozenset(pair)
                self.assertNotIn(key, opponents)
                opponents.add(key)
                for name in pair:
                    counts[name] += 2
        self.assertEqual(sum(counts.values()) // 2, 66600)
        self.assertEqual(sorted(counts.values()), [98] * 50 + [100] * 1283)

    def test_draw_ledger_complete_basic_cycle_and_csv(self):
        path = self.create(rounds=75)
        self.assertIsInstance(path, Path)
        returned = arena.run_arena(path, workers=1)
        rows, ledger = self.assert_ledger_accounts(path)
        self.assertEqual([{key: row[key] for key in FIELDS} for row in returned], rows)
        self.assertEqual(len(ledger), 150)
        self.assertEqual(set(game["round_index"] for game in ledger), set(range(75)))
        self.assertTrue(all(game["decisions"] == 1 and game["reason"] == "max_decisions" for game in ledger))
        for row in rows:
            self.assertEqual((row["games"], row["draws"], row["points"], row["elo"]), (100, 100, 50, 1000))
            self.assertEqual(sum(game["seat0"] == row["name"] for game in ledger), 50)
            self.assertEqual(sum(game["seat1"] == row["name"] for game in ledger), 50)
        original = self.dump(path)
        arena.run_arena(path, workers=2)
        self.assertEqual(self.dump(path), original)

    def test_all_entrants_exported_and_unplayed_rank_last(self):
        path = self.create(roster="all", rounds=1)
        initial = self.standings(path)
        self.assertEqual(len(initial), 1333)
        self.assertTrue(all(row["games"] == 0 and row["elo"] == 1000 and row["score_rate"] is None for row in initial))
        self.assertEqual([row["name"] for row in initial], sorted(row["name"] for row in initial))
        arena.run_arena(path, workers=1, max_pairs=1)
        rows, ledger = self.assert_ledger_accounts(path)
        self.assertEqual(len(ledger), 2)
        self.assertEqual([row["games"] for row in rows], [2, 2] + [0] * 1331)
        self.assertEqual([row["score_rate"] for row in rows[:2]], [0.5, 0.5])

    def test_batched_resume_matches_uninterrupted_across_workers_with_real_wins(self):
        continuous = self.create(seed=20260913, rounds=9, max_decisions=1000)
        resumed = self.create(seed=20260913, rounds=9, max_decisions=1000)
        arena.run_arena(continuous, workers=1)
        arena.run_arena(resumed, workers=2, max_pairs=0)
        self.assertEqual(self.games(resumed), [])
        arena.run_arena(resumed, workers=1, max_pairs=2)
        prefix = self.games(resumed)
        self.assertEqual(len(prefix), 4)
        self.assertEqual(prefix, self.games(continuous)[:4])
        arena.run_arena(resumed, workers=2)
        rows, ledger = self.assert_ledger_accounts(resumed)
        self.assertEqual(ledger, self.games(continuous))
        self.assertEqual(rows, self.standings(continuous))
        self.assertEqual(len(ledger), 18)
        self.assertGreater(sum(game["reason"] == "win" for game in ledger), 0)
        self.assertTrue(any(abs(row["elo"] - 1000) > 1e-6 for row in rows))

    def test_extension_means_total_round_target(self):
        path = self.create(rounds=3)
        reference = self.create(rounds=6)
        arena.run_arena(path, workers=1)
        prefix = self.games(path)
        arena.run_arena(path, workers=2, rounds=6)
        arena.run_arena(reference, workers=1)
        self.assertEqual(len(self.games(path)), 12)
        self.assertEqual(self.games(path)[:6], prefix)
        self.assertEqual(self.games(path), self.games(reference))
        self.assertEqual(self.standings(path), self.standings(reference))

    def test_invalid_creation_never_creates_parent_or_run(self):
        bad_settings = [dict(roster="invalid"), dict(seed=True), dict(seed=1.5), dict(seed="0")]
        for field in ("rounds", "max_decisions"):
            bad_settings.extend({field: value} for value in (0, -1, True, 1.5, "2", None))
        for index, settings in enumerate(bad_settings):
            parent = self.parent / f"invalid-{index}"
            with self.subTest(settings=settings), self.assertRaises((ValueError, RuntimeError)):
                arena.create_run(parent, **settings)
            self.assertFalse(parent.exists())

    def test_invalid_resume_never_changes_database(self):
        path = self.create(rounds=3)
        before = self.dump(path)
        bad_settings = []
        for field in ("workers", "rounds"):
            bad_settings.extend({field: value} for value in (0, -1, True, 1.5, "2"))
        bad_settings.extend(dict(max_pairs=value) for value in (-1, True, 1.5, "2"))
        bad_settings.append(dict(rounds=2))
        for settings in bad_settings:
            with self.subTest(settings=settings), self.assertRaises((ValueError, RuntimeError)):
                arena.run_arena(path, **settings)
            self.assertEqual(self.dump(path), before)
        missing = self.parent / "absent-run"
        with self.assertRaises((ValueError, RuntimeError)):
            arena.run_arena(missing, workers=1)
        self.assertFalse(missing.exists())

    def test_status_reads_database_even_when_csv_is_stale(self):
        path = self.create()
        arena.run_arena(path, workers=1, max_pairs=1)
        expected = self.standings(path)
        csv_path = path / "leaderboard.csv"
        csv_path.write_text("stale export\n", encoding="utf-8")
        before = self.dump(path)
        self.assertEqual(self.standings(path), expected)
        result = self.cli("--status", path, "--top", 1)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(expected[0]["name"], result.stdout)
        self.assertEqual(self.dump(path), before)
        self.assertEqual(csv_path.read_text(encoding="utf-8"), "stale export\n")

    def test_cli_rejects_invalid_options_before_mutation(self):
        path = self.create()
        before = self.dump(path)
        bad_new = self.parent / "cli-invalid"
        result = self.cli("--runs-dir", bad_new, "--workers", "0")
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(bad_new.exists())
        for args in (("--resume", path, "--status", path), ("--resume", path, "--seed", "19"), ("--resume", path, "--max-decisions", "2"), ("--resume", path, "--roster", "all"), ("--resume", path, "--rounds", "2")):
            with self.subTest(args=args):
                result = self.cli(*args)
                self.assertNotEqual(result.returncode, 0)
                self.assertTrue((result.stderr + result.stdout).strip())
                self.assertEqual(self.dump(path), before)


if __name__ == "__main__":
    unittest.main()
