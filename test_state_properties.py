"""Exhaustive move sets and seeded transition checks, without a gameplay bot."""

from dataclasses import replace
import pickle
import random
import unittest

from bluff_mau_mau import (
    Accept, CARDS, Card, Challenge, Draw, GameState, MoveGenerator, NewGame,
    Play, PlayCard, RANKS, SUITS, Skip,
)


def position(top, *, turn=0, chosen_suit=None, penalty=0, skip=False):
    remaining = tuple(card for card in CARDS if card != top)
    return GameState(
        pile=(top,), top=top, hands=(remaining[:3], remaining[3:7]),
        deck=remaining[7:], turn=turn, chosen_suit=chosen_suit,
        draw_penalty=penalty, skip_pending=skip,
    )


class StatePropertyTests(unittest.TestCase):
    def test_all_normal_top_cards_and_queen_suits_have_complete_move_sets(self):
        for top in CARDS:
            for chosen in ((None, *SUITS) if top.rank == "Q" else (None,)):
                for turn in (0, 1):
                    with self.subTest(top=top, chosen=chosen, turn=turn):
                        state = position(top, turn=turn, chosen_suit=chosen)
                        expected = {Draw()}
                        for actual in state.hands[turn]:
                            # Queens are always legal, each with four declarations
                            # and four continuing suits, independently of the hand.
                            expected.update(PlayCard(actual, Card("Q", suit), next_suit)
                                            for suit in SUITS for next_suit in SUITS)
                            if top.rank == "Q" and chosen is None:
                                identities = {(rank, suit) for rank in RANKS if rank != "Q"
                                              for suit in SUITS}
                            else:
                                identities = {(rank, chosen or top.suit)
                                              for rank in RANKS if rank != "Q"}
                                if top.rank != "Q":
                                    identities.update((top.rank, suit) for suit in SUITS)
                            expected.update(PlayCard(actual, Card(rank, suit))
                                            for rank, suit in identities)
                        actual_moves = MoveGenerator(state)
                        self.assertEqual(set(actual_moves), expected)
                        self.assertEqual(len(actual_moves), len(expected))

    def test_all_pending_effect_move_sets_ignore_actual_card_identity(self):
        queens = {Card("Q", suit) for suit in SUITS}
        sevens = {Card("7", suit) for suit in SUITS} | queens
        aces = {Card(rank, suit) for rank in ("A", "Q") for suit in SUITS}
        cases = [(Card("A", suit), 0, True, aces, Skip()) for suit in SUITS]
        for suit in SUITS:
            for amount in (2, 6, 30, 100):
                counters = sevens | ({Card("K", "S")} if suit == "S" else set())
                cases.append((Card("7", suit), amount, False, counters, Draw()))
        cases += [(Card("K", "S"), amount, False, {Card("7", "S")} | queens, Draw())
                  for amount in (4, 8, 32, 100)]
        for top, amount, skip, identities, alternative in cases:
            for turn in (0, 1):
                with self.subTest(top=top, amount=amount, turn=turn):
                    state = position(top, turn=turn, penalty=amount, skip=skip)
                    expected = {alternative} | {
                        PlayCard(actual, declared, suit) for actual in state.hands[turn]
                        for declared in identities
                        for suit in (SUITS if declared.rank == "Q" else (None,))
                    }
                    self.assertEqual(set(MoveGenerator(state)), expected)
                    for move in expected:
                        result = Play(state, move)
                        MoveGenerator(result)

    def test_no_draw_removes_only_draw_choice_in_every_suit_and_effect(self):
        for top in CARDS:
            for effect in (False, True):
                penalty = (2 if top.rank == "7" else 4 if top == Card("K", "S") else 0)
                state = position(top, penalty=penalty if effect else 0,
                                 skip=effect and top.rank == "A")
                available = set(MoveGenerator(state))
                empty_deck = replace(state, deck=(),
                                     hands=(state.hands[0], state.hands[1] + state.deck))
                self.assertEqual(set(MoveGenerator(empty_deck)), available - {Draw()})

    def test_hidden_truth_does_not_change_response_options(self):
        for declared in CARDS:
            for actual in CARDS:
                for chosen in (SUITS if declared.rank == "Q" else (None,)):
                    start = Card("Q", "H") if actual != Card("Q", "H") else Card("Q", "D")
                    rest = tuple(card for card in CARDS if card not in (actual, start))
                    state = GameState(deck=rest[2:], pile=(start,),
                                      hands=((actual, rest[0]), (rest[1],)), turn=0, top=start)
                    pending = Play(state, PlayCard(actual, declared, chosen))
                    expected = [Accept(), Challenge()] + [
                        move for move in MoveGenerator(replace(pending, phase="turn"))
                        if not isinstance(move, Skip)
                    ]
                    self.assertEqual(MoveGenerator(pending), expected)
                    accepted = Play(pending, Accept())
                    self.assertEqual(accepted.top, declared)
                    self.assertEqual(accepted.pile[-1], actual)
                    revealed = Play(pending, Challenge())
                    self.assertEqual(revealed.top, actual)
                    self.assertEqual(revealed.turn, 0 if actual == declared else 1)
                    self.assertEqual(revealed.draw_penalty, 0)
                    self.assertFalse(revealed.skip_pending)
                    MoveGenerator(accepted)
                    MoveGenerator(revealed)

    def test_player_relabeling_commutes_with_every_available_move(self):
        def swap(state):
            return replace(state, hands=state.hands[::-1], turn=1 - state.turn,
                           provisional_winner=(None if state.provisional_winner is None
                                               else 1 - state.provisional_winner),
                           winner=None if state.winner is None else 1 - state.winner)

        states = [position(top) for top in CARDS]
        states += [position(Card("7", "S"), penalty=10),
                   position(Card("K", "S"), penalty=8),
                   position(Card("A", "H"), skip=True)]
        seed = position(Card("Q", "H"))
        # Include a pending first final play and both-card final responses.
        one_card = replace(seed, deck=seed.deck + seed.hands[0][1:],
                           hands=(seed.hands[0][:1], seed.hands[1]))
        pending = Play(one_card, PlayCard(one_card.hands[0][0], Card("9", "H")))
        accepted = Play(pending, Accept())
        states += [pending, accepted]
        states += [Play(accepted, move) for move in MoveGenerator(accepted)
                   if isinstance(move, PlayCard)]
        last_reply = replace(accepted, deck=accepted.deck + accepted.hands[1][1:],
                             hands=((), accepted.hands[1][:1]))
        states += [Play(last_reply, move) for move in MoveGenerator(last_reply)
                   if isinstance(move, PlayCard)]
        for state in states:
            self.assertEqual(MoveGenerator(swap(state)), MoveGenerator(state))
            for move in MoveGenerator(state):
                self.assertEqual(Play(swap(state), move), swap(Play(state, move)))

    def test_seeded_action_sequences_conserve_and_replay_full_state(self):
        # Test-data sampling only: bounded traces exercise transitions and do not
        # provide a player implementation or choose actions by their quality.
        global_rng = random.getstate()
        counts = {kind: 0 for kind in (PlayCard, Draw, Skip, Accept, Challenge)}
        recycled = 0
        for seed in range(40):
            sampler = random.Random(seed + 1000)
            state = NewGame(seed, dealer=seed % 2)
            if seed == 0:
                state = replace(state, deck=(), pile=state.deck + state.pile)
            for step in range(250):
                with self.subTest(seed=seed, step=step):
                    before = pickle.dumps(state)
                    moves = MoveGenerator(state)
                    if state.winner is not None:
                        self.assertEqual(moves, [])
                        break
                    self.assertTrue(moves)
                    self.assertEqual(len(moves), len(set(moves)))
                    # Skip now exists only on a starting ace: cover that rare branch explicitly.
                    move = Skip() if step == 0 and Skip() in moves else sampler.choice(moves)
                    if seed == 0 and step == 0:
                        move = Draw()  # Exercise recycling without relying on random frequency.
                    counts[type(move)] += 1
                    result = Play(state, move)
                    self.assertEqual(result, Play(state, move))
                    self.assertEqual(pickle.dumps(state), before)
                    cards = result.deck + result.pile + result.hands[0] + result.hands[1]
                    self.assertEqual(len(cards), 32)
                    self.assertEqual(set(cards), set(CARDS))
                    MoveGenerator(result)
                    recycled += len(result.pile) < len(state.pile)
                    state = result
        self.assertTrue(all(counts.values()), counts)
        self.assertGreater(recycled, 0)
        self.assertEqual(random.getstate(), global_rng)


if __name__ == "__main__":
    unittest.main()
