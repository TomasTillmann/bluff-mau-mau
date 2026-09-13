# UI integration sanity checks

Keep this suite small, fast, and practical. These are Playwright sanity checks,
not exhaustive gameplay tests. Cover real pointer hover/click behavior, reachable
cards, and stable layout during a few ordinary selections. The Rust engine
and Node unit checks own detailed rules and combinations.

Run from the repository root:

```sh
npm --prefix tests/integration ci
npm --prefix tests/integration test
```

Google Chrome, Node.js, and Rust/Cargo must be installed. Use `test:headed`
instead of `test` to watch Chrome. The suite starts its own local game server on
port 18767 and stops it afterward; do not run it against the user's game on 8767.
Run serially because the debug server has one shared game. Each debug page load at `/?debug=1` deals
through POST /api/new. Seed that real startup request with route.continue and
postData; keep the server response real. Load stress hands by redirecting only
the first startup request to the real /api/debug/max-hand endpoint with
route.continue. Later reloads must still call /api/new and reset the game.
Normal-play checks load `/`, choose a bot, and use the real `/api/play/*` APIs.
Do not inject deal seeds into normal play; the server keeps those private.
Do not mock legal moves or replace app event handlers.

Test rendered behavior, not the presence of CSS declarations. Use real hover
and clicks without `force`, including the visible edges of a card stack. Check
both available actions and inert states. For layout compare document
coordinates (bounding-rectangle top plus scrollY), so whole-panel movement fails
without confusing locator auto-scrolling with app reflow. Wait
for fonts/images and observable state changes; avoid arbitrary sleeps, pixel
snapshots, long random games, exhaustive matrices, or new helper frameworks.
Use a small pixel tolerance for geometry. Add a scenario only for a concrete
regression or important interaction, and keep the suite under roughly 30 seconds.

Keep generated `node_modules`, `test-results`, and reports out of git.
