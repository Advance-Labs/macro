# Macro Forms follow-up — review round 1

Reviewer: Opus, read-only. Scope: `git diff HEAD` (HEAD 4c13b5f73c) plus untracked files, focused on
forms / collab_surface / sync_service_client / DSS wiring and `apps/web/src/features/block-form` +
scheduling booking. Source-only; nothing browser-verified. I did not flag the in-flight sync
initializer 409/retry work, SQLx cache or formatting.

**Verdict: FAIL.** There are required fixes (R1–R5). Everything else is a recommended fix or a question.

---

## Required fixes (verified by reading)

### R1. Any channel member can permanently squat a form's surface id, which locks the builder and SDK layout writes
- `collab_surface/src/domain/service.rs` `ensure_surface` lets callers create surfaces under any
  caller-owned parent (channel, chat, …) with any random UUID. `is_random_id` accepts v7, so form ids
  pass. `refuse_taken_id` only checks *documents* and *existing sessions*.
- Forms never create their surface at creation (`service/create.rs` has no `ensure_draft`). They create
  it lazily in `ensure_draft` (first builder open or first SDK `put_layout`).
- Scenario: a respondent sees form id `F` in the URL. They call the public `ensure_surface` with
  `parent = their channel` and `id = F`. Later, `ensure_owned_surface_from_snapshot(Form F, F, …)` hits
  the fast path. `verify_receipt_matches_parent` returns `AccessDenied` → `FormError::Collaboration` → 500.
  - `POST /forms/F/collaboration` and `PUT /forms/F/layout` then fail forever.
  - The builder never opens.
  - Every existing (legacy) form and every new form is exposed until its first builder open.
- RFC 04 says: "Public surface creation and deletion cannot take ownership of a form's document".
  The test `the_public_api_never_ensures_or_deletes_a_form_surface` only covers a *form* parent, not a
  form *id* under another parent.
- Fix:
  - Make ids of domain-owned parents unavailable to the public API. For example, generalize the
    `DocumentIds` port into a reserved-id port that also answers "is a form id", implemented by an
    outbound adapter over the forms domain (not cross-domain SQL).
  - And/or seed the form surface at creation, with a backfill for existing forms.
  - Add a red test: channel-parent `ensure_surface(id = form id)` is refused.

### R2. An undecodable draft returns 500 to respondents and cannot be repaired
- `service/drafts.rs::refresh_layout` maps `collaboration::read_layout` failures (`InvalidSection`,
  `Malformed`, `InvalidJson`, `UnsupportedFormat`) to `FormError::Collaboration` with `?`.
- `read_detail`, `respondent_context` (submit/edit), `my_response`, `summary` and tallies all call it.
  So one malformed value fails every read and every submission with 500, for respondents too.
  - Example malformed values: `required: "yes"`, a booking `target` with the wrong shape, an unknown
    `kind`, or a non-text `title`.
  - Sources: a client bug, an older/newer client, or any Edit holder who can push raw Loro ops with
    their surface token.
- This contradicts RFC 04: "Invalid drafts remain repairable in the builder; respondents use the last
  validated layout."
- No repair path exists:
  - The TS `decodeLayout` puts the builder into `error` ("edited by a newer version").
  - SDK `put_layout` → `collaboration::replace_layout` first decodes the *previous* layout, so it
    fails with the same codec error.
  - Only someone with a raw Loro client could fix it.
- Fix:
  - In `refresh_layout`, treat codec errors like validation errors: return the repository layout and
    set `publication_error`.
  - Give `replace_layout` a recovery path when the current document is undecodable, e.g. diff against
    the raw records it *can* read, or delete unknown/malformed keys and rewrite. Then the owner can
    repair via SDK or "reset layout".
  - Add tests for both.

### R3. SDK `put_layout` can persist a change into the draft and still return an error
- `replace_draft` runs: `validate_layout` → `drafts.update(...)` (the Loro change is committed to
  sync-service) → `refresh_layout`. Only then does `project_layout` check:
  - cross-form id collisions (`id_of_another_form` → `IdTaken`),
  - the locked audience (`AudienceChanged`).
- `validate_layout` only checks ids are unique *within* the layout.
- Scenario: an SDK caller reuses a section/question UUID that another form already uses.
  - The Loro update applies; the projection returns `IdTaken`; the SDK gets `RepeatedId`.
  - The caller reasonably assumes nothing changed. But every editor's builder now shows the colliding
    section plus a "Respondents still see the last valid version…" banner, and respondents stay frozen
    until someone deletes it.
  - The same happens on the audience race.
  - Previously the relational write was atomic.
- Fix: check cross-form id collisions (a read port over the repo) before writing Loro. Return
  `Conflict`/`AudienceChanged` *before* mutating where possible. Otherwise document that the draft
  write is not rolled back and return success with a `publication_error` instead of an error.

### R4. "Booking is always last" is stored order, so concurrent inserts break it and then most builder edits are refused
- Order is a Loro movable list. If editor A adds the booking step while editor B concurrently adds a
  section at the end, the two concurrent end-inserts merge in peer-id order. About half the time the
  booking step lands before B's section. Two concurrent "add booking" actions give two booking sections.
- The server then reports `BookingMustBeLast` (respondents frozen, which is acceptable). The builder is
  where it hurts:
  - `core/form-layout.ts` `checkedLayout` validates the *whole* layout on `updateSection`,
    `moveSection`, `moveQuestion` and `addSection`.
  - So typing in any section title or description, or moving any question, is refused on every
    keystroke with a toast.
  - The controlled `<input value={props.title}>` keeps the typed characters even though nothing was
    written, so the UI shows unsaved text.
  - The refusal text says "Move 'Book a time' to the end", but `BookingCard` has no drag handle. The
    only fix is to move *other* sections above it, or delete one.
- Fix (preferred): make the position structural. Both codecs (`read_records` / `readLayout`) order
  booking sections last, so position can't be invalid. Separately reject a second booking step.
- Alternative: `checkedLayout` refuses only problems the edit *introduces* (compare with the problems
  before the edit).
- Add a two-peer merge test like the existing concurrency tests.

### R5. Surface retirement misses cascade purges, and its ordering can leave a restorable form unreadable
- `forms.database_id` / `table_id` / `owner_id` are `ON DELETE CASCADE`. Purging a database, its
  table, or the owning user removes the form without `retire`.
  - The `collab_surfaces` row and the sync-service session survive. That session holds layout and
    booking-target content.
  - RFC 04: "Purging a form retires its owned surface." CS-05 asks for a deliberate cleanup story.
- `lifecycle.rs` permanent delete calls `drafts.retire` *before* `repository.delete_form`. Scenario:
  1. The retire succeeds, then `delete_form` fails.
  2. The form is still trashed and restorable.
  3. After restore, `draft_state.enabled = true`.
  4. `refresh_layout` → `owned_surface_snapshot` → the surface is retired (`get_live` fails).
  5. Every read and submit for that form returns 500, permanently, because a retired id "never comes back".
- Fix: delete the row first, then retire (idempotent, retryable). Cover the cascade paths too, e.g.
  the databases purge flow notifies forms, or a cleanup job retires surfaces whose form row is gone.

---

## Recommended fixes

### M1. Publication errors reach the UI as raw domain strings with UUIDs and "gate" wording
- `FormCollaboration.publication_error = error.to_string()`. The builder renders it verbatim:
  `Respondents still see the last valid version of this form. the table has no column 0199…`.
- Other examples: "a gate can only test questions of earlier sections; column <uuid> is not one" and
  "the id <uuid> is used twice". The UI calls gates Screeners, and RFC 04 removes implementation copy.
- A common trigger: deleting a column in the grid that a question asks.
- Fix: return a typed problem (`LayoutProblem` / `FormErrorCode` plus ids) and render it through the
  existing `layoutRefusalMessage` with section and column names.

### M2. Linked names go stale in previews and in other users' open pages
- `renameForm` updates the *form's* preview name but not the linked database's preview item.
- `renameDatabase` invalidates form detail/list queries but never touches `previewKeys` for linked
  forms. Tabs, mentions and channel cards that read previews keep the old name until a hard refetch.
- Server side, a database rename (grid or GraphQL) emits no `FormTopicEvent::Renamed` and no
  `announce(form)`. Another user with the form open keeps the old title, and the activity feed never
  records the form rename.
- RFC 04's "no asynchronous name-copy process" makes reads correct, but not live.
- Fix:
  - Update or invalidate the preview item of the *other* entity on both rename paths.
  - Decide whether linked forms should be announced on a database rename (e.g. forms subscribes to the
    database rename event).

### M3. Respondent reads are expensive, and a size limit can make them fail for good (question/perf)
- Every `read_detail`, submit, `my_response`, summary and tally now does a collab_surfaces lookup, a
  `documents` id lookup (`refuse_document_id`), a full Durable Object snapshot fetch over HTTP, a Loro
  import, and a version-vector compare. This happens even when nothing changed.
- Public forms and channel polls (many cards × many viewers) multiply this. CS-47 asks to keep hot
  paths thin.
- The snapshot is a *full* history export capped at 4 MiB (`MAXIMUM_DOCUMENT_BYTES` / sync-client
  bound). Once a long-edited form's history exceeds it, `TooLarge` turns every respondent read into 500.
- Suggestions:
  - A cheap revision probe (version only) before fetching the snapshot, or a shallow snapshot.
  - Refresh only when the stored revision is behind.
  - At minimum, fall back to the stored projection on `Collaboration` errors for respondent paths (see R2).
- RFC 04 does require that reads refresh from durable state, so please confirm the intended cost.

### M4. Booking targets are not validated on the server
- `validate_layout`'s `Booking` arm checks only the id, title/description length, and that it is last.
- Any Edit holder or SDK caller can set an arbitrary `{profileId, eventTypeId}`: someone else's event,
  a disabled one, or a nonexistent one. Respondents then reach "This booking link is no longer
  available" only *after* their response is saved.
- RFC 04: "chooses an existing personal or team Macro booking link".
- Suggest a scheduling read port in the forms domain: the event exists and is enabled, and the editor
  (or form owner) owns it or is on its team.

### M5. Generic `ShareButton` now hard-codes the forms feature
- `lib/core/component/TopBar/ShareButton.tsx` imports `@app/features/block-form/core/respond-link` and
  lazily imports `block-form/form-link-sharing`. It also branches on `itemType === 'form'` in two
  places (`shareUrl`, `customLinkSharing`).
- The previous injected `audience` prop was the right shape. FE-16 and FE-33 say per-host behavior
  goes through props.
- Suggest passing `linkSharing` and the copy URL from the form host, and keeping the dialog generic.
- Question: `shareUrl('form')` now copies the *responder* link, and the standard `LinkSharingControls`
  are replaced for forms. Where does an editor copy or share the *editing* URL, which RFC 04 says is
  separate?

### M6. Drag feedback for a new section dropped after the booking step
- `create-builder-drag.ts` computes no refusal for `new-section` targets. The indicator and the
  announcement say "Added … at position N+1".
- `form-layout.ts::addSection` silently clamps the index to just before the booking step.
- So the line shows below the booking card while the section lands above it, and screen readers hear
  the wrong position.
- Either clamp the drop index while locating, or announce the actual index that `addSection` returns.

---

## Questions and low-priority items

- **Q1 (UX, needs browser check).** Title, description, help and message inputs write on each
  `onInput` and are controlled by `value={props.…}`. When a peer edits the *same* field, Solid resets
  `input.value`, which moves the local caret to the end mid-typing. The CRDT merge is fine; the caret
  is the problem. Consider preserving the selection when the value updates during focus.
- **Q2.** `names.rs::database_edit_receipt` builds a database `EntityAccessReceipt<Edit>` with
  `try_new` inside the domain. This matches the entity_access rule (form edit/owner ⇒ database edit),
  but it duplicates that policy. Should the database receipt come from entity access, or as an
  internal-receipt-style rename that records attribution?
- **Q3.** Pending question scenario: a column is created, then a peer deletes the target section before
  the `landed` callback writes the layout. `withPendingQuestions` presumably drops the question, which
  leaves a column with no question and no notice. Confirm the intended behavior.
- **L1. Dead code.** `put_layout` now always goes through `replace_draft`. The non-projection branch of
  `pg_forms_repo/drafts.rs::write_layout` (`projection: None` → `DraftRequired`) and the
  `LayoutReplacement::DraftRequired` handling in `refresh_layout` look unreachable from production.
  Remove them, or document who still calls `replace_layout`.
- **L2. CS-49.** The new `existing_snapshot_is_a_typed_conflict` test extends the inline `mod tests` in
  `sync_service_client/src/initialize.rs`. Move it to a sibling `test.rs` (`document_state` already
  does this).
- **L3. CS-24.** `forms/src/domain/collaboration/test.rs` is 1,779 lines and collab_surface
  `service/test.rs` grew by about 720 lines to roughly 1,690. Split them by concern.
- **L4.** `docs/design/forms/README.md` still says "Read the three RFCs in order"; there are now four.
- **L5.** `EntityType::Form.with_entity_string(id.to_string())` appears four times in
  `outbound/collaborative_layout.rs`. Use a helper.

---

## Checked and correct

- **Field-level Loro edits.** Both codecs diff previous → next per field. Texts use `LoroText.update`.
  Order changes are minimal moves, and concurrent inserts are kept. The builder's `write()` diffs
  against the *live* document layout (`collaboration.layout()`), not a stale copy. Rust and TS agree on
  the schema, including JS doubles for the format key.
- **Projection.** It is compare-and-set on `layout_revision` under the form row lock, with bounded
  retries. A stale snapshot can't overwrite a newer revision. An invalid draft keeps respondents on the
  last valid projection; the codec-error case is the exception (R2).
- **WAL and sync session.**
  - Refused appends are held in memory and re-logged on flush.
  - The base snapshot is persisted before edits are accepted.
  - A disposed session can't save a snapshot, so it can't prune another session's WAL.
  - Cross-tab gossip comes from the default `BroadcastChannelChatter`.
  - Flush errors are typed (offline, unsaved, storage, publication, publish-failed) and shown via
    `saveNotice` and `flushFailureOf`.
- **Preview.** It flushes column and metadata writes, then the layout, and publishes before showing the
  reserved tab. It reads the published projection, never submits, uses `blob:` files in-tab only, and
  the picker refuses to book in preview.
- **Booking privacy.**
  - `form_detail` strips the target below Edit.
  - Only `Submitted` outcomes carry `booking`; stopped, refused and failed-ledger outcomes reveal nothing.
  - `my_response` re-evaluates gates and required answers against the current layout, and requires the
    row and table to exist.
  - Surface tokens need Edit on forms (`surface_access_level`).
  - Server-side state access uses 60 s service tokens without a user id.
- **Names.** The forms domain reads names through the `DatabaseMetadataReads` port; there is no
  cross-domain SQL for names. The HTTP adapter (`collaborate_form_handler`) is thin, and authorization
  stays in the receipt plus the domain.
- **UI copy and conventions.** User-facing copy says "Screener". The persistent saved badge and the
  violet stripe are gone. Semantic color tokens are used. Booking and receipt flows reuse the native
  `BookingPicker` and receipt route.

## Convention verdict

Mostly compliant: ts-pattern `match` throughout, no `any`, semantic tokens, ports and newtypes, typed
errors, rootcause, Debug impls without content. Deviations:
- FE-16 / FE-33: ShareButton is coupled to the forms feature (M5).
- CS-49: an inline test was added (L2).
- CS-24: oversized test files (L3).
- Dead branches (L1).

## Architecture verdict

The layering is sound:
- `forms` depends on collab_surface's `OwnedSurfaceService` port through a forms-owned
  `FormDraftStore` adapter.
- Names go through the databases domain's read port.
- The surface token policy lives in the collab_surface domain.
- The sync client stays in outbound.

The real architecture defects are invariant ownership:
- Surface id reservation does not cover domain-owned parents (R1).
- "Booking is last" is a stored order rather than a structural invariant, so the CRDT can violate it (R4).
- Surface lifecycle is not tied to every way a form row is deleted (R5).

Respondent availability should not depend on draft decodability (R2).
