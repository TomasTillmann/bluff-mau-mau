# Blafovací Prší component study

A static, interactive specimen of the components in the supplied reference. It uses the shared styles in `components.css`; `showcase.css` only arranges the presentation. The design conventions and full component inventory are in `../DESIGN.md`.

After cloning this repository, initialize the supplied card assets with `git submodule update --init free-playing-cards`.

From the repository root:

```sh
python3 -B -m http.server 8766 --bind 127.0.0.1
```

Open [the specimen](http://127.0.0.1:8766/design-system/). Select either hand's cards, choose any of the 32 declarations, and use **New game** to reset the examples. The page acknowledges action-button presses locally; it does not run the game, validate moves, or change engine rules.

With the existing `bluff-ui` Playwright CLI Chrome session open at that URL, run the browser check from the repository root:

```sh
playwright-cli -s=bluff-ui run-code --filename=design-system/check.js
```

The supplied Public Domain Deck remains under `../free-playing-cards/` with its CC0 license. The self-hosted Bricolage Grotesque font is under `fonts/` with its SIL Open Font License. No dependencies or build step are required.
