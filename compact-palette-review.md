# Compact question palette review

Revision: `3afc530b1d` · 6 October 2026

## Change and conventions

The question palette uses one panel, native ghost buttons in two columns, and a divider before structure controls. Icons remain left of full, single-line labels. No drag handlers, permission checks, data mutations, or dependencies changed. The loading skeleton and Forms agent guide match the new density.

Compared with the previous build in Firefox at 1440 × 1000, the panel shrank from 715.6 px to 452.2 px (37%). All 14 question types remain visible. Desktop buttons are 32 px high; native touch sizing gives 36 px targets.

## Browser verification

Own headless Chromium and Firefox against `https://wolf-macro-google:35109/app/`, using the local backend and a dedicated test form:

- All 14 labels fit fully, with every icon to the left, in both browsers.
- Dragging Short answer into the canvas created exactly one question at the insertion line.
- Clicking Paragraph and keyboard Enter on Date each inserted one question.
- Dragging Section created one section; Escape during a Number drag created nothing.
- A Chromium touch long-press and drag inserted Link; the question survived reload.
- Changes appeared in the other browser, and the saved dragged question survived reload.
- At widths 1440, 1024 and 390, the page had no horizontal overflow. The mobile palette had no clipped labels.
- The existing Workshop booking request form loaded with the compact palette.
- No page errors in either browser.

The first cross-browser text assertion needed `.first()` because the question appears in both the outline and canvas; correcting that test selector confirmed persistence.

## Automated checks

- Forms suite: 439 passed, 1 skipped, 43 files.
- Final builder/drag/title-card rerun: 37 passed, 3 files.
- TypeScript, final Vite build, `just check`, `cargo fmt --all -- --check`, `cargo x kafka-topics --check`, and `git diff --check` passed.
- Existing repository warnings remained non-failing. CI for the new revision was triggered by the push; the preceding revision had fully green CI.

The new frontend was installed atomically without restarting services or changing stored form data outside the dedicated test form. Existing hashed assets remain available to open tabs.
