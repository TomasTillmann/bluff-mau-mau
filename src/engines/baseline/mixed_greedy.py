"""A greedy baseline with independent bluff, no-truth bluff, and challenge rates."""

from dataclasses import dataclass
from itertools import product
from random import Random

from bluff_mau_mau import Accept, Challenge, Move
from ..observation import Observation
from .greedy import greedy_turn
from .tactics import candidates


@dataclass(frozen=True, slots=True)
class MixedGreedy:
    bluff: int = 10
    no_truth_bluff: int = 10
    challenge: int = 20

    def __post_init__(self):
        if any(type(value) is not int or not 0 <= value <= 100
               for value in (self.bluff, self.no_truth_bluff, self.challenge)):
            raise ValueError("Bot percentages must be integers from 0 to 100")

    @property
    def name(self) -> str:
        return f"MixedGreedy[B{self.bluff}-N{self.no_truth_bluff}-C{self.challenge}]"

    def __call__(self, observation: Observation, legal_moves: tuple[Move, ...], rng: Random) -> Move:
        moves = candidates(observation, legal_moves)
        if len(moves) == 1:
            return moves[0]
        if observation.phase == "response":
            response = Challenge if rng.random() < self.challenge / 100 else Accept
            return next(move for move in moves if isinstance(move, response))
        return greedy_turn(observation, moves, rng, self.bluff, self.no_truth_bluff)


def mixed_grid():
    """Enumerate B, N, C in ten-point steps, with C varying fastest."""
    return (MixedGreedy(*values) for values in product(range(0, 101, 10), repeat=3))
