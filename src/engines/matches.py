"""Reproducible matches and comparisons with player-specific pile knowledge."""

import argparse
from collections.abc import Mapping
from dataclasses import dataclass
from itertools import combinations
import json
from random import Random

from bluff_mau_mau import Challenge, GameState, MoveGenerator, NewGame, Play, PlayCard
from .observation import (
    Bot, EMPTY_KNOWLEDGE, PileKnowledge, _knowledge_pair, advance_knowledge, new_knowledge, observe,
)


@dataclass(frozen=True, slots=True)
class MatchResult:
    final_state: GameState
    knowledge: PileKnowledge
    decisions: int
    plays: tuple[int, int]
    bluffs: tuple[int, int]
    responses: tuple[int, int]
    challenges: tuple[int, int]
    correct_challenges: tuple[int, int]

    @property
    def winner(self) -> int | None:
        return self.final_state.winner

    @property
    def truncated(self) -> bool:
        return self.winner is None


COUNTERS = ("plays", "bluffs", "responses", "challenges", "correct_challenges")


def _validate_settings(bot_seeds, max_decisions):
    if (not isinstance(bot_seeds, tuple) or len(bot_seeds) != 2
            or any(type(seed) is not int for seed in bot_seeds)):
        raise ValueError("Provide two integer bot seeds")
    if type(max_decisions) is not int or max_decisions < 1:
        raise ValueError("The decision limit must be a positive integer")


def _validate_seeds(seeds):
    try:
        seeds = tuple(seeds)
    except TypeError as error:
        raise ValueError("Provide at least one integer game seed") from error
    if not seeds or any(type(seed) is not int for seed in seeds):
        raise ValueError("Provide at least one integer game seed")
    return seeds


def _validate_roster(bots, minimum):
    if (not isinstance(bots, Mapping) or len(bots) < minimum
            or any(type(name) is not str or not name or not callable(bot)
                   for name, bot in bots.items())):
        raise ValueError(f"Provide at least {minimum} named callable bots")


def run_match(bots: tuple[Bot, Bot], *, seed: int = 0,
              bot_seeds: tuple[int, int] = (0, 1), max_decisions: int = 1000,
              initial_state: GameState | None = None,
              initial_knowledge: PileKnowledge | None = None) -> MatchResult:
    """Run policies without enforcing baseline tactics on custom engines.

    Game and two per-seat policy RNGs are independent. Resuming without knowledge
    is deliberately conservative: a snapshot cannot establish reveal history.
    Cutoffs remain unfinished and never award a winner.
    """
    _validate_settings(bot_seeds, max_decisions)
    if type(seed) is not int:
        raise ValueError("The game seed must be an integer")
    if (not isinstance(bots, tuple) or len(bots) != 2
            or any(not callable(bot) for bot in bots)):
        raise ValueError("Provide two callable bots")
    state = NewGame(seed) if initial_state is None else initial_state
    moves = tuple(MoveGenerator(state))
    knowledge = initial_knowledge
    if knowledge is None:
        knowledge = new_knowledge(state.top) if initial_state is None else EMPTY_KNOWLEDGE
    # Normalize both private sets even when a resumed game is already finished.
    knowledge = _knowledge_pair(knowledge)
    rngs = tuple(Random(bot_seed) for bot_seed in bot_seeds)
    counts = {field: [0, 0] for field in COUNTERS}
    decisions = 0
    while state.winner is None and decisions < max_decisions:
        player = state.turn
        move = bots[player](observe(state, knowledge), moves, rngs[player])
        result = Play(state, move)
        if state.phase == "response":
            counts["responses"][player] += 1
        if isinstance(move, PlayCard):
            counts["plays"][player] += 1
            counts["bluffs"][player] += move.actual_card != move.declared_card
        elif isinstance(move, Challenge):
            counts["challenges"][player] += 1
            counts["correct_challenges"][player] += state.pile[-1] != state.top
        knowledge = advance_knowledge(state, move, result, knowledge)
        state = result
        decisions += 1
        moves = tuple(MoveGenerator(state))
    return MatchResult(state, knowledge, decisions,
                       **{key: tuple(value) for key, value in counts.items()})


def _new_stats():
    return dict.fromkeys(("games", "wins", "losses", "truncated", "total_game_decisions", *COUNTERS), 0)


def _record(row, result, player):
    row["games"] += 1
    row["wins"] += result.winner == player
    row["losses"] += result.winner == 1 - player
    row["truncated"] += result.truncated
    row["total_game_decisions"] += result.decisions
    for field in COUNTERS:
        row[field] += getattr(result, field)[player]


def _rates(stats):
    for row in stats.values():
        row["mean_game_length"] = row["total_game_decisions"] / row["games"]
        for rate, numerator, denominator in (
                ("bluff_rate", "bluffs", "plays"),
                ("challenge_rate", "challenges", "responses"),
                ("challenge_success_rate", "correct_challenges", "challenges")):
            row[rate] = row[numerator] / row[denominator] if row[denominator] else None
    return stats


def round_robin(bots: Mapping[str, Bot], seeds, *, bot_seeds: tuple[int, int] = (0, 1),
                max_decisions: int = 1000) -> dict[str, dict]:
    """Every unordered pair plays the same deals in both seat assignments."""
    _validate_settings(bot_seeds, max_decisions)
    _validate_roster(bots, 2)
    seeds = _validate_seeds(seeds)
    stats = {name: _new_stats() for name in bots}
    for pair in combinations(bots, 2):
        for seed in seeds:
            for seats in (pair, pair[::-1]):
                result = run_match(tuple(bots[name] for name in seats), seed=seed,
                                   bot_seeds=bot_seeds, max_decisions=max_decisions)
                for player, name in enumerate(seats):
                    _record(stats[name], result, player)
    return _rates(stats)


def evaluate_grid(seeds, *, candidates=None, opponents=None,
                  bot_seeds: tuple[int, int] = (0, 1), max_decisions: int = 1000) -> dict[str, dict]:
    """Score named configurations against fixed opponents, with paired seats.

    This is evaluation only: callers choose disjoint screening/validation/test
    deals. It does not select a champion or run candidate-versus-candidate games.
    """
    from .baseline import HonestFirst, RandomLegal, mixed_grid
    _validate_settings(bot_seeds, max_decisions)
    seeds = _validate_seeds(seeds)
    if candidates is None:
        candidates = mixed_grid()
    if opponents is None:
        opponents = {bot.name: bot for bot in (RandomLegal(), HonestFirst())}
    _validate_roster(opponents, 1)
    candidates = tuple(candidates)
    names = [getattr(bot, "name", None) for bot in candidates]
    if (not candidates or any(type(name) is not str or not name for name in names)
            or len(set(names)) != len(names) or any(not callable(bot) for bot in candidates)):
        raise ValueError("Provide uniquely named callable candidates")
    stats = {name: _new_stats() for name in names}
    for candidate in candidates:
        for opponent in opponents.values():
            for seed in seeds:
                for seat in (0, 1):
                    bots = (candidate, opponent) if seat == 0 else (opponent, candidate)
                    result = run_match(bots, seed=seed, bot_seeds=bot_seeds,
                                       max_decisions=max_decisions)
                    _record(stats[candidate.name], result, seat)
    return _rates(stats)


def main():
    from .baseline import HonestFirst, MixedGreedy, RandomLegal
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--deals", type=int, default=20)
    parser.add_argument("--seed", type=int, default=0)
    parser.add_argument("--bot-seed", type=int, default=10000)
    parser.add_argument("--max-decisions", type=int, default=1000)
    parser.add_argument("--grid", action="store_true", help="evaluate all 1331 configurations")
    args = parser.parse_args()
    if args.deals < 1 or args.max_decisions < 1:
        parser.error("--deals and --max-decisions must be positive")
    settings = dict(bot_seeds=(args.bot_seed, args.bot_seed + 1), max_decisions=args.max_decisions)
    seeds = range(args.seed, args.seed + args.deals)
    if args.grid:
        result = evaluate_grid(seeds, **settings)
    else:
        roster = {bot.name: bot for bot in (RandomLegal(), HonestFirst(), MixedGreedy())}
        result = round_robin(roster, seeds, **settings)
    print(json.dumps(result, indent=2))
