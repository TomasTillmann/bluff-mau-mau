"""Pure, two-player Bluff Mau-Mau engine. See RULES.md and README.md."""

from dataclasses import dataclass, field, replace
from math import isfinite
from random import Random


RANKS = ("7", "8", "9", "10", "J", "Q", "K", "A")
SUITS = ("H", "D", "C", "S")


@dataclass(frozen=True, slots=True)
class Card:
    rank: str
    suit: str

    def __post_init__(self):
        if (type(self.rank) is not str or type(self.suit) is not str
                or self.rank not in RANKS or self.suit not in SUITS):
            raise ValueError("Card needs rank 7..A and suit H, D, C, or S")


CARDS = tuple(Card(rank, suit) for suit in SUITS for rank in RANKS)


@dataclass(frozen=True, slots=True)
class PlayCard:
    actual_card: Card
    declared_card: Card
    chosen_suit: str | None = None


@dataclass(frozen=True, slots=True)
class Draw:
    pass


@dataclass(frozen=True, slots=True)
class Challenge:
    pass


@dataclass(frozen=True, slots=True)
class Accept:
    pass


@dataclass(frozen=True, slots=True)
class Skip:
    pass


Move = PlayCard | Draw | Challenge | Accept | Skip


@dataclass(frozen=True, slots=True)
class GameState:
    deck: tuple[Card, ...]
    pile: tuple[Card, ...]
    hands: tuple[tuple[Card, ...], tuple[Card, ...]]
    turn: int
    top: Card
    chosen_suit: str | None = None
    draw_penalty: int = 0
    skip_pending: bool = False
    phase: str = "turn"
    provisional_winner: int | None = None
    winner: int | None = None
    rng_state: tuple = field(default_factory=lambda: Random(0).getstate(), repr=False)


def _contribution(card: Card) -> int:
    if card.rank == "7":
        return 2
    return 4 if card == Card("K", "S") else 0


def _validate(state: GameState) -> None:
    """Reject malformed externally constructed states at the public boundary."""
    if type(state) is not GameState:
        raise ValueError("Expected GameState")
    if (not issubclass(type(state.deck), tuple) or not issubclass(type(state.pile), tuple)
            or not issubclass(type(state.hands), tuple) or len(state.hands) != 2
            or any(not issubclass(type(hand), tuple) for hand in state.hands)):
        raise ValueError("Deck, pile, and both hands must be tuples")
    cards = state.deck + state.pile + state.hands[0] + state.hands[1]
    if (not state.pile or len(cards) != 32
            or any(type(card) is not Card for card in cards)
            or set(cards) != set(CARDS)):
        raise ValueError("State must contain each of the 32 cards exactly once")
    if type(state.turn) is not int or state.turn not in (0, 1):
        raise ValueError("Turn must be player 0 or 1")
    if type(state.top) is not Card:
        raise ValueError("Top must be an effective card identity")
    if state.chosen_suit is not None and (
            type(state.chosen_suit) is not str
            or state.top.rank != "Q" or state.chosen_suit not in SUITS):
        raise ValueError("Only a queen can have a continuing suit")
    if (type(state.draw_penalty) is not int or state.draw_penalty < 0
            or state.draw_penalty % 2 or type(state.skip_pending) is not bool
            or (state.draw_penalty and not _contribution(state.top))
            or (state.draw_penalty and state.draw_penalty < _contribution(state.top))
            or (state.skip_pending and state.top.rank != "A")
            or (state.draw_penalty and state.skip_pending)):
        raise ValueError("Invalid pending effect")
    if type(state.phase) is not str or state.phase not in ("turn", "response", "finished"):
        raise ValueError("Unknown decision phase")
    for player in (state.provisional_winner, state.winner):
        if player is not None and (
                type(player) is not int or player not in (0, 1) or state.hands[player]):
            raise ValueError("A victory claim must identify an empty-handed player")
    if (state.phase == "response" and not any(state.hands)
            and state.provisional_winner != state.turn):
        raise ValueError("The responder must hold the earlier empty-hand victory claim")
    if (state.phase == "finished") != (state.winner is not None):
        raise ValueError("Finished phase must have a winner")
    if state.winner is not None:
        if state.turn != state.winner:
            raise ValueError("Finished turn must identify the winner")
        if state.provisional_winner is not None or state.draw_penalty or state.skip_pending:
            raise ValueError("Finished games have no pending effects or victory claim")
    else:
        if any(not hand for hand in state.hands) != (state.provisional_winner is not None):
            raise ValueError("An empty hand needs a provisional victory claim")
        if state.phase == "turn" and not state.hands[state.turn]:
            raise ValueError("Forced empty-hand actions must already be resolved")
    if state.phase == "response" and (
            len(state.pile) < 2 or (state.top.rank == "Q" and state.chosen_suit is None)
            or state.draw_penalty < _contribution(state.top)
            or state.skip_pending != (state.top.rank == "A")):
        raise ValueError("A response needs a complete declaration and its pending effect")
    try:
        # Random.setstate also accepts mutable containers; require its tuple format.
        if (not issubclass(type(state.rng_state), tuple) or len(state.rng_state) != 3
                or type(state.rng_state[0]) is not int
                or not issubclass(type(state.rng_state[1]), tuple)
                or any(type(word) is not int for word in state.rng_state[1])
                or (state.rng_state[2] is not None and (
                    type(state.rng_state[2]) is not float or not isfinite(state.rng_state[2])))):
            raise ValueError
        Random(0).setstate(state.rng_state)
    except (TypeError, ValueError, OverflowError) as error:
        raise ValueError("Invalid random-generator state") from error


def NewGame(seed: int = 0, dealer: int = 0) -> GameState:
    """Shuffle, deal five each starting with the non-dealer, and reveal a card."""
    if type(seed) is not int or type(dealer) is not int or dealer not in (0, 1):
        raise ValueError("Seed must be an integer; dealer must be 0 or 1")
    rng = Random(seed)
    cards = list(CARDS)
    rng.shuffle(cards)
    hands = [(), ()]
    hands[1 - dealer], hands[dealer] = tuple(cards[:10:2]), tuple(cards[1:10:2])
    top = cards[10]
    return GameState(
        deck=tuple(cards[11:]), pile=(top,), hands=tuple(hands), turn=1 - dealer,
        top=top, draw_penalty=_contribution(top), skip_pending=top.rank == "A",
        rng_state=rng.getstate(),
    )


def _declarations(state: GameState) -> tuple[Card, ...]:
    if state.skip_pending:
        return tuple(card for card in CARDS if card.rank in ("A", "Q"))
    if state.draw_penalty:
        if state.top == Card("K", "S"):
            return tuple(card for card in CARDS if card.rank == "Q" or card == Card("7", "S"))
        return tuple(card for card in CARDS if card.rank in ("7", "Q")
                     or (state.top == Card("7", "S") and card == Card("K", "S")))
    if state.top.rank == "Q" and state.chosen_suit is None:
        return CARDS
    suit = state.chosen_suit or state.top.suit
    return tuple(card for card in CARDS
                 if card.rank in ("Q", state.top.rank) or card.suit == suit)


def MoveGenerator(state: GameState) -> list[Move]:
    """Enumerate all legal decisions, including all legal bluff declarations."""
    _validate(state)
    if state.winner is not None:
        return []
    moves: list[Move] = [Accept(), Challenge()] if state.phase == "response" else []
    if not state.hands[state.turn]:
        return moves
    if state.skip_pending:
        if state.phase == "turn":
            moves.append(Skip())
    elif state.deck or len(state.pile) > 1:
        moves.append(Draw())
    declarations = _declarations(state)
    for actual in state.hands[state.turn]:
        for declared in declarations:
            for suit in SUITS if declared.rank == "Q" else (None,):
                moves.append(PlayCard(actual, declared, suit))
    return moves


def _draw(state: GameState, player: int, count: int) -> GameState:
    deck, pile = state.deck, state.pile
    drawn: tuple[Card, ...] = ()
    rng = Random(0)
    rng.setstate(state.rng_state)
    while count:
        if not deck:
            if len(pile) == 1:
                break
            recycled = list(pile[:-1])
            rng.shuffle(recycled)
            deck, pile = tuple(recycled), pile[-1:]
        take = min(count, len(deck))
        drawn += deck[:take]
        deck = deck[take:]
        count -= take
    hands = list(state.hands)
    hands[player] += drawn
    claimant = state.provisional_winner
    if drawn and claimant == player:
        claimant = 1 - player if not hands[1 - player] else None
    return replace(state, deck=deck, pile=pile, hands=tuple(hands),
                   provisional_winner=claimant, rng_state=rng.getstate())


def _finish(state: GameState, player: int) -> GameState:
    return replace(state, turn=player, phase="finished", winner=player,
                   provisional_winner=None, draw_penalty=0, skip_pending=False)


def Play(state: GameState, move: Move) -> GameState:
    """Apply one legal decision; invalid states/moves raise ValueError."""
    if type(move) not in (PlayCard, Draw, Challenge, Accept, Skip):
        raise ValueError("Expected a supported move type")
    if isinstance(move, PlayCard) and (
            type(move.actual_card) is not Card or type(move.declared_card) is not Card):
        raise ValueError("A play needs concrete actual and declared cards")
    if (isinstance(move, PlayCard) and move.chosen_suit is not None
            and type(move.chosen_suit) is not str):
        raise ValueError("A chosen suit must be a suit code")
    if move not in MoveGenerator(state):
        raise ValueError("Move is not legal in this state")
    player, other = state.turn, 1 - state.turn
    if isinstance(move, PlayCard):
        hands = list(state.hands)
        hands[player] = tuple(card for card in hands[player] if card != move.actual_card)
        claimant = state.provisional_winner
        if not hands[player] and claimant is None:
            claimant = player
        return replace(
            state, hands=tuple(hands), pile=state.pile + (move.actual_card,),
            turn=other, top=move.declared_card, chosen_suit=move.chosen_suit,
            phase="response", draw_penalty=(0 if move.declared_card.rank == "Q" else
                                            state.draw_penalty + _contribution(move.declared_card)),
            skip_pending=move.declared_card.rank == "A", provisional_winner=claimant,
        )
    if isinstance(move, Challenge):
        winner = other if state.pile[-1] == state.top else player
        result = _draw(state, 1 - winner, state.draw_penalty + 2)
        result = replace(result, turn=winner, phase="turn", top=result.pile[-1],
                         chosen_suit=None, draw_penalty=0, skip_pending=False)
        return _finish(result, winner) if not result.hands[winner] else result
    if isinstance(move, Accept):
        result = replace(state, phase="turn")
        if state.skip_pending and any(state.hands):
            result = replace(result, skip_pending=False, turn=other)
            return _finish(result, other) if not result.hands[other] else result
        if state.hands[player]:
            return result
        if state.draw_penalty:
            result = _draw(result, player, state.draw_penalty)
            result = replace(result, draw_penalty=0, turn=other)
            return _finish(result, other) if not result.hands[other] else result
        return _finish(result, player)
    if isinstance(move, Draw):
        result = _draw(state, player, state.draw_penalty or 1)
        result = replace(result, turn=other, phase="turn", draw_penalty=0)
    else:  # The legality check leaves only Skip here.
        result = replace(state, turn=other, skip_pending=False)
    return _finish(result, other) if not result.hands[other] else result
