# Forms builder browser QA — 2026-10-06

Browser: own headless Chromium, own storage contexts; no shared Chrome. URL: https://wolf-macro-google:35109/app/. Initial bundle app-BS7lZjqp.js / 7427feccb2; final verification app-XJaNwXWL.js (root's rebased build plus outline fix). No production edits, commits, migrations, backend restarts, emails or invitations.

Fixtures created and retained:
- QA builder Oct6 all types: /app/form/01a10fb5-4afa-7046-b30f-6840762b83f4; linked database 01a10fb5-4ac1-7a30-9716-54e18a91eae3.
- QA builder Oct6 drag and layout: /app/form/01a10fb9-e6f7-7d3b-b8d2-8044203485e3.

## Confirmed finding, now fixed and reverified

First outline click after changing a question title could scroll but leave the old question selected. Repro: select question, fill title, immediately click another outline question. Second click worked. Root implemented stable Key rendering for question and section identity after red regression tests. New bundle first-click reproduction passed. Additional two-context check: keep Annual recurring revenue outline button focused while second context renames the section; first context retained focus on the same outline button. Remote insertion and rename arrived live.

## Passed

- Create new form, empty state, initial question insertion; banner name and description, section title/description, question title and help text. Changes persisted across reload.
- All 14 question types inserted through right palette: Short answer, Paragraph, File upload, Link, Multiple choice, Checkboxes, Dropdown, Checkbox, Number, Date & time, Date, Person, Document, Database row. Database row chose Responses in native table selector.
- Options renamed, appended with Enter, removed, and color picker opened/dismissed. Duplicate kept options. Remove from form kept column; From database re-added it. Multiple choice -> Dropdown changed presentation without losing options.
- Required control toggled; required validation prevented preview submit with missing short answer. Preview filled short text, multiline, URL, radio, multiselect, dropdown, boolean, number, datetime, date, person, document and preview file. Preview completed; Responses tile stayed 0. Row selector showed correct empty state when source table had no rows.
- Grouped type menu contained Text & files, Choices, Numbers & dates, Linked items; current type and native Escape dismissal worked.
- Palette pointer drag inserted exactly one question at requested position. Escape and outside drop inserted none and cleared drag preview.
- Pointer drag of an existing question reordered it. Pointer drag of an existing section reordered it.
- Palette Section and Screener drag each inserted one at the line.
- Keyboard question Space / Up / Space reordered; Space / Down / Escape preserved order. Keyboard Enter on palette added a checkbox to the selected outline section.
- Screener condition editor: ARR > 100000000 saved. Attempt to keyboard-drag that question after the screener displayed a refused drop line with the exact screener/question reason and preserved order.
- Touch context with real CDP touch events: long press right-palette Link, move to canvas, release -> one question. Touch cancel -> no addition. Touch insertion synchronized to other context.
- Form -> linked database -> Forms button -> same form passed four loops with no pageerror. Native menu rendered without previous Kobalte context crash.
- 1440 desktop: outline left, centered canvas, palette right. 1024 desktop: canvas and palette adjacent. 390 desktop narrow: palette below canvas. Document scrollWidth equaled viewport width at 1024 and 390. Long question title wrapped; palette labels and icons fit. Touch/tablet native shell also tested.
- No captured pageerrors in primary or touch contexts during this run.

## UI assessment / limits

Grouped palette and native controls are clear and compact. Footer Required control and subdued collapsed badge are easier to scan than a title star. No additional blocking visual problem found. At 390px, the palette is below the full form and long form name is horizontally clipped inside its editable input; neither caused overflow or an unusable control. Moving the palette into another mobile interaction would be a separate UX decision, not a bug fix.

This pass did not send a real response, upload a real file, send a calendar invitation, or select a relation row from a nonempty source. It did not test conversion of existing incompatible answers or live collaboration under distinct account permissions. Those are outside this builder-only pass and should be combined with the response/security reviewer results. Touch was Chromium emulation, not physical iOS Safari.

## Screenshots

- desktop-final.png, medium-final.png, mobile-top-final.png, mobile-palette-final.png: final bundle.
- type-menu-final.png: native grouped menu, final bundle.
- all-types-builder-final.png: all-types fixture, final bundle.
- all-types-preview.png: all question controls in respondent Preview, initial bundle.
- palette-drag.png: pointer insertion line.
- touch-palette-drag.png: touch insertion line on final bundle.
- screener-blocked-drag.png: refused move below dependent screener.

All screenshots are in /home/wolf/tmp/forms-qa-oct6/builder/.

# Final regression verification — live bundle app-Brj45bfH.js

Reverified after root's final fixes on code a253893b51 rebased onto latest main. New own fixture: QA builder Oct6 final race audit, /app/form/01a10fd4-dbfb-730a-a8d0-8b72da518322. Linked database 01a10fd4-dbc4-7d69-af81-38a964e476e2. Tests used Playwright request interception only to pause and release real backend requests; no mocked successful responses.

## Passed final checks

1. Delayed Share metadata before Preview. Paused PATCH /dss/forms/01a10fd4-dbfb-730a-a8d0-8b72da518322 whose body was {confirmationMessage: 'QA final metadata ordering verified'}. Clicked Preview while request was paused. Reserved popup stayed at about:blank and displayed 'Saving your changes for the preview…'. After releasing the real PATCH, popup navigated to respond?preview=true. Completing the preview displayed the exact new confirmation 'QA final metadata ordering verified' and 'No response was saved.'
2. Pending question survives Build -> Share. Paused POST /dss/databases/01a10fd4-dbc4-7d69-af81-38a964e476e2/ops creating a Number column. The optimistic question was visible; entered title 'Delayed question survives Share', switched to Share and clicked Preview. Popup stayed about:blank while column-create was paused. Released request; real create and queued rename completed (two ops requests). Preview contained the Number question with the new title. Back in Build, hard reload retained both initial and delayed questions.
3. Recreate question section before Booking. Selected the existing local fixture booking link 'Forms screening conversation' without booking or modifying that link. Deleted the only question section through its confirmation dialog, leaving one Booking section. Clicked Add Short answer: new Questions section appeared before Booking. Deleted that Questions section again, leaving Booking alone; From database -> Initial persisted question recreated Questions before Booking. Reload preserved Questions then Booking. No backend gate/layout validation error or stuck save.
4. First outline click after rename remains fixed. Reused a Number column, immediately edited its title, clicked Initial persisted question once in the outline; selected editor correctly switched on first click.
5. Form -> database -> Forms menu -> form navigation passed two further loops on final bundle. No captured builder pageerrors. Final fixture has a Questions section with two questions followed by Booking.

Screenshots added:
- final-delayed-metadata-wait.png and final-delayed-metadata-complete.png
- final-delayed-column-wait.png and final-delayed-column-preview.png
- final-add-before-booking.png and final-reuse-before-booking.png
- final-race-fixture.png

Limitations unchanged: no real calendar invitation or real submission was created. The booking link already existed; this tested builder ordering, not external calendar delivery. One first-run harness response-inspection callback assumed the wrong JSON layout nesting and crashed its own Node driver after releasing metadata; the entire metadata pause/release scenario was repeated successfully in a fresh driver. This was a test-harness error, not an application error.

# Latest-main integration smoke — ef3332618e

Verified live bundle /app/app-CsdFMqdk.js after final clean rebase onto main f381296fde. Root confirmed Forms sources are byte-identical to the previously audited a253893b51, so this was an integration smoke rather than another exhaustive pass.

- Loaded existing own fixture 01a10fd4-dbfb-730a-a8d0-8b72da518322.
- Preview opened successfully at respond?preview=true with preview banner.
- Pointer-dragged Paragraph from palette into first Questions section: count 2 -> 3, renamed to 'Final integration drag question'.
- Started another palette drag (Link), saw allowed insertion indicator, pressed Escape and released pointer: count stayed 3.
- Opened linked database, used Forms button and returned to the form; new question persisted.
- Captured no builder or preview pageerrors.
- Screenshot: final-integration-ef3332618e.png.

No source edits, backend changes, real submissions or invitations. Own browser/helper cleaned up afterward.
