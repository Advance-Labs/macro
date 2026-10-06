# Browser QA: collaboration, entity integration, channels

Date: 2026-10-06 UTC. Own Playwright Chromium process and isolated network namespace; never shared Chrome. Started against app-BS7lZjqp.js and repeated the final focus/channel checks on app-XJaNwXWL.js. No production source edits.

## Fixtures and safety

- Form: `01a10fb6-29ff-7e6a-9097-e2eef507b889`, final name `QA collaboration Oct6 database renamed`.
- Linked standalone database: `01a10fb6-29d2-79f5-a602-9e7e813fb3f5`.
- Private channel: `01a10fbf-803b-7ffd-a02c-799bb6dd6e94`, `QA collaboration Oct6 private`.
- Channel-created form: `01a10fc2-52fc-7fb6-9b91-e137ec60548b`, `QA collaboration Oct6 channel form`.
- Channel participants explicitly inspected: only `forms-owner@local.macro.test` and `forms-editor@local.macro.test`. All messages confined to this newly created private channel. No external messages, invitations, or calendar actions.
- Existing synthetic editor account refreshed through local login/onboarding; skipped all connections/team invites and continued as Guest. State saved privately (0600); no tokens exposed.

## Passed

- Create → Form: standalone form and database creation.
- Two independently authenticated contexts: section titles and descriptions, new questions, question renames, Required switch, new section, reorder via native menu all propagate.
- Concurrent edits to separate section fields preserve both changes. Simultaneous full replacement of one title converges characterwise in Loro (combined text, both clients identical), then a normal replacement cleans it up consistently.
- Peer selection avatar appears on selected question/section; header presence visible.
- Offline second editor changes section description while first editor independently changes another. Visible warning explains pending local changes. After reconnect, both changes converge; reloaded first editor recovers them.
- New bundle: keyboard focus on outline's Company name button survives a remote section-description update.
- Form rename updates standalone database; database rename updates form live in other editor.
- Four form → linked database → Forms-menu return cycles without crash or Kobalte error.
- Native Share grants synthetic second user Edit. Non-owner editor sees disabled audience controls and owner-only explanation.
- Anyone with link opens anonymously through /respond; anonymous editor URL renders view-only respondent page, no builder.
- Drive lists form and database with distinct icons. Quick Access search finds both correctly.
- /poll native dialog validates/creates in private channel. Referenced peer receives `can respond`; owner `can edit`. Vote updates live and changing vote moves the one existing vote rather than incrementing total. Results table shows selected option, count, percent.
- @ mention search includes form; inserted mention has form block identity; sending and opening the mention works in the peer's split.
- /form opens builder beside channel; saving first question inserts draft card. Send card, peer fills it inline and receives receipt.
- Edit my response updates embedded answer; owner Responses shows 1 response and 1 table row with updated value.
- Opening channel-referenced form as peer renders Viewer response receipt; no Build or Responses tab.
- Zero pageerror events in both authenticated contexts throughout tested flows.

## Finding

Low severity UI consistency: poll option row says `1 votes` while footer and expanded result table correctly use `1 vote`. Repro: /poll → two choices → peer votes. Reported to root for polish.

## Feature flag audit (read-only)

`enableForms` is defined with `defineFlag`, key `enable-forms`, env `ENABLE_FORMS`. Authoring call sites cover form builder, Create launcher, database controls, Drive filters, Quick Access, activity, and channel slash actions. Form-block tests explicitly cover unresolved flag, disabled owner, and respond route under enabled flag. Global config left unchanged; flag-off browser run not performed to avoid affecting the active preview.

## Limits

Only Chromium in this task; other agents handle cross-browser/respondent/drag coverage. No real calendar connected, so calendar invitations not exercised here. Did not claim every possible concurrency or poll combination is exhaustive.

## Evidence

- `two-editor-presence.png`
- `offline.png` (visible pending sync warning)
- `drive-entries.png`
- `quick-access.png`
- `poll-results.png`
- `channel-form-response.png`
- `channel-response-grid.png`
