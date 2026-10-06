# Forms respondent browser QA — October 6, 2026

Tested live local HTTPS https://wolf-macro-google:35109/app/ with own headless Firefox, bundle from 7427feccb2. No production files, user forms, backend runtime, database configuration, or shared browser touched. No page errors in tested flows.

## Fixtures

- `01a10fb5-256a-78a0-8d98-df12153ef1fa` — QA respond Oct6 qualification. Created through UI. Company type, ARR, employee count required; screener; native booking link. Final state public, automatically closed at October 5, with OR rules and three accepted responses (two anonymous, one signed in). Owner's accepted response edited once without creating extra row. Do not use as demonstration of original AND configuration without restoring it.
- `01a10fbc-6f99-73dd-a2be-b496ce00bc94` — QA respond Oct6 answer controls. Invited audience; two sections with 11 question types; no real responses.

## Passed

- Native Share > Manage access includes Who can respond; changed to Anyone with the link there and anonymous route opened without login. Invited-only anonymous route asks sign in.
- Empty required company/ARR/employee answers show three inline validation errors. Required legend appears beneath questions.
- AND screener: Startup + ARR=100000000 + employees12 fails strict greater-than boundary. ARR100000001 + employees13 fails equality. Corrected Startup/100000001/12 succeeds. Check my answers preserves entered values. Rules and predicates do not render on stop page.
- Public form details omit booking destination. Failed screens do not reveal destination. Accepted server response returned booking destination and then native time picker appeared. No booking request occurred before acceptance.
- OR screener: Enterprise + ARR1 + employees12 passes when any condition suffices.
- Signed-in public respondent retains identity, sees receipt after refresh, edits through Update response. POST once, PUT once, row count unchanged by edit. Anonymous response has empty Respondent.
- Responses counts and embedded table match three accepted rows overall. CSV downloaded successfully and reflected updated ARR200000000 for signed-in row; no duplicate row after editing.
- Manual closure blocks anonymous form; signed-in return retains saved receipt, shows closed reason, removes Edit my response. Setting Close automatically to past time also closes form.
- Preview tested booking path and 11 response controls. No responses/upload/booking writes observed. Preview booking advertises no booking. File input in preview creates preview-only answer with no upload network request.
- Short text, paragraph/newline, URL, multiple choice, checkboxes, dropdown, single checkbox, number, date, datetime, person picker and document picker work. Invalid URL blocks Next with inline error. Person/document selections render correctly in receipt. Back/Next retains entered text, multiline paragraph, selected options and checkbox.
- Public audience refused for form with file-upload question, clear error: change/remove file questions first. Audience stays invited.
- Preview receipt renders all test values correctly.

## Findings / polish

1. No blocking functional defect found in this pass.
2. Stopped-by-screener tile scope: normal UI failures intentionally stay local per RFC02 §4; no answers sent and no ledger record, including signed-in failure. Only server-submitted signed-in failures count. Root is clarifying tile copy; do not change local behavior.
3. Low-priority duplicate preview completion copy: title 'Preview complete.' followed by 'Preview complete. No response was saved.' in views/respond-view.tsx:191 and :201. Consider detail just 'No response was saved.' Screenshot preview-receipt.png.
4. Native booking unavailable in this fixture because test owner has no connected calendar. Selecting date yields readable 'Availability could not be loaded. Please retry or contact the host.' and Refresh, expected503. Actual booking/invitation, reschedule/cancel NOT verified. No real external invite sent.

## Not covered in this pass

- Real signed-in file upload and download (preview-only exercised).
- Database-row relation selection, polls/channel cards, editor permissions, collaborative simultaneous editing (other QA agent scope).
- Mobile browser/responsive layouts and physical touch (other QA agent scope).
- Current rebased bundle smoke: this pass ran original live bundle7427feccb2.
- Tampered HTTP invalid audience payloads; UI validation and expired close time exercised.

## Evidence

- screener-boundary-failure.png
- accepted-booking.png
- preview-receipt.png
- responses.csv (fixture data only)

## Browser handoff

Own headless Firefox driver is still running in exec session57542, script `/home/wolf/tmp/forms-qa-oct6/respond/browser.mjs`. No Docker helper needed. Variables `browser`, `context`, `page`, `fs`, `errors`, `sockets`; globalThis `respond` (anonymous), `ownerRespond`, `preview` (booking), `controlsPreview` (all-type preview), `controlsForm`, `ownerRequests`, `responses`, `previewWrites`, `controlWrites`. `page` is on second fixture Share tab. Send JSON line {"code":"..."}; stop with {"stop":true}. Do not print sockets raw URLs or context storageState. All page errors currently []. Keep own session for final new-bundle smoke, then close.

## Final rebased-bundle smoke

Reloaded after install; verified `/app/app-XJaNwXWL.js`. Responses screen renders 'Stopped submissions' and 'Signed-in respondents only' correctly, 3 responses/3 rows; no page errors. Captured `final-responses.png`. Own browser closed with stop command after this smoke; session57542 no longer needed.

## Targeted final QA — a253893b51 / app-Brj45bfH.js

Own headless Firefox on local HTTPS, verified new bundle path before testing. Created own fixtures only:

- `01a10fd5-0858-76a8-9c1b-8a74e3b27e8f` — QA respond Oct6 recovery. One optional short answer, no submissions.
- `01a10fd6-2906-75e9-8c4e-dd7e86b63bd2` — QA respond Oct6 booking only. Removed initial question section; layout has only booking step, zero questions. Public, one anonymous accepted response, zero real bookings.

Passed:

1. Filled draft answer 'Please preserve my draft 91827' in respondent tab. Injected503 only for that tab's detail GET. A real metadata change in editor tab caused form_changed and failed background refetch. Form remained rendered and unsent answer remained intact.
2. Removed503 route, made another real metadata change. Respondent automatically refetched200, displayed changed description and retained same draft. No manual reload or synthetic socket message.
3. Injected403 detail GET and triggered another real metadata write. Form title, fields and draft controls disappeared; proper access-denied screen rendered. Removed fault and another metadata event recovered actual form.
4. Repeated with404 against already-loaded detail. Proper nonexistent-form screen rendered; title and answer controls absent. Sensitive cached detail was not rendered after terminal access/missing failures.
5. Booking-only form renders Continue to booking with zero question controls, no picker before action. Paused actual submission POST with a promise barrier: while awaiting server, picker stayed absent. Released request; actual backend returned200/outcome submitted/booking target, then picker appeared. Exactly one POST, one response and one table row.
6. Booking-only Preview renders Continue to booking, opens preview picker, shows no-booking guidance. No response/upload/booking writes observed; count stayed one real response/row after preview.

No page errors. No stack restart, DB direct mutation, migrations, production-code edits, existing fixture mutation, or actual calendar booking. Browser closed after testing.

Evidence: transient-retains-draft.png, refetch-recovery-draft.png, forbidden-clears-detail.png, notfound-clears-detail.png, booking-only-before.png, booking-only-accepted.png, booking-only-preview.png.
