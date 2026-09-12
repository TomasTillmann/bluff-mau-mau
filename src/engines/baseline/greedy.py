"""The coarse card preferences and family selection shared by both greedy bots."""

from random import Random

from bluff_mau_mau import Draw, Move, PlayCard, Skip, _contribution
from ..observation import Observation


def best_play(observation: Observation, plays: list[PlayCard], rng: Random) -> PlayCard:
    # ponytail: fixed priorities; replace with evaluation/search when comparisons justify it.
    def score(move):
        declared, actual = move.declared_card, move.actual_card
        attack = bool(_contribution(declared) or declared.rank == "A")
        ordinary_actual = not (_contribution(actual) or actual.rank in ("A", "Q"))
        suit_count = sum(card.suit == move.chosen_suit for card in observation.hand
                         if card != actual)
        return (
            actual != declared and declared in observation.hand,
            attack if observation.opponent_count <= 2 else not (attack or declared.rank == "Q"),
            ordinary_actual,
            suit_count,
        )

    scores = [score(move) for move in plays]
    best = max(scores)
    return rng.choice([move for move, value in zip(plays, scores) if value == best])


def greedy_turn(observation: Observation, moves: tuple[Move, ...], rng: Random,
                bluff: int = 0, no_truth_bluff: int = 0) -> Move:
    truthful = [move for move in moves if isinstance(move, PlayCard)
                and move.actual_card == move.declared_card]
    bluffs = [move for move in moves if isinstance(move, PlayCard)
              and move.actual_card != move.declared_card]
    passes = [move for move in moves if isinstance(move, (Draw, Skip))]
    if truthful:
        plays = bluffs if bluffs and bluff and rng.random() < bluff / 100 else truthful
    elif bluffs and (not passes or no_truth_bluff and rng.random() < no_truth_bluff / 100):
        plays = bluffs
    else:
        return passes[0]
    return best_play(observation, plays, rng)
