"""Private player knowledge and immutable input shared by engine policies."""

from collections.abc import Callable
from dataclasses import dataclass
from random import Random

from bluff_mau_mau import Card, Challenge, GameState, Move, PlayCard


PileKnowledge = tuple[frozenset[Card], frozenset[Card]]
EMPTY_KNOWLEDGE: PileKnowledge = (frozenset(), frozenset())


@dataclass(frozen=True, slots=True)
class Observation:
    player: int
    hand: tuple[Card, ...]
    opponent_count: int
    top: Card
    chosen_suit: str | None
    phase: str
    draw_penalty: int
    skip_pending: bool
    provisional_winner: int | None
    deck_count: int
    pile_count: int
    known_pile_cards: frozenset[Card] = frozenset()

    def __post_init__(self):
        object.__setattr__(self, "hand", tuple(self.hand))
        object.__setattr__(self, "known_pile_cards", frozenset(self.known_pile_cards))


Bot = Callable[[Observation, tuple[Move, ...], Random], Move]


def _knowledge_pair(knowledge) -> PileKnowledge:
    try:
        first, second = knowledge
        pair = (frozenset(first), frozenset(second))
    except (TypeError, ValueError) as error:
        raise ValueError("Knowledge needs two sets of actual cards") from error
    if any(type(card) is not Card for known in pair for card in known):
        raise ValueError("Knowledge needs concrete Card identities")
    return pair


def new_knowledge(starting_card: Card) -> PileKnowledge:
    """Both players see the starting discard when a new game is dealt."""
    if type(starting_card) is not Card:
        raise ValueError("The starting identity must be a Card")
    known = frozenset((starting_card,))
    return known, known


def advance_knowledge(before: GameState, move: Move, after: GameState,
                      knowledge: PileKnowledge) -> PileKnowledge:
    """Track one genuine Play transition without revealing accepted hidden cards.

    The trusted runner has already validated/applied the move. Intersecting only
    previously known or newly revealed identities with the remaining pile removes
    recycled cards; it never adds knowledge from the hidden pile itself.
    """
    known = list(_knowledge_pair(knowledge))
    if isinstance(move, PlayCard):
        known[before.turn] = known[before.turn] | {move.actual_card}
    elif isinstance(move, Challenge):
        revealed = {before.pile[-1]}
        known = [cards | revealed for cards in known]
    remaining = frozenset(after.pile)
    return known[0] & remaining, known[1] & remaining


def observe(state: GameState, knowledge: PileKnowledge = EMPTY_KNOWLEDGE) -> Observation:
    """Project a valid game state and coherent history for the next actor only."""
    known = _knowledge_pair(knowledge)
    return Observation(
        player=state.turn, hand=state.hands[state.turn],
        opponent_count=len(state.hands[1 - state.turn]), top=state.top,
        chosen_suit=state.chosen_suit, phase=state.phase,
        draw_penalty=state.draw_penalty, skip_pending=state.skip_pending,
        provisional_winner=state.provisional_winner,
        deck_count=len(state.deck), pile_count=len(state.pile),
        known_pile_cards=known[state.turn],
    )
