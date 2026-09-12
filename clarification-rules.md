# Rule clarifications

`RULES.md` remains authoritative. The following conventions make its decisions
explicit in the function API.

1. **One response per play.** Playing a card returns a state where the opponent
   can accept, challenge, or commit a legal next play/draw that implicitly accepts
   the previous claim. Selecting cards alone does not accept. The latest
   declaration's effects are recorded in that state but are not applied yet. For example, declaring a seven over
   an accumulated two-card penalty records four; a challenge loser draws six.

2. **Forced empty-hand actions happen within acceptance.** A player with no cards
   still chooses `Accept()` or `Challenge()`. Once they accept a return penalty,
   drawing and ending their turn happen in that same `Play` call. An accepted
   ace automatically skips their empty hand when the opponent still has cards.
   This avoids an extra decision with no alternative. For example, accepting an
   opponent's final return seven draws the available penalty cards and confirms
   that opponent's victory; the newly drawn cards cannot counter it.

3. **Accepting an ace consumes its skip immediately.** To counter an ace, submit
   an ace or queen directly in response phase. Explicit acceptance gives the turn
   back to the ace's player and confirms their victory if their hand is empty.
   A starting ace still offers `Skip()` because it has no claim to accept.
   Accepting a seven or K♠ with cards in hand leaves the choice to counter or draw.

4. **A queen cancels all pending effects.** Any declared queen and continuing suit
   can counter an ace, seven, or K♠. Playing it clears the skip or accumulated draw
   penalty. If that queen is challenged, the loser draws only two cards. A queen
   does not return an already empty opponent; accepting it confirms that opponent's win.

Setup uses a seeded shuffle, deals alternately starting with the non-dealer, and
treats the first draw-pile element as the next card. These are reproducible
ordering conventions; they add no restriction to legal moves.
