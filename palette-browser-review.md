# Forms palette and loading review — 2026-10-06

Scope: the UI in c1f4278940, running at https://wolf-macro-google:35109/app/ with bundle app-CP0zzagX.js.

## Source review

The question palette reuses native buttons and the existing pointer/keyboard drag primitive. New columns are created only on a completed drop, and relation questions wait for the native table picker. Cancellation and a drop outside the canvas create no question. Labels and identifiers are typed; the new dispatch uses exhaustive matching. Loading retains a screen-reader status and withholds inputs until the shared layout is available. An obsolete builder response-navigation callback was identified and removed in the follow-up cleanup.

## Browser review

Own headless Chromium and Firefox, using the local stack and synthetic forms:

- Banner, separate left outline, centered canvas and right palette; icons precede labels, and the drag instruction hint is absent.
- Question insertion before an existing question, first question in an empty form, section insertion, keyboard activation, cancellation and relation-table selection.
- Touch drag added a date question. A final short-answer drag was saved and survived a reload.
- No horizontal overflow at 390 px. Native table picker owns focus on its first and repeated opening.
- A fresh browser context held the real collaboration request to inspect loading at desktop and mobile widths. Releasing it rendered all six saved questions, removed the loading status, and produced no page errors.
- Firefox completed Form → database banner link → Forms menu → Form on the final bundle with no page errors.
- An independent Sol audit repeated the Forms-menu path in Firefox. Local telemetry tied the user's crash to the older app-B35Ak4cO.js bundle; the current grouped menu passed.

## Checks

- Full web run: 13,753 passed, one skipped, 1,559 files.
- Forms after the loading change: 439 passed, one skipped, 43 files.
- TypeScript, Vite build, just check, cargo fmt, Kafka topic registry, and git diff check passed.
- New behavior regressions were first run red against the preceding implementation.

The broader feature's two prior convention/quality reviews and full-stack flow videos are on this proof branch. The preview preserves the older verified local backend and existing data; the current main database cutover was tested separately in an isolated database. Real external calendar invitation completion is still unverified because the local account has no connected calendar.
