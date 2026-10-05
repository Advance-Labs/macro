# Browser-fixes follow-up review (wolf/forms-channels, uncommitted tree)

**Verdict: PASS.** No blocking gap from the main rebase.

## Title (Drive "Unknown")
- `apps/web/src/features/entity/extractors/entity-title.tsx:51-55` adds the `{ type: 'form' }` arm (`name || blockNameToDefaultFile('form')`) and replaces `.otherwise(() => 'Unknown')` with `.exhaustive()`. A future `EntityData` variant now fails to compile instead of rendering "Unknown".
- `'Untitled form'` comes from `features/block-form/definition.ts:19`, which reaches `blockNameToDefaultFile` through the `allBlocks.ts` definition glob. The test mocks it as `` `Untitled ${name}` ``, which matches.
- `entity-title.test.tsx` covers name, the empty-name default, and a reactive rename through a signal. `drive-title-red.log` shows all 3 tests failing before the fix, and they pass now.

## Other form arms across Drive and Quick Access (branch vs. main)
- Icon: `lib/core/component/EntityIcon.tsx:685` has a `form` arm, and the match is `.exhaustive()`.
- Key properties, rename, and shared types: `entity-key-properties.tsx:73`, `entity/queries/rename.ts:111,233`, `entity/utils/shared.ts:12`, `buildEntityData.ts:192`.
- Drive: `drive-forms.ts` maps `ListedForm` → `FormEntity` with service `access`. `drive-list.tsx:68`, `drive-facets.ts:71`, and `drive-create-menu.tsx:50` all have form arms.
- Quick Access: the `form` bucket is in `types.ts` (Bucket, ALL_BUCKETS, combinations, BucketItemMap). `QuickAccessSource.tsx` adds `formEntries`, gated by `enableForms`, carrying `access`. Forms appear in the index and refetch.
- Actions: delete and rename are disabled for forms (`make-delete-action.ts:154`, `make-rename-action.ts:19`), the same as for databases.
- Sweep: every non-test `apps/web/src` file that matches on an entity and mentions `'database'` also handles `'form'`. No `.otherwise` fallback silently drops forms.

## form-global-sharing.tsx
- **Wire conversion:** `FormShareDialog` reads `useFormDetailQuery` and passes `toFormDetail(query.data)` (`queries/form-detail.ts:106`) only after `isSuccess`. `openFormShareModal` calls `fetchFormDetail`, which goes through `queryClient.fetchQuery` on the same key. That fills the cache, so the dialog opens with data and stays live as `updateForm` changes it.
- **Native modal:** it renders `ShareModal` from `TopBar/ShareButton` with `blockAlias/itemType: 'form'`, `userPermissions: getPermissions(detail.access)`, a `copyLink` that copies the respond link, and `linkSharing`.
  - `ShareButton.tsx` dropped the lazy form fallback, so `linkSharing` now comes only from the host. Both hosts supply it: `form-block.tsx:81` (`createFormShareInput`) and the global path.
  - No other `itemType: 'form'` caller exists, and `form-link-sharing` has a single importer.
  - Forms are still excluded from generic link sharing (`ShareButton.tsx:137`). Their permissions and channel updates go through the form endpoints (`:166`, `:940`, `:1056`).
- **Global entry:** `GlobalShareModal.tsx` sends `type === 'form'` to a dynamic import of `openFormShareModal`. If the import fails, it shows a toast. `isShareableEntityType` includes `form`, and `make-share-action` now awaits the call.
- **Test contract:** `openFormShareModal › uses the live wire detail…` gives the query mock a real wire `FormDetail` (`sections`, `createdAt/updatedAt`, no `layout/columns`). It then renders the actual dialog component passed to `openDialog` and clicks the audience radio, asserting `updateFormMetadata('form-1', { audience: 'public' })`. That covers the real data path end to end, with only `ShareModal` stubbed.

## Non-blocking nits
1. The wire test would still pass if the `toFormDetail` call were dropped. `FormLinkSharing` reads only `form.audience` and `access`, and both exist on the wire shape. `tsc` is what enforces the conversion (`final-web-types-with-title.log` is clean). If a runtime check is wanted, assert something only the domain shape has, such as `layout`.
2. `openFormShareModal › opens the share dialog…` mocks `fetchFormDetail` with the domain `rsvp(...)` instead of a wire detail. It's harmless because the test only checks call arguments, but the fixture type is inconsistent.

## Evidence run in this review
`bun run vitest run` on entity-title, form-global-sharing, the global-share-modal tests, QuickAccessSource, and ShareButton: **6 files, 111 tests passed.**

Not run here: browser (root-owned), full `tsc` (relied on root's log), `just check`.
