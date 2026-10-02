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
