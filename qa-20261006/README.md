# Final Forms verification — October 6, 2026

Code: `ef3332618e`, rebased on `f381296fde` (latest main at push). Local frontend: `app-CsdFMqdk.js`, served at https://wolf-macro-google:35109/app/ against preserved local services. CI status is recorded on PR #7478.

## Review and regression tests

Five final reviews passed with no remaining issues: code quality, simplicity, consistency, robustness, and scope. Review findings were reproduced in failing regression tests before fixes. The 44-file Forms test suite now passes 455 tests with one skipped. TypeScript, the Vite build, `just check`, Rust formatting, Kafka topic generation check and `git diff --check` passed. Rust Forms and models tests passed 222 tests, one ignored, with SQLX_OFFLINE unset against an isolated database migrated to current main. No live preview database reset or destructive cutover was performed.

The changes preserve outline focus during saves and remote edits; keep Build and Share pending writes alive across tabs so Preview waits for publication; insert new questions before a final Booking section; retain drafts during transient background refresh failures while dropping revoked or missing detail; support submission from valid empty/booking-only forms; and retry unreadable existing receipts without another submission. Stopped-submission counts now explain their signed-in ledger scope, Preview completion copy is shorter, and single-vote poll labels use the singular.

The Forms Rust boundary was reviewed: authorization and business policy remain in the domain, inbound adapters consume typed outcomes, and cross-crate database access goes through domain ports/models.

The first full review and browser round used `a253893b51` / `app-Brj45bfH.js`. Main then advanced with agent-session changes. The second rebase changed no Forms source; all local checks and 455 web /222 Rust tests passed again. An additional browser smoke on `ef3332618e` / `app-CsdFMqdk.js` verified Preview, pointer insertion/cancellation, persistence and database navigation with no page errors. The document-storage OpenAPI test binary also compiled successfully locally while investigating an earlier diagnostic-free CI compiler exit.

## Independent browser coverage

Three browser agents used their own headless browsers against the local backend. Reports cover all question types, click/pointer/touch/keyboard insertion and reordering, cancellation and invalid screener moves, responsive layout, native menus/sharing, required validation, AND/OR screeners and numeric boundaries, public and invited audiences, submission/edit/closure, CSV/grid counts, two-user Loro collaboration/offline recovery/presence, both rename directions, repeated form/database navigation, Drive and Quick Access, channel forms/polls/tallies/mentions, viewer/editor boundaries and feature-flag wiring.

Final targeted rechecks used `app-Brj45bfH.js`: real delayed metadata and database writes held Preview until completion across tabs; booking-only requests did not reveal the picker before the real server accepted; transient503 detail reads preserved drafts and recovered from real form_changed events;403/404 removed cached content. Preview created no submission, upload or booking writes. See builder/report.md and respond/report.md.

## Additional root browser verification

Own headless Firefox; no shared Chrome. Zero pageerror events.

- Actual file upload: `forms-qa-oct6.txt` uploaded through the local static-file service, response accepted, authenticated file download returned the exact content, and receipt/file link survived a full reload. Fixture `01a10fbc-6f99-73dd-a2be-b496ce00bc94` now has one signed-in response. Evidence: respond/real-file-upload-receipt.png.
- Receipt recovery: fault-injected503 for only that browser's GET /responses/mine; attempted POST received real AlreadyResponded refusal; the page displayed an explicit retry. Removing the fault and clicking Try again restored the existing receipt and file link. Observed exactly one POST and three GETs across initial read, failed receipt read, and retry. Evidence: respond/receipt-retry-before.png and receipt-retry-after.png.
- Conversion: fixture `01a10fd5-6760-7403-92d4-82473ab33c7a`, QA Oct6 conversion and relation. Submitted text 'not a number'. Changing type to Number refused and offered Convert into a new question. Conversion created a Number column and kept the original text column/value. Edited the same response to numeric123 and selected its existing response row through Database row. Receipt survived reload; grid retained original text,123 and relation, with one response/row. Evidence: conversion-refusal.png, conversion-preserved-original.png, relation-receipt.png, conversion-and-relation-grid.png.
- Existing startup screening demo loaded on the final bundle, showing Startup AND ARR >100000000 AND employee count12 before Booking. Evidence: startup-demo-final.png.

## Practical limits

No calendar account is connected locally, so actual calendar invitation/rescheduling/cancellation was not exercised. The local picker shows its readable availability error. Touch coverage used Chromium emulation, not physical iOS hardware. Feature-flag off/loading behavior is covered by component tests and call-site review; the shared running preview remains enabled. The preserved preview backend predates main's database-entity cutover; current backend source and migrations were tested separately in an isolated database and by final-head CI. No production deployment, data reset, external calendar invitation, or messages to real users were performed.
