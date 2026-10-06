# Builder panels, question menu, and required controls

Revision: `7427feccb20fbd65a44b112c568e9e05bf55a27f`.
Browser checks: 2026-10-06, against the local HTTPS stack.

## Changes reviewed

- Separate outline, centered canvas, and question palette panels. The palette and menus share Text & files, Choices, Numbers & dates, and Linked items categories.
- Form flow separately holds Section, Screener, and Booking. Existing fields are available through From database.
- Selected outline entries identify where a click adds a question. The menus show the current type and preserve the native relation-table submenu.
- Required questions have a badge below their answer preview. Editing puts the Required switch in the footer. Respondents' required-field guidance appears below the questions; native required validation and accessible labels remain intact.
- Shared Button, Dropdown, Badge, and ToggleSwitch primitives and semantic colors; no new dependencies, synchronization effects, or persistence changes.

## Browser verification

Own headless Chromium and Firefox; no shared Chrome session.

- Native empty-section Add question menu: grouped options, Date insertion, current-type marker, and Escape dismissal.
- Palette: pointer Number insertion, keyboard Date insertion, Escape cancellation without a new question, and touch long-press Checkbox insertion. Each completed gesture created one question.
- Changes appeared in Firefox and survived reload. The Required switch persisted after reload.
- Desktop, medium, mobile, and touch-tablet layouts were inspected. No horizontal overflow at widths 1440, 1024, and 390.
- Anonymous respondent required guidance is below the questions. Continuing without a required answer preserves validation.
- Both browsers reported no page errors. Connection-gateway and collaboration sockets exchanged frames.

The pointer harness initially measured a target during outline smooth scrolling. Scrolling to a stable position before measuring confirmed the expected drop indicator and successful insertion. Kobalte's hidden switch input is operated through its visible label in these checks.

## Startup meeting screener example

A separate test form was built through the UI with three required questions, one Screener, and an existing Booking link. The rules use And:

| Question | Rule |
| --- | --- |
| Company type | is any of Startup |
| Annual recurring revenue (USD) | is greater than 100000000 |
| Employee count | is 12 |

- ARR of exactly 100000000 stopped the respondent.
- ARR 100000001 with 13 employees stopped the respondent.
- Other company with ARR 100000001 and 12 employees stopped the respondent.
- Startup with ARR 100000001 and 12 employees was accepted by the server, created one response/table row, and revealed the embedded booking picker.
- The public form read omitted the booking destination.
- Direct server submissions with the ARR boundary or 13 employees returned `stopped` without a booking target. Omitting employee count returned HTTP 400 `missingAnswer`.
- The Responses grid contained only the passing response, with all three values intact.

The local test account has no connected calendar. Actual availability and an external calendar invitation remain unverified; the existing direct booking URL is still independently usable.

## Automated checks

- Tests-first run: four expected failures before implementation.
- Forms suite: 442 passed, one skipped, 43 files.
- Focused final menu/builder/respondent suite: 38 passed.
- TypeScript, production Vite build, `just check`, `cargo fmt --all -- --check`, `cargo x kafka-topics --check`, and `git diff --check` passed.
- CI is tracked on PR #7478. The preceding revision `3afc530b1d` was fully green.

The frontend was installed atomically at https://wolf-macro-google:35109/app/ with older hashed assets retained. The preserved local backend and stored user data were not reset or migrated.
