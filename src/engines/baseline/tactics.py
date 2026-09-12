"""Small shared baseline tactics, using only the acting player's information."""

from bluff_mau_mau import Accept, Challenge, Move, PlayCard, _contribution, _declarations
from ..observation import Observation


def candidates(observation: Observation, legal_moves: tuple[Move, ...]) -> tuple[Move, ...]:
    """Take recognized wins and last chances, then retain viable return routes."""
    if not legal_moves:
        raise ValueError("A bot needs at least one legal move")
    if observation.phase == "response":
        response = None
        if not observation.hand and not observation.draw_penalty and (
                not observation.skip_pending or observation.opponent_count == 0):
            response = Accept
        elif len(observation.hand) == 1:
            last = observation.hand[0]
            winning_final = (observation.opponent_count == 0 and _contribution(last)
                             or observation.opponent_count == 1 and last.rank == "A")
            # The core declaration helper needs only these public top/effect/suit fields.
            if winning_final and last in _declarations(observation):
                response = Accept
        if response is None:
            last_chance = (observation.opponent_count == 0 and (
                not observation.hand and observation.draw_penalty
                or len(observation.hand) == 1 and observation.skip_pending))
            known_bluff = (observation.top in observation.hand
                           or observation.top in observation.known_pile_cards)
            if last_chance or known_bluff:
                response = Challenge
        return tuple(move for move in legal_moves if isinstance(move, response)) if response else legal_moves

    if len(observation.hand) == 1:
        for move in legal_moves:
            if (isinstance(move, PlayCard) and move.actual_card == move.declared_card
                    and (observation.opponent_count == 0 and _contribution(move.declared_card)
                         or observation.opponent_count == 1 and move.declared_card.rank == "A")):
                return (move,)
    if observation.opponent_count == 0:
        returns = tuple(move for move in legal_moves if isinstance(move, PlayCard)
                        and (_contribution(move.declared_card)
                             or move.declared_card.rank == "A" and len(observation.hand) > 1))
        return returns or legal_moves
    return legal_moves
