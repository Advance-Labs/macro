# PDF anchor schema-drop prerequisite

Contract PR #6732 (`f7b3753db2`) shipped in `v2026.10.1.0` but still reads
`"PdfHighlightAnchor"."threadId"` when attaching a discussion to a highlight.
Migrations run before services: dropping that column while this code remains
deployed would fail PDF discussion creation with an undefined-column error.

This service-only change removes that predicate. The document, unclaimed
`root_id`, and non-deleted-highlight predicates remain. Tests cover cross-document
rejection, preventing a second discussion from taking an occupied highlight,
and reattaching after discussion deletion. No database migration is included.

The full schema-drop PR on `gab/messages/8-schema-drop` depends on this change.
**Deploy this prerequisite to every production consumer and verify the release
and rollout before deploying the drop. Merge alone does not close the gate.**
The schema drop separately requires a verified production snapshot and external
SQL-reader check. Neither PR authorizes merge or deployment, and neither changes
hosted data/config or publishes the SDK.

Deploy this attachment change only after legacy imports have completed and
legacy writes are frozen. An importer running afterward could overwrite a new
highlight root with the old thread mapping. The document cutover completed in
production September 25; CRM import completed September 29 (77 messages, 76
threads, zero invariant failures, second run a no-op). The approved dev PDF
exception was resolved September 30, leaving zero unmapped live comments and
rootless placeables; production had neither exception. #6732 is deployed in
`v2026.10.1.0` (`4dcc0d2529c66c3f707f7e30bf8ab804d15566c1`), with all Cloud
Storage migrations/deployments, web, sync, and AI editing successful in
[run 36934197899](https://github.com/macro-inc/macro/actions/runs/36934197899).
Before deploying this prerequisite, recheck import completeness and confirm
that no importer remains scheduled or running. Keep the schema drop last.
