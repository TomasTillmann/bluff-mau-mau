# Blafovací Prší

<!-- impeccable:product-schema 1 -->

## Platform
web

## Purpose
Two-player Bluff Mau-Mau, backed by the Rust engine.

## Current scope
A local human-versus-bot game built from the approved component study. The default
page opens an opponent chooser with 1,335 runnable bots: 1,333 baseline
configurations, Tactical, and BeliefSearch. Fuzzy search covers names, strategies,
and parameter labels such as `B0 N100 C40`. Results show ratings, win rates, and
W/D/L records. Arena Elo comes from the frozen baseline tournament; test Elo comes
from a separate top-ten benchmark. Both use records saved on 13 September 2026;
playing in the browser never changes those ratings.

Selecting an opponent starts a new game with the human in the bottom seat
(player 0), moving first. The opponent's cards remain face down, and the server
plays the bot's moves automatically. New game deals another game against the
same bot; Choose opponent returns to the chooser. `?debug=1` opens the separate
debug table, where both hands are visible and the user controls both players.
The component showcase remains a separate reference.

## Confirmed requirements
Use the supplied Public Domain Deck from `free-playing-cards/`, shallow overlapping card fans, fixed Player 2 above Player 1, and readable card indices. Preserve the approved green cloth, paper controls, vermilion actions, and Bricolage lettering. Include draw/discard piles, effective declaration/status, pending effects, actual-card selection, all 32 declaration choices, queen continuation suits, Play, Play as itself, and Accept declaration buttons, pile draw/challenge actions, and recent public moves. An empty hand is provisional until the engine reports a winner. Player 1, the bottom seat, always starts new games. Only actionable piles animate: the draw pile draws, and the facedown discard challenges a pending claim. Selection keeps a response open. Submitting a play or drawing implicitly accepts in one engine move. Accept declaration appears while a response is pending; accepting an ace skips automatically. The starting card has no forced effect; normal matching and drawing remain available. An opening seven or K♠ contributes only when the first play starts a draw penalty. No separate draw, skip, or challenge panel buttons appear. Illegal declarations remain visible but dimmed and disabled. Keep pile counts; hide visible pile names, title/footer text, and status text in the declaration plaque. A played queen shows its continuing suit beneath the declared identity while that suit remains active.

Keep the latest MoveExplain result directly below Player 1’s hand. Explain the
last completed action from Player 1’s perspective using only public information,
with actual hand-count changes for draws and shortages. Preserve it through
selection and failed moves; clear it on reloads, new games, and debug presets.
Reloading the normal page returns to the chooser; choosing a bot starts a fresh
game. Loading the debug page starts a fresh debug game. Each mode has one shared
local table held in server memory, so tabs using the same mode share its table.
Debug operations cannot change the human-versus-bot game. Do not persist these
games in browser storage or on disk.

The “Play as itself” shortcut below the declaration grid uses the selected hand
card as its declaration. Start without a selected card, and enable the shortcut
only for an exact engine-legal move; queens require an explicitly chosen continuing
suit. Grid selections remain independent and keep the existing bluff action.

## Constraints
Use the engine’s legal moves as the source of truth. Every actual card can be used for any declaration returned by `move_generator`; never filter bluffs using card identity or location. All declaration tiles remain visible; illegal declarations are dimmed and disabled. Use native HTML/CSS/JavaScript and the Rust HTTP server. English labels and desktop-first composition with usable narrow layouts are current choices. Normal play exposes only the human hand and legal actions, public state, and the opponent's card count. Bot declarations stay hidden until a challenge reveals the actual card. This is a local game with one shared table per mode, without multiplayer sessions. Use a small, fast Playwright sanity suite in tests/integration for pointer actions, layout stability, and card access; keep its scope in that folder’s AGENTS.md. Continue visual verification through the Chrome extension and detailed logic checks in Rust/Node.
