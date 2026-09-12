**Bluff Mau-Mau** uses our two-player Prší rules, but every played card is face down and may be falsely declared.

## Legal moves only

Players choose only from the legal moves available in the current game state. The implementation’s move generator enforces these rules: illegal declarations and other unavailable actions cannot be selected or played. This applies throughout the game, including penalty counters, final cards, and return attempts.

**Bluffing means hiding any card from your hand behind a legal declaration.** The declaration must obey the current suit/rank and any pending effects; the actual card need not match it. A challenge checks only whether that legal declaration matches the actual card. Illegal moves never reach acceptance or challenge, so no in-game penalty or recovery procedure for them is needed.

## Setup and normal play

- Use **32 cards**: 7, 8, 9, 10, J, Q, K, A in ♥ ♦ ♣ ♠. No jokers.
- Deal **five cards each**. Reveal one starting discard; the remaining cards form the draw pile. The non-dealer starts.
- On your turn, play **one card face down** and announce its identity (rank and suit). Your **declared card must always be legal**, including when countering a penalty or returning an empty-handed player. The actual card can be anything in your hand.
- Normally, the declaration must match the current **suit or rank**.
- Alternatively, draw one card and end your turn—even if you could play. You cannot immediately play the drawn card. Outstanding draw penalties and aces follow the rules below.
- If drawing is impossible even after recycling discards, a player with cards **must make a legal play instead of choosing to draw or pass**. The actual card need not match: a legal bluff is still a possible play.

## Card effects

| Declared card | Effect |
|---|---|
| **Queen** | Can be played on **any card unless an ace skip is pending**, including a seven or K♠. It cancels the entire accumulated draw penalty. Announce its identity and the suit to continue with **before the opponent accepts or challenges**. The chosen suit cannot then be changed. Another queen may follow. |
| **Ace** | Accepting immediately skips the opponent's turn; alternatively they can counter only with another ace before accepting. Aces pass the skip onward; they do not accumulate multiple skipped turns. |
| **Any seven** | Adds **two cards** to the draw penalty. Another seven can counter it. |
| **K♠** | Adds **four cards**. Only **7♠** adds to it; a queen cancels it. **K♠ can also counter 7♠**. |
| **Other cards** | No special effect. |

Draw penalties accumulate until someone takes them or plays a queen to cancel them. Taking the penalty clears it and ends that player’s turn. A skipped turn consumes the ace’s skip effect. In either case, the top card still determines the current suit/rank.

If the **starting card is a queen**, there is **no suit or rank restriction until another card is played**. This freedom applies to **both players on their turns**, for as many turns as the queen remains on top. Its printed suit does not restrict play. Drawing ends the turn as usual but **does not remove this freedom**; drawing is allowed only while cards can be drawn. The next played card is accepted or challenged normally and determines how play continues.

Starting aces, sevens and K♠ have **no active skip or draw effect**. Until the first card is played, use normal suit/rank matching (or a queen), even after intervening draws: **A♥ → 8♥** and **7♠ → 8♠** are legal. An opening seven or K♠ still contributes its two or four cards **if the first played declaration is a legal seven or K♠**. For example, opening **7♥ → 7♠** creates a **four-card** penalty; a challenge loser draws **six**. Opening **7♠ → K♠** or **K♠ → 7♠** creates six. An ordinary reply or queen discards this opening contribution. This exception ends with the first played card and is never restored by recycling.

## Trusting or challenging

After each play, before applying its effect to the opponent, the opponent chooses:

- **Accept:** Continue according to the declaration. It now counts as that card, regardless of its actual identity, and cannot be challenged later. Accepting an ace immediately consumes the skip and gives the turn back to its player.
- **Play or draw:** A legal next play or draw implicitly accepts the previous declaration in the same decision. Only aces can counter a pending ace this way, before its skip is consumed. Merely choosing a card or declaration does not accept anything.
- **Challenge:** Reveal the latest card and compare its printed rank and suit with the declaration. A queen’s chosen continuing suit is separate from its identity and does not make an otherwise truthful declaration false.

If the declaration was false, **the bluffer loses the challenge**. If it was truthful, **the challenger loses**.

In either case:

- The loser draws **the entire accumulated draw penalty, including the challenged declaration’s contribution, plus two extra cards**. Use the declared card’s contribution, not the revealed card’s actual effect.
- **All pending draw and skip effects are cleared.** The revealed card does not activate a new special effect.
- The **challenge winner takes the next turn**. If their hand is empty, they win immediately.
- The revealed card stays on top; its **actual suit/rank** determines continued play.
- If the revealed card is a **queen**, discard any previously announced continuing suit and ignore its printed suit. The same unrestricted-play rule as for a starting queen applies: **both players may declare any card on their turns until another card is played, even after intervening draws**. The challenge winner takes the next turn; if they draw, the opponent still has this freedom. Drawing is allowed only while cards can be drawn.

With no earlier accumulated penalty:

| Challenged declaration | Challenge loser draws |
|---|---:|
| Ordinary card, queen or ace | **2** |
| Seven | **4** |
| K♠ | **6** |

Only the latest card is checked. Earlier accepted declarations remain valid even if those hidden cards were not what their players declared.

## Going out and returning

- You may play any actual card as your last card, including a bluff, but **your declaration must still be legal**. No special announcement is required.
- Your final play can still be challenged. If it was truthful, the challenger draws the challenge penalty and **you win immediately**. If it was a bluff, you draw the challenge penalty and the game continues.
- If your final play is accepted, emptying your hand is **provisional**: the opponent can bring you back by legally declaring a **seven or K♠** and making you draw the applicable accumulated penalty.
- The return attempt may itself be a bluff. You must **accept or challenge before drawing**, even though your hand is empty. If you accept, you take the penalty. If you challenge and catch a bluff, the opponent takes the challenge penalty and **you win**. If their declaration was truthful, you take the challenge penalty and return to play.
- The opponent may first play **consecutive legal aces**, skipping your empty hand, then declare a legal seven or K♠. You may accept or challenge **each ace separately**. Losing such a challenge also brings you back by making you draw cards.
- If the opponent does not bring you back, **you win**. Drawing, skipping, or playing an accepted card other than an ace or a return penalty does not prolong their opportunity to return you.
- **Being returned cancels your previous claim to victory.** Priority belongs to whoever next empties their hand and lasts only while their hand stays empty. If the opponent also empties their hand without making that player draw—for example, with an accepted ace—the player who was already empty wins. If the opponent’s final card makes that player draw, the opponent wins: the returned player cannot counter with the cards they have just drawn.
- **Resolve acceptance or a challenge before deciding victory.** Even an opponent’s final ace or ordinary card can bring you back if you challenge it and it is truthful: you draw the challenge penalty and the opponent wins. If you accept that same final ace or ordinary card, you win because you were already empty and it did not make you draw. If you challenge it and catch a bluff, you also win.

## When the draw pile runs out

Keep the top discard and shuffle the rest into a new draw pile **without inspecting hidden cards**. Recycle during a draw if necessary.

If a draw cannot be completed even after recycling, draw **as many cards as possible** and cancel the unpaid remainder. Apply the normal turn transition: taking a normal draw or draw penalty ends the turn; after a challenge, the winner takes the next turn.

If **no cards at all can be drawn**, a player with cards must play a legal declaration instead of choosing to draw or pass. This does not remove an ace’s skip effect. A challenge is still resolved normally even if its loser cannot draw the full penalty. There is no draw result from two voluntary passes.

## Examples

### A legal declaration can hide any card

The current card is **9♥**. You play **J♣** face down and declare **7♥**. This is a legal declaration because hearts match. If accepted, it imposes a two-card penalty. Declaring **K♠** here is not an available move, even if you hold the actual K♠. You may still play that K♠ face down under a legal declaration such as **7♥**.

### Penalties stack, but only the latest card is challenged

You declare a seven; your opponent accepts and declares another seven. You challenge the second card. The challenge loser draws **2 + 2 + 2 = 6 cards**. Only the second card is revealed and checked. The first accepted declaration still contributes two cards even if it was a bluff. The accumulated penalty is then cleared.

### K♠ has specific counters

After a declared **7♠**, the opponent may counter with **K♠**, bringing the penalty to **six cards**. The next stacking counter can only be **7♠**, bringing it to eight. A queen of any suit can instead cancel the entire six-card penalty; challenging that queen costs the loser two cards. **7♥** cannot counter K♠, and K♠ cannot counter 7♥.

### A revealed special card does not activate its effect

You declare **7♠**, but a challenge reveals **A♥**. With no earlier penalty, you draw **four cards** for the false seven declaration. Your opponent takes the next turn, matching **hearts or ace**, or declaring a queen. The revealed ace does **not** force a skip.

If you instead declare an ordinary card and a challenge reveals **K♠**, you draw **two cards**, not six. No new four-card penalty starts.

### A revealed queen allows any next play

You declare **9♥**, but a challenge reveals **Q♣**. With no earlier penalty, you draw two cards. The challenge winner takes the next turn and may declare any card. If they choose to draw instead, you may still declare any card on your turn: neither player has to match clubs. This freedom lasts until another card is played.

The same freedom applies after a challenge reveals a truthfully declared queen: its previously chosen continuing suit no longer applies.

### Drawing does not end an unrestricted queen

The starting card is **Q♥**. Alice draws instead of playing. Bob may legally declare **9♣**, despite its matching neither the queen’s rank nor its printed suit. If Alice accepts that declaration, play continues on **9♣** under the normal rules.

Bob may also draw instead, if drawing is possible. In that case, Alice may still declare any card on her next turn. Any number of intervening draws leaves this freedom intact. The same rule applies to a queen revealed by a challenge.

### A challenged final card

You play your last card, declaring **9♠**, and your opponent challenges. If it really is 9♠, they draw two cards and **you win immediately**. If it is another card, you draw two and they take the next turn.

If your final declaration was instead a seven, the challenge loser draws **four cards**, plus any earlier accumulated penalty.

### A bluff can return an empty-handed player

You finish with an accepted **9♥**. Your opponent plays a hidden card and declares **7♥**:

- If you **accept**, you draw two cards and return to play, even if the hidden card was not a seven.
- If you **challenge and it is a bluff**, your opponent draws four cards and **you win**.
- If you **challenge and it really is 7♥**, you draw four cards and return to play.

Your opponent cannot declare K♠ directly on 9♥: the return declaration must still be legal.

### Aces can extend a return attempt

You finish with an accepted **9♥**. Your opponent declares **A♥**; you accept, so your empty hand is skipped. They then declare **A♠**; you accept again. They may now legally declare **K♠** or **7♠** to attempt to return you. Every play can be challenged separately.

If either accepted ace was their last card, **you win**: you emptied your hand first and they did not make you draw.

### Both players play their last card

You finish with an accepted **9♥**. Your opponent plays their last card as **7♥**:

- You accept: you draw **two cards** and **they win**, because they have returned you and you cannot counter while drawing the penalty.
- You challenge and it is false: they draw **four cards** and **you win**.
- You challenge and it is truthful: you draw **four cards** and **they win**.

### Victory priority resets when a player returns

Alice empties her hand, but Bob returns her with a seven. Later, Bob finishes with an accepted **9♥**. Alice responds with her final card, declaring **A♥**, and Bob accepts. **Bob wins**: Alice’s earlier empty hand no longer gives her priority, and her accepted ace did not return Bob.

### Challenging a final ace can cost you the win

Alice finishes with an accepted **9♥**. Bob plays his final card, declaring **A♥**:

- Alice accepts: **Alice wins**. Bob has no cards left to return her.
- Alice challenges and it really is A♥: Alice draws **two cards** and **Bob wins**.
- Alice challenges and it is a bluff: Bob draws **two cards** and **Alice wins**.

The same outcomes apply if Bob’s final declaration is an ordinary legal card such as **10♥**. Acceptance or the challenge is resolved before deciding the winner.

### Too few cards to draw

You owe **eight cards**, but only three can be drawn even after recycling. You take those three, the remaining five are cancelled, and your turn ends. If those eight were a challenge penalty, the challenge winner takes the next turn instead.

If the draw pile and recyclable discards are both empty when you would choose a normal draw, you **must play**. Even a hand with no matching actual card can make a legal declaration by bluffing; you cannot pass instead.
