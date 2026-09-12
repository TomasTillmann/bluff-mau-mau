"""Prefer truthful plays and accept uncertain declarations."""

from random import Random

from bluff_mau_mau import Accept, Move
from ..observation import Observation
from .greedy import greedy_turn
from .tactics import candidates


class HonestFirst:
    name = "HonestFirst[B0-N0-C0]"

    def __call__(self, observation: Observation, legal_moves: tuple[Move, ...], rng: Random) -> Move:
        moves = candidates(observation, legal_moves)
        if len(moves) == 1:
            return moves[0]
        if observation.phase == "response":
            return next(move for move in moves if isinstance(move, Accept))
        return greedy_turn(observation, moves, rng)
