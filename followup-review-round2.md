# Macro Forms follow-up — review round 2

Reviewer: Opus, read-only. HEAD 20896b33b9 (rebased on main 4e9c613b3e), plus `git diff HEAD` and untracked files.
I read the round-1 report, the four fix reports, the changed files in full where relevant, and `collaborative-builder.png`. Nothing was built, run or browser-verified. I left alone the in-progress items: per-card booking source, Rust repair split, the post-SDK-write metadata refresh, and test/cache/OpenAPI drift.

## Verdicts

| Area | Verdict |
| --- | --- |
| Convention | PASS (optional notes) |
| Code smells | PASS (optional notes) |
| Correctness / security | PASS |
| UI / accessibility | PASS (optional notes; browser checks are with the parent) |
| **Overall** | **PASS**: no required fixes |

## Round-1 items, checked against the source

| Item | Result | Evidence |
| --- | --- | --- |
| R1 surface squatting | Fixed | The public `ensure_surface` refuses with `IdReserved` when `FormIds::is_form_id` matches, and checks this before the existing-row fast path, so an earlier squat row is not handed back. The port is implemented by forms over `FormsRepo` (no cross-domain SQL). An owned-only service has no public impl (`OwnedSurfacesOnly`). DSS wires `with_form_ids`. Tests are in `test/form_ids.rs`. |
| R2 undecodable draft | Fixed for respondents and SDK | `refresh_layout` maps a codec failure to the stored projection plus `InvalidDraft`, so reads and submits no longer return 500. SDK replacement repairs the draft through `repair::clear_unreadable` and keeps its history. `TooLarge`, `Decode` and `IncompleteHistory` are still refused. |
| R3 write-then-error | Fixed | `conflicting_layout_id` runs before the Loro write. After a successful write, publication problems come back as a 200 `FormCollaboration{detail, publicationError}`. `Pending` is used when the refresh fails. RFC 04 documents this, and the SDK returns `{form, publicationError}`. |
| R4 booking order | Fixed | Both codecs order booking sections last (stable). `checkedEdit` refuses only problems an edit introduces. Two concurrent booking steps decode, and each can be edited or removed. |
| R5 retire order / cascades | Fixed | Purge deletes the form first, then retires (a retire failure is logged). Cascaded orphans are retired lazily in `get_parent` on first access. The remaining gap (sync-service session bytes are never reclaimed) is the same pre-existing gap documents have. |
| M1 typed publication | Fixed | `FormPublicationProblem` is a typed enum. `publicationMessage` uses an exhaustive `match`. No UUIDs or "gate" wording reach the UI. |
| M2 linked names live | Fixed for editors | `database_changed` is published after a rename. The form editor invalidates the database detail. `createFormDetailSource` refetches the form when the database name changes. Previews are invalidated in both directions. Non-editors refresh on their next read, which is acceptable. |
| M3 compact snapshots | Fixed (size); cost accepted | `?shallow=true` returns a shallow snapshot at `state_frontiers` with the full `oplog_vv` revision, so updates and compare-and-set still work. Each respondent read still makes one Durable Object round trip, as RFC 04 states. |
| M5 native sharing slots | Fixed | `ShareButton` no longer imports any forms code. The host passes `linkSharing`/`copyLink`. Global sharing goes through `openFormShareModal`. "Copy editor link" answers the editing-URL question. |
| M6 drop position | Fixed | `sectionInsertIndex` clamps the index. The drag line is redrawn at the clamped index, and the announcement and creation use it too. |
| Caret / IME | Fixed (logic) | `LiveTextInput`/`LiveTextarea` are uncontrolled native controls. They remap the selection, preserve its direction, and defer remote updates during composition, then rebase on compositionend. A refused edit reverts the text and keeps the caret. Section, screener, booking and help-text fields use them. The question title stays as a blur-commit `DraftInput`, which is correct. |
| L1–L5 | Mostly done | Initialize tests moved to `initialize/`. Test files are split. `collaboration.rs` is 955 lines and `repair.rs` 110. The README wording is fixed. |

## Booking-target permission: not a material gap

- The target is `{profileId, eventTypeId}` of a native public booking link. Anyone can already list and book that event type through the public profile. The form does not grant any new capability.
- The server hides the target below Edit and until a submission is accepted.
- The scheduling service independently enforces availability and booking eligibility when a respondent books.
- If an editor attaches someone else's public link, or a deleted or disabled one, the result is the same as sharing that public URL: respondents get "no longer available" (handled gracefully). No data or calendar access leaks.
- The builder picker lists only the viewer's own and their team's enabled links.
- A save-time existence check would only improve editor feedback; it is not a permission requirement.

## Optional notes (not blocking)

1. **RFC 04 wording vs. behavior.** RFC 04 still says "Invalid drafts remain repairable in the builder". That holds for validation problems. For `InvalidDraft` (malformed records), the TS `readLayout` throws, the builder shows "This form could not be opened", and only an SDK replacement repairs it. Either reword the RFC ("malformed drafts are repaired by a layout replacement") or add a builder "Restore published layout" action that PUTs the last projection. The codec agent asked for the same.
2. **`publicationMessage` names questions from `result.detail`**, which is the published projection (`form-publication.ts`). A problem in a question added only to the draft (`repeatedColumn`, `gateNamesLaterColumn`) reads "That question". Resolving against the builder's draft layout and column names would name it.
3. **Redundant banner copy.** In `builder-view.tsx:513`, the banner prefix "Respondents still see the last valid version…" is followed by `invalidDraft` "…Your published form is still available.". `pending` says "try again" but offers no retry control (reload or the next publish retries it).
4. **Silent catch-all.** `publication_problem(_ => Pending)` in `service/drafts.rs` collapses any unexpected `validate_layout` error into "pending". Prefer listing the variants or logging the unexpected one, in line with the no-silent-fallbacks rule.
5. **Floating promise (FE-13).** `make-share-action.ts:23` now calls the async `openGlobalShareModal` without `void`/`await`. A failed chunk import of `form-global-sharing` would be an unhandled rejection with no toast.
6. **Hand-rolled clipboard (FE-17).** `form-link-sharing.tsx` writes `navigator.clipboard` directly, while its sibling host uses `useCopyLink`. Use the one helper.
7. **Port naming.** The `FormIds` port puts a specific domain's name into the generic collab_surface crate (CS-28). A neutral "reserved parent ids" port would also cover the same lazy-surface squat class for initiatives, which the surfaces report already noted.
8. **Screenshot.** `collaborative-builder.png` shows the form "UI feedback verification" stored in database "Untitled form". If this is a standalone form created after the linked-names migration, the names should match. It may predate that migration; worth confirming in the parent's browser pass.
9. **Pending formatting.** `forms-sync.test.ts` and parts of `layout.rs`/`drafts.rs`/`form-publication.ts` aren't biome/rustfmt formatted yet. The parent's format pass covers this.

## Convention verdict
Compliant: ts-pattern exhaustive matches, typed wire errors, ports/adapters direction, tests split by concern, no `any`, semantic tokens, native controls. Items 4–6 are small, optional.

## Smells verdict
No blocking smells. The repair is isolated in `repair.rs`, and the per-millisecond presence send is documented with a root cause. Item 4 is the only fallback-style smell.

## UI verdict
Source review only: existing Kobalte/`@ui` components (Dropdown, CopyButton, ShareModal slots), labelled native inputs, `role=status`/`alert` used appropriately, the drag announcement is corrected, and no colored stripe or saved badge. Copy items 2–3 and 8 are optional. The parent's two-context, caret, IME, mobile, public-sharing and booking browser checks are still outstanding.
