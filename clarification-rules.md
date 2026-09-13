# Rule clarifications

`RULES.md` remains authoritative. The following conventions make its decisions
explicit in the function API.

1. **One response per play.** Playing a card returns a state where the opponent
   can accept, challenge, or commit a legal next play/draw that implicitly accepts
   the previous claim. Selecting cards alone does not accept. The latest
   declaration's effects are recorded in that state but are not applied yet. For example, declaring a seven over
   an accumulated two-card penalty records four; a challenge loser draws six.

2. **Forced empty-hand actions happen within acceptance.** A player with no cards
   still chooses `Move::Accept` or `Move::Challenge`. Once they accept a return penalty,
   drawing and ending their turn happen in that same `play` call. An accepted
   ace automatically skips their empty hand when the opponent still has cards.
   This avoids an extra decision with no alternative. For example, accepting an
   opponent's final return seven draws the available penalty cards and confirms
   that opponent's victory; the newly drawn cards cannot counter it.

3. **Accepting an ace consumes its skip immediately.** To counter an ace, submit
   another ace directly in response phase. Explicit acceptance gives the turn
   back to the ace's player and confirms their victory if their hand is empty.
   A starting ace has no skip effect: normal matching declarations or a queen
   are legal, and drawing takes one card.
   Accepting a seven or K♠ with cards in hand leaves the choice to counter or draw.

4. **A queen cancels pending draw penalties, but cannot counter an ace.** Any
   declared queen and continuing suit can counter a seven or K♠, clearing the
   accumulated draw penalty. Once an ace skip is consumed or cleared by a
   challenge, queens are legal again. If a queen is challenged, the loser draws
   only two cards. A queen does not return an already empty opponent; accepting
   it confirms that opponent's win.

5. **Opening special cards are inactive, with a conditional penalty contribution.**
   `opening_card` stays true until the first `Move::Play`, including across draws.
   Opening aces, sevens and K♠ allow normal matching play without owing a skip or
   penalty. If that first declaration is a seven or K♠, add the starting card’s
   two/four-card contribution; otherwise discard it. Opening 7♥ → 7♠ owes four,
   or six to a challenge loser. Later played specials follow normal effect rules;
   a recycled one-card pile never restores opening status.

Setup uses a seeded shuffle, deals alternately starting with the non-dealer, and
treats the first draw-pile element as the next card. These are reproducible
ordering conventions; they add no restriction to legal moves.
