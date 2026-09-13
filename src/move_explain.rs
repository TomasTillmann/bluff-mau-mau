//! Public explanations derived only from adjacent public snapshots.
use crate::game::{Card, Suit};
use serde::Deserialize;
use serde_json::{Value, json};

pub fn suit_name(suit: Suit) -> &'static str {
    match suit {
        Suit::H => "hearts",
        Suit::D => "diamonds",
        Suit::C => "clubs",
        Suit::S => "spades",
    }
}

pub fn card_name(card: Card) -> String {
    let rank = ["7", "8", "9", "10", "jack", "queen", "king", "ace"][card.rank() as usize];
    format!("{rank} of {}", suit_name(card.suit()))
}

#[derive(Deserialize)]
struct Snapshot {
    phase: String,
    turn: usize,
    hand_counts: [usize; 2],
    top: Card,
    chosen_suit: Option<Suit>,
    draw_penalty: u32,
    skip_pending: bool,
    winner: Option<usize>,
    top_status: String,
}

/// The default UI perspective is player zero. Hidden cards never enter this function.
pub fn explain_move(before: Option<&Value>, after: &Value, perspective: usize) -> Option<Value> {
    let before = before?;
    if before == after {
        return None;
    }
    let before: Snapshot = serde_json::from_value(before.clone()).ok()?;
    let after: Snapshot = serde_json::from_value(after.clone()).ok()?;
    let actor = before.turn;
    if actor > 1 || perspective > 1 {
        return None;
    }
    let drawn = [
        after.hand_counts[0].saturating_sub(before.hand_counts[0]),
        after.hand_counts[1].saturating_sub(before.hand_counts[1]),
    ];
    let names = [
        if perspective == 0 {
            "You"
        } else {
            "Your opponent"
        },
        if perspective == 1 {
            "You"
        } else {
            "Your opponent"
        },
    ];
    let possessives = [
        if perspective == 0 {
            "Your"
        } else {
            "Your opponent’s"
        },
        if perspective == 1 {
            "Your"
        } else {
            "Your opponent’s"
        },
    ];
    let draw_text = |player: usize| {
        format!(
            "{} drew {} card{}.",
            names[player],
            drawn[player],
            if drawn[player] == 1 { "" } else { "s" }
        )
    };
    let mut detail = Vec::new();
    let mut requested = 0;
    let mut recipient = None;
    let kind;
    let title;
    if after.phase == "response" && after.hand_counts[actor] < before.hand_counts[actor] {
        kind = "play";
        title = format!("{} declared {}.", names[actor], card_name(after.top));
        if before.phase == "response" {
            detail.push(format!(
                "Accepted {} and played face down.",
                card_name(before.top)
            ));
        }
        if let Some(suit) = after.chosen_suit {
            detail.push(format!("Continue with {}, if accepted.", suit_name(suit)));
        }
        if after.hand_counts[actor] == 0 {
            detail.push(format!(
                "{} hand is empty; the result is still pending.",
                possessives[actor]
            ));
        } else if detail.is_empty() {
            detail.push("Played face down; awaiting a response.".into());
        }
    } else if before.phase == "response" && matches!(after.phase.as_str(), "turn" | "finished") {
        if after.top_status == "Revealed" {
            let caught = after.top != before.top;
            kind = if caught {
                "bluff_caught"
            } else {
                "challenge_failed"
            };
            title = if caught {
                format!("{} caught a bluff.", names[actor])
            } else {
                format!("{} challenge was wrong.", possessives[actor])
            };
            let player = if caught { 1 - actor } else { actor };
            recipient = Some(player);
            requested = before.draw_penalty as usize + 2;
            detail.push(draw_text(player));
        } else if after.top_status == "Accepted"
            && drawn[actor] > 0
            && before.hand_counts[actor] > 0
        {
            kind = "draw";
            recipient = Some(actor);
            requested = before.draw_penalty.max(1) as usize;
            title = draw_text(actor);
            detail.push(format!(
                "Accepted {}; the turn ended.",
                card_name(before.top)
            ));
        } else if after.top_status == "Accepted" {
            kind = "accept";
            title = format!("{} accepted {}.", names[actor], card_name(before.top));
            if before.hand_counts[actor] == 0 && before.draw_penalty > 0 {
                recipient = Some(actor);
                requested = before.draw_penalty as usize;
                detail.push(draw_text(actor));
            } else if before.skip_pending && before.hand_counts.iter().any(|&count| count > 0) {
                detail.push(format!(
                    "{} turn was skipped under the ace.",
                    possessives[actor]
                ));
            }
        } else {
            return None;
        }
    } else if before.phase == "turn" && matches!(after.phase.as_str(), "turn" | "finished") {
        if let Some(player) = drawn.iter().position(|&count| count > 0) {
            kind = "draw";
            recipient = Some(player);
            requested = before.draw_penalty.max(1) as usize;
            title = draw_text(player);
            detail.push(format!("{} turn ended.", possessives[actor]));
        } else if before.skip_pending
            && !after.skip_pending
            && before.hand_counts == after.hand_counts
        {
            kind = "skip";
            title = format!("{} skipped the turn.", names[actor]);
            detail.push("The ace’s skip is cleared.".into());
        } else {
            return None;
        }
    } else {
        return None;
    }
    if recipient.is_some_and(|player| drawn[player] < requested) {
        detail.push("No more cards were available; the unpaid penalty was cancelled.".into());
    }
    if let Some(winner) = after.winner {
        detail.push(
            if winner == perspective {
                "You win."
            } else {
                "Your opponent wins."
            }
            .into(),
        );
    } else if let Some(player) =
        recipient.filter(|&player| before.hand_counts[player] == 0 && drawn[player] > 0)
    {
        detail.push(format!("{} returned to play.", names[player]));
    }
    Some(
        json!({"kind": kind, "actor": actor, "drawn": drawn, "winner": after.winner, "title": title, "detail": detail.join(" ")}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot() -> Value {
        json!({"phase":"response","turn":1,"hand_counts":[1,2],"top":"7H","chosen_suit":null,"draw_penalty":6,"skip_pending":false,"winner":null,"top_status":"Awaiting response"})
    }
    #[test]
    fn challenge_shortage_and_winner_text_match_public_contract() {
        let before = snapshot();
        let mut after = before.clone();
        after["phase"] = json!("finished");
        after["top"] = json!("JC");
        after["top_status"] = json!("Revealed");
        after["hand_counts"] = json!([2, 2]);
        after["winner"] = json!(1);
        let result = explain_move(Some(&before), &after, 0).unwrap();
        assert_eq!(
            result,
            json!({"kind":"bluff_caught","actor":1,"drawn":[1,0],"winner":1,"title":"Your opponent caught a bluff.","detail":"You drew 1 card. No more cards were available; the unpaid penalty was cancelled. Your opponent wins."})
        );
        assert!(explain_move(None, &after, 0).is_none());
        assert!(explain_move(Some(&before), &before, 0).is_none());
    }
    #[test]
    fn queen_direct_response_uses_public_declaration_and_fixed_perspective() {
        let before = snapshot();
        let mut after = before.clone();
        after["hand_counts"] = json!([1, 1]);
        after["top"] = json!("QS");
        after["chosen_suit"] = json!("C");
        let result = explain_move(Some(&before), &after, 1).unwrap();
        assert_eq!(result["title"], "You declared queen of spades.");
        assert_eq!(
            result["detail"],
            "Accepted 7 of hearts and played face down. Continue with clubs, if accepted."
        );
    }
}
