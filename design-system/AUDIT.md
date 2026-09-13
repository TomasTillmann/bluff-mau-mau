# Component audit

**Verdict: ship the component study.** All requested components are present. No unresolved material defects found in the reviewed source or final screenshots.

The independent review covered `DESIGN.md`, component CSS, presentation HTML/CSS, the reference image, and five final Chrome captures: desktop full page, desktop table, medium table, mobile table, and mobile controls. The interaction and test results below were supplied by the root agent's browser run.

| Reference components | Implementation | Coverage |
| --- | --- | --- |
| Brand, suit mark, topbar | `.bp-brand*`, `.bp-topbar*` | Complete |
| Debug indicator, new game | `.bp-debug`, compact secondary button | Complete |
| Player names, counts, active state | `.bp-hand-label*` | Complete |
| Face, back, mini, selected cards | `.bp-card*`, `aria-pressed` | Complete |
| Both shallow five-card fans | `.bp-hand`, five defined positions | Complete |
| Draw/discard piles, counts | `.bp-piles`, `.bp-pile*` | Complete |
| Green table, oval boundary | `.bp-table`, `::before` | Complete |
| Declaration plaque, Accepted | `.bp-declared*`, `.bp-status` | Complete |
| Effects text and divider | `.bp-effects` | Complete |
| Action heading, helper, actual-card field | `.bp-action-panel*`, `.bp-field*`, `.bp-actual*` | Complete |
| Four suits × eight ranks, selection | `.bp-declarations`, `.bp-declaration` | All 32 enabled and instantiated |
| Play summary, penalty, primary/secondary actions | `.bp-play-summary`, `.bp-penalty`, `.bp-actions` | Complete |
| History rows, status, rules, footer mark | `.bp-history*`, `.bp-status`, `.bp-footer` | Complete |

## Resolved findings

- Informative suits in the declaration plaque, play summary, history, and dynamic `suitMark()` output now have `role="img"` and readable suit names. Rechecked in final source.
- Static selected cards retain natural stacking so adjacent rank corners remain visible. Confirmed in desktop, medium, and mobile captures.
- Narrow-page overflow was fixed through shrinkable grid columns and an earlier stacking breakpoint. Final captures are contained; the declaration matrix intentionally scrolls within its own wrapper on mobile.

## Verification evidence

`design-system/check.js` passed in Chrome: 21 component selectors; all 32 declaration clicks with exclusive selection; keyboard selection in both five-card hands; and specimen reset. At 1440, 1000, and 390px, document width equalled viewport width. Declaration target widths were 48.75, 51.25, and 44px respectively, with 44px height. Fonts loaded, broken images were zero, reduced-motion transitions were 0s, and console errors, warnings, and failed requests were zero.

The supplied SVG deck is used directly from the initialized `free-playing-cards` submodule, preserving complete artwork and 5:7 proportions. The typography detector advisory was a false positive: measured heading sizes were 96/46/38px against 14px helper text. The paper-palette advisory reflects the intentional green-and-paper direction.

This component audit did not review game-engine behavior. Five-card-only fan positions and the absence of game integration are intentional limits of the requested component specimen.
