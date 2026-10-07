# block-canvas

Saved documents can opt into [Canvas Next](canvas-next/README.md) with the
`enable-canvas-next` flag. The frontend migrates legacy JSON on load and writes
version 2 through the existing storage endpoint after editing. Unsupported legacy
content stays available in the legacy editor; newer versions are never handed to
it. SyncService integration is deferred.

`CanvasBlock.tsx` is the app-facing host. It loads the document and owns access,
history tracking, entity commands, split chrome, and block-handle registration
without creating a legacy block instance. `component/CanvasDocument.tsx` remains
the editor boundary shared by the direct host and Drive's entity-detail view.
Canvas cards in Markdown mount the direct host as read-only nested documents;
Canvas Next embeds mount an unmanaged full editor inside their isolated panel.

This block is a canvas where you can create, export, and import diagrams.

The rewrite lives in [canvas-next](canvas-next/README.md) and uses the pure TS
graphics package. It is exercised through saved Canvas documents behind the rollout
flag; there is no separate local demo route.

The editor has drawing, rich text/mentions, connectors, media and document
cards/embeds. Embedded documents keep their existing persistence. See the
[parity plan](../../../../../docs/GRAPHICS_PARITY.md) for remaining collaboration,
legacy-content support and editing work.
