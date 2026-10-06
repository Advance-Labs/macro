# Phone calls

Macro places and receives ordinary phone calls through
[LiveKit SIP](https://docs.livekit.io/sip/). A phone call is a standalone Macro
call whose LiveKit room also holds one SIP participant: the person on the phone
network. Everything else is the existing call pipeline — the room is recorded,
the transcription agent transcribes it, the archived call record is summarized
and named, it appears under **Calls**, and it is linked to the CRM.

## How it fits together

| Piece | Where |
| --- | --- |
| E.164 parsing of typed numbers (extensions, `tel:` URIs, NANP rules) | `crates/phone_number` |
| Phone vocabulary: direction, status/outcomes, SIP failure mapping, dialing policy | `crates/call/src/domain/phone.rs` |
| Use cases: dial, ring, answer, hang up, webhook handling | `crates/call/src/domain/service/phone.rs` |
| LiveKit SIP adapter (`CreateSIPParticipant`, SIP participant attributes) | `crates/call/src/outbound/livekit_rtc_client.rs` |
| Numbers, live and archived phone legs | `crates/call/src/outbound/pg_call_repo/phone.rs` |
| HTTP API (`/call/phone/...`) | `crates/call/src/inbound/axum_router/phone.rs` |
| CRM contact numbers, caller lookup, call record linking | `crates/crm` (`contact_phones`, `inbound/phone_contacts.rs`, `outbound/call_link.rs`) |
| Composition and configuration | `services/document_storage_service` |
| Web: dialer, in-call card, ringing, Phone settings | `apps/web/src/features/phone` (`phone.tsx` mounts it app-wide) |
| Web: phone calls in Calls and call details | `features/entity` (call rows), `features/block-call` (detail, transcript, Call Again) |
| Web: contact numbers and click-to-call | `features/crm` (`views/contact-phone-numbers.tsx`) |
| SDK: `Contact.phoneNumbers()`, `crm.contactByPhone()` | `packages/sdk/src/entities/crm` |

Tables (`crates/macro_db_client/migrations`):

- `phone_numbers` — a number (E.164) and the Macro user it rings.
- `call_phone_legs` — the phone leg of a live call; archived with the call into
  `call_record_phone_legs`, which keeps the outcome.
- `crm_contact_phone_numbers` — numbers of CRM contacts, used to name callers
  and to link call records to contacts.

### Outbound

1. `POST /call/phone/dial { "to": "(555) 234-5678 ext. 89" }` parses the number,
   checks the dialing policy (allowed country codes; premium-rate NANP `900`/`976`
   are always refused), looks the number up in the caller's team CRM, creates the
   room and the call, and returns the caller's RTC token plus the phone leg.
2. The client joins the room with its microphone on, so it hears the call ring.
3. In the background the service starts recording and transcription, then asks
   LiveKit to dial (`wait_until_answered`, 45 s ringing timeout, 4 h maximum
   duration). An extension is keyed as DTMF two seconds after the callee answers.
4. The outcome is stored on the leg (`active`, or `busy` / `no_answer` /
   `declined` / `failed`) and sent to the caller's devices as
   `phone_call_updated`. A call that did not connect is ended.

The caller id is the user's own number when they have one, otherwise
`PHONE_DEFAULT_CALLER_ID`, otherwise whatever the trunk chooses.

### Inbound

1. A caller dials a number whose inbound trunk and dispatch rule put them in a
   new LiveKit room.
2. LiveKit's `participant_joined` webhook reports a SIP participant with
   `sip.phoneNumber` (caller) and `sip.trunkPhoneNumber` (dialed). Calls without
   either, or to a number not assigned to anyone, are rejected by deleting the room.
3. Otherwise the service creates a call owned by the number's user (idempotent per
   room), starts recording and transcription, and sends the owner
   `phone_call_incoming` with the caller and their CRM contact. The caller keeps
   hearing it ring: LiveKit only answers the SIP call once someone in the room
   publishes audio, and neither the recorder nor the transcriber does.
4. `POST /call/phone/{call_id}/answer` returns a token; the leg becomes `active`
   and every device of the owner gets `phone_call_updated`, so other devices stop
   ringing. Joining with the microphone on connects the caller.
5. If the caller hangs up first, the leg ends as `missed`; if the owner declines
   (`POST /call/phone/{call_id}/hang-up` while ringing), it ends as `declined`.

### Ending and archiving

A phone call lives and dies with its phone party. Hanging up from Macro deletes
the room, which disconnects the person on the phone; the person on the phone
hanging up ends the call for everyone. The call is then archived like any other.
The archive keeps the phone leg; the Calls list shows unanswered inbound calls
as **Missed** for the number's owner, and the call's display name falls back to
the CRM contact's name or the formatted number until the summary names it.

Transcript segments spoken on the phone use the leg's `participantIdentity` as
their speaker id. Summaries call that speaker by the contact's name or number.

### CRM

- `GET/PUT /crm/contacts/{contact_id}/phone-numbers` lists or replaces a contact's
  numbers (as typed; stored as E.164, at most 10).
- `GET /crm/contacts/by-phone?phone=...` names a number in the caller's team CRM.
- When a call record is archived, the CRM linker matches the phone leg's number
  against contact numbers in the teams of the people on the call and sets the
  record's **Companies** and **Contacts** properties, so phone calls appear on
  company and contact pages.

### Billing

Phone calls are paid for through usage billing; see
[Phone minutes](AI_QUOTA_ENFORCEMENT.md#phone-minutes) for the ledger.

- **Who can call.** Max and enterprise seats include phone calling. Premium
  seats need the Phone add-on ($15 per seat per month), which the payer turns on
  per seat in Settings → Phone. Free seats cannot call.
- **Minutes.** Every phone seat includes `AI_USAGE_PHONE_INCLUDED_MINUTES` (1,000)
  minutes per billing period. Minutes past them spend credits or usage billing
  at the `pstn` rate plus the usage markup. Connected time is billed from answer
  to hang-up in whole minutes; unanswered calls are free.
- **Gate.** Dialing a call the plan does not cover answers `402` with a `code`
  (`phone_plan_required`, `phone_minutes_exhausted`, or a shared overage code)
  and the dialer links to Phone settings. Inbound calls to such a number are
  rejected before they ring.
- **Where.** The call domain's
  [`PhoneBilling`](../crates/call/src/domain/ports/phone.rs) port admits calls and
  records minutes when a call is archived; the storage service composes it from
  AI usage admission and recording
  ([`phone_billing.rs`](../services/document_storage_service/src/outbound/phone_billing.rs)).

## Setting up a deployment

Phone calls need a LiveKit Cloud project with SIP and a SIP provider (Twilio
Elastic SIP Trunking, Telnyx, …). LiveKit Phone Numbers can receive calls but
cannot place them, so outbound calling always needs a provider trunk.

1. **Outbound trunk.** Create one with the provider's termination URI, the
   numbers it may present, and credentials
   (`lk sip outbound create outbound-trunk.json`). Set its id (`ST_…`) as
   `LIVEKIT_SIP_OUTBOUND_TRUNK_ID`.
2. **Inbound.** Either buy LiveKit Phone Numbers or create an inbound trunk for the
   provider's numbers (`lk sip inbound create inbound-trunk.json`). Then create an
   *individual* dispatch rule so every caller gets a room of their own, for
   example `{"rule": {"dispatchRuleIndividual": {"roomPrefix": "phone-"}}}`. Do not
   add agent dispatch to the rule: Macro dispatches the transcription agent
   itself. Any room name works; the service tracks calls by room name.
3. **Webhooks.** The project's webhook must already point at the storage
   service's `/call/webhook` (it does for ordinary calls); SIP participants arrive
   through the same endpoint.
4. **Configuration** (Doppler, `cloud-storage-service`):

   | Variable | Meaning |
   | --- | --- |
   | `LIVEKIT_SIP_OUTBOUND_TRUNK_ID` | Outbound trunk id. Unset: dialing out is off; inbound still works. |
   | `PHONE_DEFAULT_CALLER_ID` | Optional E.164 caller id for users without a number. |
   | `PHONE_ALLOWED_COUNTRY_CODES` | Comma-separated calling codes users may dial. Defaults to `1`. |
   | `AUTHENTICATION_SERVICE_SECRET_KEY` | Optional. With `ENABLE_AI_USAGE_BILLING`, minutes past an allowance settle as soon as a call ends; otherwise on the payer's next billing read. |

   Startup fails if any of these is set but unreadable. Billing also needs
   `AI_USAGE_PHONE_INCLUDED_MINUTES` on every billing host (`shared_ai`, and the
   authentication service's own configs) and, to sell the add-on,
   `STRIPE_PHONE_ADDON_PRICE_ID` on the authentication service: a recurring
   monthly per-unit price of $15.
5. **Assign numbers.** Numbers ring the user they are assigned to, and a user's
   first number is their caller id:

   ```sh
   curl -X PUT "$DSS/call/phone/numbers/%2B15552345678" \
     -H "x-internal-auth-key: $INTERNAL_API_SECRET_KEY" \
     -H 'content-type: application/json' \
     -d '{"userId": "macro|someone@example.com"}'
   ```

   `DELETE` the same path to release a number; calls to it are then rejected.
6. **Web app.** The phone UI is behind the `enable-phone-calls` PostHog flag
   (on by default in development). The client listens for two websocket
   events: `phone_call_incoming` (an `IncomingPhoneCall`) and
   `phone_call_updated` (`{ callId, phone }`).

## Operating notes

- Recording consent: calls are recorded and transcribed from the moment they
  start ringing. Some jurisdictions require every party's consent; enable phone
  calls only where the workspace has a lawful basis, or add an announcement
  (for example a provider-side greeting on inbound numbers).
- Toll fraud: keep `PHONE_ALLOWED_COUNTRY_CODES` as narrow as the workspace
  needs, and use the provider's geo permissions and spend limits as a second
  line of defense.
- A withheld caller id cannot be matched or called back, so such calls are
  rejected.

## Not yet built

- Voicemail and call forwarding when nobody answers.
- Ringing on iOS through VoIP push (inbound phone calls ring web and desktop
  clients over the websocket today).
- Missed-call notifications in the inbox.
- A self-serve number purchase and assignment UI; numbers are assigned through
  the internal API.
- Per-destination rates: every minute is priced at the one `pstn` rate, so keep
  `PHONE_ALLOWED_COUNTRY_CODES` to destinations that rate covers.
- Transfers, holds, and adding a second phone party.
