# Macro Forms browser proof

Real local-stack recordings for the Forms PR stack #7471–#7478. All people and responses shown are synthetic test data.

- `forms-builder-respond-proof.mp4`: mouse and keyboard question reordering, a gate, required-field validation, submitting, editing the same response, and ledger counts above the live response grid. Recorded against the assembled stack at `aa40170ba1`; the builder/response implementation is unchanged in the final head `764e8eba01`.
- `forms-channel-poll-proof.mp4`: `/poll`, posting to a DM, a second user voting, keyboard vote changes, respondent results, and the author's live tally. Recorded against final head `764e8eba01` and its rebuilt local backend.

GIFs are compressed previews; MP4s retain full quality. Contact sheets and stills were inspected. Editing removes only idle automation waits; the app actions and results are real.

Local demo: `http://localhost:35110/app/form/01a10a44-23d0-76a2-89d1-17493417797e`. Use local email login as `forms-owner@local.macro.test`. These links work on the development machine running the local stack.

Additional final-build checks: sent-card Collapse/Expand, and mobile touch dragging with edge scrolling across sections and into an empty section. Exactly one successful layout write per touch drop; the original demo layout was restored.


## Collaborative builder and screened booking followup

`forms-screened-booking.mp4` and its GIF show sidebar section dragging, the screener and linked booking step, native public-link sharing, an anonymous visitor stopped by the screener, a passing submission unlocking the calendar, and the accepted response appearing live in the grid. Recorded from the real local HTTPS stack after the final browser fixes.

The local test host has no connected calendar: the recording shows the native availability error explicitly. No external calendar invitation is claimed. Desktop/mobile screenshots and the two review rounds plus a final browser-fix review are included.

Source head: b2ab4bb6df01b1981e7d4095ea60bed3dd8ca807.
