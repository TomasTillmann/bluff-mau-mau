"""Explain one move using only adjacent public game snapshots."""

SUITS = {"H": "hearts", "D": "diamonds", "C": "clubs", "S": "spades"}
RANKS = {"J": "jack", "Q": "queen", "K": "king", "A": "ace"}


def explain_move(before, after, perspective=0):
    """Return the last move's public result, from one fixed player's perspective."""
    if before is None or before == after:
        return None
    actor = before["turn"]
    drawn = [max(0, after["hand_counts"][p] - before["hand_counts"][p]) for p in (0, 1)]
    names = ["You" if p == perspective else "Your opponent" for p in (0, 1)]
    possessives = ["Your" if p == perspective else "Your opponent’s" for p in (0, 1)]
    detail = []
    requested = 0
    recipient = None

    def card_name(card):
        return f"{RANKS.get(card[:-1], card[:-1])} of {SUITS[card[-1]]}"

    def draw_text(player):
        count = drawn[player]
        return f"{names[player]} drew {count} card{'s' if count != 1 else ''}."

    if after["phase"] == "response" and after["hand_counts"][actor] < before["hand_counts"][actor]:
        kind = "play"
        title = f"{names[actor]} declared {card_name(after['top'])}."
        if before["phase"] == "response":
            detail.append(f"Accepted {card_name(before['top'])} and played face down.")
        if after["chosen_suit"]:
            detail.append(f"Continue with {SUITS[after['chosen_suit']]}, if accepted.")
        if not after["hand_counts"][actor]:
            detail.append(f"{possessives[actor]} hand is empty; the result is still pending.")
        elif not detail:
            detail.append("Played face down; awaiting a response.")
    elif before["phase"] == "response" and after["phase"] in ("turn", "finished"):
        if after["top_status"] == "Revealed":
            caught = after["top"] != before["top"]
            kind = "bluff_caught" if caught else "challenge_failed"
            title = (f"{names[actor]} caught a bluff." if caught else
                     f"{possessives[actor]} challenge was wrong.")
            recipient = 1 - actor if caught else actor
            requested = before["draw_penalty"] + 2
            detail.append(draw_text(recipient))
        elif after["top_status"] == "Accepted" and drawn[actor] and before["hand_counts"][actor]:
            kind = "draw"
            recipient, requested = actor, before["draw_penalty"] or 1
            title = draw_text(actor)
            detail.append(f"Accepted {card_name(before['top'])}; the turn ended.")
        elif after["top_status"] == "Accepted":
            kind = "accept"
            title = f"{names[actor]} accepted {card_name(before['top'])}."
            if not before["hand_counts"][actor] and before["draw_penalty"]:
                recipient, requested = actor, before["draw_penalty"]
                detail.append(draw_text(actor))
            elif before["skip_pending"] and any(before["hand_counts"]):
                detail.append(f"{possessives[actor]} turn was skipped under the ace.")
        else:
            return None
    elif before["phase"] == "turn" and after["phase"] in ("turn", "finished"):
        if any(drawn):
            kind = "draw"
            recipient = next(p for p in (0, 1) if drawn[p])
            requested = before["draw_penalty"] or 1
            title = draw_text(recipient)
            detail.append(f"{possessives[actor]} turn ended.")
        elif before["skip_pending"] and not after["skip_pending"] and before["hand_counts"] == after["hand_counts"]:
            kind = "skip"
            title = f"{names[actor]} skipped the turn."
            detail.append("The ace’s skip is cleared.")
        else:
            return None
    else:
        return None

    if recipient is not None and drawn[recipient] < requested:
        detail.append("No more cards were available; the unpaid penalty was cancelled.")
    if after["winner"] is not None:
        winner = after["winner"]
        detail.append("You win." if winner == perspective else "Your opponent wins.")
    elif recipient is not None and not before["hand_counts"][recipient] and drawn[recipient]:
        detail.append(f"{names[recipient]} returned to play.")
    return {"kind": kind, "actor": actor, "drawn": drawn, "winner": after["winner"],
            "title": title, "detail": " ".join(detail)}
