"""Uniform randomness after the small baseline tactical layer."""

from random import Random

from bluff_mau_mau import Move
from ..observation import Observation
from .tactics import candidates


class RandomLegal:
    name = "RandomLegal[uniform]"

    def __call__(self, observation: Observation, legal_moves: tuple[Move, ...], rng: Random) -> Move:
        moves = candidates(observation, legal_moves)
        return moves[0] if len(moves) == 1 else rng.choice(moves)
