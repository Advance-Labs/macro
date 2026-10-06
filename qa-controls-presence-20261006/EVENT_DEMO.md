# Event planning demo — ready for form testing

Owner: forms-final-qa@macro.com (local passwordless email login).

- Editor: https://wolf-macro-google:35709/app/form/01a11281-8947-7216-8e95-7640968a35fa
- Respondent: https://wolf-macro-google:35709/app/form/01a11281-8947-7216-8e95-7640968a35fa/respond
- Database: https://wolf-macro-google:35709/app/database/01a11281-8918-7ee8-90e1-2cd96a6a295a

Seven questions: name, work email, event type, expected attendees, USD budget, preferred date, notes. First five required. Screener requires corporate event OR team offsite, AND attendees >=12, AND budget >=10000. A helpful refusal message explains eligibility. Final booking step links an actual local scheduling event with weekday09:00–17:00UTC availability and30-minute consultations.

Four accepted responses populate the database through the real submission endpoint: Avery120/$85k, Jordan28/$18k, Sam250/$160k, Morgan32/$22k. One signed-in failed submission is recorded in the stopped ledger. Final server summary and Responses UI agree: submitted4, stopped1, rows4.

Actual anonymous browser checks: private celebration/8attendees/$5k showed refusal and no booking picker. Updating to team offsite/32/$22k saved the fourth response and revealed the booking calendar. Selecting October7 showed available time buttons. Editor and Responses table verified with no JavaScript page errors.

Calendar fixture is synthetic and local only. Availability works. A real booking attempt returned booking.status=failed because the fixture has no Google OAuth grant (server ReauthRequired). No calendar invitation was sent. Do not describe final booking as working end to end.

HTTP PUT/forms/{id}/layout returned500 because sync HTTPupdate returned503. Seeded the same layout using the app's actual Loro codec and SyncServiceSource websocket instead; durable acknowledgement succeeded, POST/forms/{id}/collaboration published successfully. No direct database layout writes or mocked responses. The HTTP update bug remains to diagnose separately.

Artifacts: event-demo-fixture.json, event-demo-results.json, event-demo-builder.png, event-demo-responses.png, event-screener-stopped.png, event-booking-slots.png. Calendar seed SQL and fixture IDs are also in this directory. Original Forms playground demo was untouched.
