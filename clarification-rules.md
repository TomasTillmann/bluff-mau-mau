# Rule clarifications

`RULES.md` remains authoritative. No additional gameplay rule was needed.
The following conventions make its decisions explicit in the function API.

1. **One response per play.** Playing a card returns a state where the opponent
   can only accept or challenge. The latest declaration's effects are recorded
   in that state but are not applied yet. For example, declaring a seven over
   an accumulated two-card penalty records four; a challenge loser draws six.

2. **Forced empty-hand actions happen within acceptance.** A player with no cards
   still chooses `Accept()` or `Challenge()`. Once they accept a return penalty,
   drawing and ending their turn happen in that same `Play` call. An accepted
   ace automatically skips their empty hand when the opponent still has cards.
   This avoids an extra decision with no alternative. For example, accepting an
   opponent's final return seven draws the available penalty cards and confirms
   that opponent's victory; the newly drawn cards cannot counter it.

3. **Nonempty hands make their own draw/skip decisions.** Accepting a first
   finisher's seven gives the opponent a turn to counter or draw. Accepting a
   first finisher's ace gives them a turn to counter with an ace or skip.
   Acceptance alone does not consume that choice or confirm the first victory.

Setup uses a seeded shuffle, deals alternately starting with the non-dealer, and
treats the first draw-pile element as the next card. These are reproducible
ordering conventions; they add no restriction to legal moves.
