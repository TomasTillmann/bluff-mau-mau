# Blafovací Prší

<!-- impeccable:product-schema 1 -->

## Platform
web

## Purpose
Two-player Bluff Mau-Mau, backed by the existing immutable Python engine.

## Current scope
A playable local debug table built from the approved component study. Both hands are visible and the user makes decisions for both players. The component showcase remains a separate reference.

## Confirmed requirements
Use the supplied Public Domain Deck from `free-playing-cards/`, shallow overlapping card fans, fixed Player 2 above Player 1, and readable card indices. Preserve the approved green cloth, paper controls, vermilion actions, and Bricolage lettering. Include draw/discard piles, effective declaration/status, pending effects, actual-card selection, all 32 declaration choices, queen continuation suits, play/draw/skip/accept/challenge actions, and recent public moves. An empty hand is provisional until the engine reports a winner. Player 1, the bottom seat, always starts new games. Only actionable piles animate: the draw pile draws, and the facedown discard challenges a pending claim. Choosing another gameplay action accepts the claim first, then follows the engine’s returned state. Keep pile counts; hide visible pile names, title/footer text, and all status or continuing-suit text in the declaration plaque.

Keep a persistent MoveExplain result directly below Player 1’s hand. Explain the
last completed action from Player 1’s perspective using only public information,
with actual hand-count changes for draws and shortages. Preserve it through
reloads, selection, and failed moves; clear it on new games and debug presets.

The “Play as itself” shortcut below the declaration grid uses the selected hand
card as its declaration. Start without a selected card, and enable the shortcut
only for an exact engine-legal move; queens require an explicitly chosen continuing
suit. Grid selections remain independent and keep the existing bluff action.

## Constraints
Keep the engine and its rules unchanged. Every actual card can be used for any declaration returned by MoveGenerator; never filter bluffs using card identity or location. All declaration tiles remain selectable, with an explanation when the selected declaration cannot be submitted under the current engine rules. Use native HTML/CSS/JavaScript and a Python standard-library server. English labels and desktop-first composition with usable narrow layouts are current choices. This iteration is a local shared debug game; normal play with a hidden opponent hand, bots, and multiplayer are outside this UI iteration. No Playwright UI test files: verify interactions manually through the Chrome extension and cover bridge logic with Python checks.
