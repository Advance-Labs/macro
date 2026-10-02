# Databases performance — PR #7215

Measured on 2026-10-02 against the local stack. Sales CRM has 1,040 Deals, 36 Contacts, and 24 Companies. Both Soup's page limit and the engine's `PAGE_LIMIT` remain 500. Rows still use `content-visibility: auto`; there is no virtualization. Grid cells retain native title tooltips.


Final medians across three runs were **2.268 s cold**, **1.538 s cached return**, and **33 ms scroll-frame p95**. The full interaction run measured **57 ms** for a visible text edit, **83 ms** for a visible inserted row, and **151 ms** for the first select-picker open. Cold opening improved 21% against the initial measured baseline; cached return improved 29% against the matched cache-enabled baseline. The requested <1.5 s cold and <1 s warm targets were **not reached**.

## Method and evidence

The existing `measure2.mjs` harness ran in a private headless Chrome through Playwright, at 1600×1000, against this worktree's Vite on port 3007 and the shared local backend through port 32010. The shared Chrome on port 9222 was never used. CPU profiles, Chrome traces, request trace IDs, long tasks, and DOM mutation counts were recorded. These are instrumented development-build measurements, not production-build benchmarks.

A cold Deals open means a fresh browser context, with Companies already visible, followed by clicking Deals and waiting for every row. Warm opening means switching to Contacts and back to Deals in the same context. The harness's small `dealsAllRows` value after a full reload is **not** a warm-open measurement: the remembered Deals view is already loaded before that click. Full navigation/reload also includes Vite module loading and is reported separately in the raw artifacts.

The initial baseline had the GraphQL cache disabled. A separate matched before/after pair explicitly enabled `VITE_ENABLE_GRAPHQL_SOUP=true`, confirmed a live cache host, and isolated the cache-opening change. No hot reload or formatting ran during retained timing runs. Edit, insert, and select scenarios restored the original values and deleted only their own inserted rows. Final SQL counts confirmed the fixture's original row counts.

Artifacts and logs are local in `/home/wolf/tmp/database-speed-perf/`. Each measurement label has a `-summary.json`, per-phase `.cpuprofile` and `.trace.json`, and a `.log`. `cache-proof.png` shows the loaded grid while background row requests were held; `cache-proof.json` records the result. Its 1,041 DOM rows include the blank insertion row, alongside 1,040 stored Deals.

## Changes

| Commit | Change and reason |
| --- | --- |
| `e26a1f2c5c` | Added authorization/hydration and GraphQL execution spans; bounded independent property authorization at 16 concurrent checks. This exposed the remaining per-row SQL cost. |
| `ee012e346f` | Replaced the eager relation-chip Button with its native-button styling; deferred the Projects flag for non-project entities and bot profile queries for ordinary people. Preserved picker, navigation, disabled, and keyboard behavior. |
| `ee0e2a63a0` | Preserved the request span when DataLoader spawns its task, connecting property timings to the browser/DSS trace. |
| `5f4a6c9546` | Shows a complete cached statement result first, then refreshes from the network. A cache miss stays provisional, and stale generations cannot start a superseded network read. Updated the app agent guide. |
| `e8680b2a30` | Shares cell-kind and selected-option derivations with Solid memos; removes the 700 ms row/sticky-cell hover transitions. Sticky positioning and hover/highlight colors remain. |
| `3d27a8fbaf` | Adds bulk database-row view receipts through the owning `entity_access` service and repository. One parameterized grant query serves each loader batch; the original single-row authorization path remains intact. Includes generated SQLx metadata. |

## Measurements by stage

Times are milliseconds. Stage measurements are individual runs; they are useful comparisons, not statistical guarantees.

| Scenario | Initial baseline, cache off | Cell setup changes, cache off | All frontend changes, cache on; old backend |
| --- | ---: | ---: | ---: |
| Cold Deals open | 2,862 | 2,524 | 2,489 |
| Return to Deals | 2,919 | 2,584 | 1,447 |
| Switch to Contacts | 516 | 444 | 421 |
| Text edit visible | 70 | 92 | 61 |
| Added row visible | 91 | 113 | 77 |
| Next blank insertion row ready | 438 | 445 | 373 |
| Select picker first open | 266 | 255 | 159 |
| Select change visible | 120 | 108 | 95 |
| Scroll frame p95 | 50 | 50 | 33 |

Labels: `baseline-ready`, `cells`, `memo`. The separate cache-enabled pair (`cached-before-ready` → `cached-after`) measured **2,161 → 1,674 ms** for returning to Deals, with the same backend and cell implementation. Cold opening in that pair was 2,601 → 2,487 ms. A browser check subsequently held four background GraphQL requests and still displayed all cached Deals before releasing the network.

The `memo` scroll trace recorded 598 ms of Paint events versus 684 ms in the initial baseline. Its return-to-Deals longest main-thread task was still 1,184 ms, showing why cached data alone cannot make the whole grid mount instantly.

## Final deployed results

`db-e2e` deployed `3d27a8fbaf` at 07:03:25 UTC and confirmed a quiet stack. Measurements ran after its builds finished and its browser tabs were parked. `final-1` ran all scenarios; `final-2` and `final-3` repeated cold opening, scrolling, and table switching. The entries below use medians for repeated scenarios and the full run for editing/pickers. SQL counts after cleanup remained 1,040/36/24.

| Scenario | Initial baseline (ms) | Final (ms) | Before trace | After trace |
| --- | ---: | ---: | --- | --- |
| Cold Deals open | 2,862 | 2,268 | `aeb98a3a0eb4eed090ec89f9eb6c930f` | `257521846fb79df6d990cf8831fefa1e` |
| Return to Deals | 2,919 | 1,538 | `d5a8de20b7e029c774d4387a486fcdb7` | `6fd700570a0d641207b7d152db43009a` |
| Switch to Contacts | 516 | 478 | `e18fa79f5bf6b3bee59ecbbb68755d3a` | `58b5195acf2fc42906363afc096fa030` |
| Text edit visible | 70 | 57 | `bb7074038ae4ce83b4e3ee9e83ecf5c6` | `2cfc595dda61050a94399d4e31f139aa` |
| Added row visible | 91 | 83 | `9fe230aead91a45de9fd9c84a1076c75` | `c37954ab826484c2de310c0970e71ed5` |
| Next blank insertion row ready | 438 | 339 | Same insertion trace | Same insertion trace |
| Select picker first open | 266 | 151 | `4b009a2bf5cecdac82bff578832b57aa` | `cbd564d4d6773171c908e10c5aaedcad` |
| Select change visible | 120 | 87 | Same select trace | Same select trace |
| Scroll frame p95 | 50 | 33 | `baseline-ready-scroll.trace.json` | `final-1-scroll.trace.json` |

For cached returns, the after trace identifies the background refresh; displayed rows precede it. Picker opening and scrolling are browser work; their Chrome traces/CPU profiles are the timing evidence, while the select trace identifies the subsequent write/refetch. Trace JSON is available from `http://localhost:3200/api/traces/<id>` while the local Tempo data is retained.

| Repeat | Cold (ms) | Return to Deals (ms) | Contacts (ms) | Scroll p95 (ms) |
| --- | ---: | ---: | ---: | ---: |
| `final-1` | 2,157 | 1,600 | 478 | 33 |
| `final-2` | 2,295 | 1,527 | 443 | 33 |
| `final-3` | 2,268 | 1,538 | 519 | 33 |

The separate cache-enabled opening baseline was 2,601 ms cold / 2,161 ms warm, versus the final 2,268 / 1,538 ms medians. Thus the gain is not solely a comparison between cache-disabled and cache-enabled runs. Repeat cold traces are `162728fddad9122abf7f3d8354a850b4`, `c4b76cebac78678c6b5887efecfe6422`, and `257521846fb79df6d990cf8831fefa1e`.

## Server findings

The original trace `aeb98a3a0eb4eed090ec89f9eb6c930f` had full-page DSS HTTP spans of 284/279 ms while Soup's spans were only 59/57 ms. Connected property spans in `eac995408e9655a381584ef26b638306` then showed the bottleneck: many independent row authorizations, with loader authorization spans commonly 121–177 ms and hydration roughly 8–21 ms. Full-page GraphQL execution was about 286–290 ms; HTTP spans were about 372–376 ms. Bounded concurrency alone did not remove this database work.

The final implementation reads access levels for the requested row IDs in one SQLx macro query per batch and mints typed receipts in the domain service. It retains the database entity-type and caller source predicates, chooses the highest grant per row, and denies missing or malformed rows. Source lookup already has a short existing cache; this change does not add an authorization cache. A representative 100-row `EXPLAIN (ANALYZE, BUFFERS)` used indexes and executed in 0.694 ms (`batch-query-plan.txt`).


Live trace `162728fddad9122abf7f3d8354a850b4` confirmed the deployed result:

| Server measurement | Before bulk authorization | Final |
| --- | ---: | ---: |
| Loader authorization median | 145.3 ms | 3.3 ms |
| Loader authorization range | 13.8–177.0 ms | 1.5–4.7 ms |
| Property hydration median | 14.4 ms | 15.7 ms |
| Full-page GraphQL execution | 289.9 / 285.7 ms | 166.0 / 147.1 ms |
| Full-page DSS HTTP spans | 376.1 / 371.9 ms | 251.1 / 232.7 ms |
| Browser SQL run, all 1,040 rows | 1,008.9 ms | 619.5 ms |

These server comparisons use `deployed-tempo.json` (the instrumented, concurrent per-row implementation) and `final-tempo.json`. Against the **original** baseline, the full-page HTTP comparison is 284/279 → 251/233 ms; the larger intermediate comparison must not be mistaken for the original baseline. The remaining HTTP/GraphQL gap is about 85 ms per full page and needs more focused response/serialization profiling. The final trace also retains 2,080 per-row conversion spans; their payload formatting/export overhead is another follow-up worth profiling.

## Validation

- Red/green tests cover lazy person/project setup, cached answers before a delayed network response, cache misses, and batch receipt behavior. Authorization tests cover malformed IDs, denied rows, multiple databases, highest user/team grants, wrong entity types, and missing rows.
- `cargo test -p entity_access`: 515 passed; `cargo test -p graphql_properties`: 11 passed; `cargo test -p document_storage_service`: 14 passed. Tests used the live local database with `SQLX_OFFLINE` unset.
- The affected frontend suite passed 562 tests across 80 files; the post-rebase focused suite passed 67 tests.
- Web TypeScript checking passed with a 16 GiB Node heap. The initial default-heap run exhausted its heap.
- Workspace SQLx preparation, including tests and all features through the repository helper, passed. Only metadata for the changed query was retained in the commit; unrelated generated cache churn was excluded. The changed query uses compile-time checked, parameterized SQL, with bounded input IDs and unchanged access predicates.
- Browser database scenarios and cleanup succeeded. Cold-load logs still include existing 404 responses for `/dss/instructions` and `/auth/link/github/status`; these are outside the database scenarios.
- Full `just check` passed after rebasing the other agent's SDK formatting fix. Existing warn-only lint findings remain. A DSS compiler depth-limit regression from nesting the old single-row helper was caught and fixed before pushing; the final DSS build and tests passed.
- Hexagonal boundaries were checked: SQL stays in `entity_access`'s outbound repository; permission decisions and receipt minting stay in its domain service. GraphQL consumes receipts and calls the properties domain service.

## Remaining work and deliberate limits

The Rust engine emits one fetch step at a time. Parallel ID-first hydration would require changing the step protocol, Rust state machine, WASM bindings, and browser/native drivers, as well as handling cancellation and partial failures. It did not meet the goal's “only if it stays simple” condition and was deferred. The 500-row limits were not changed.

Rendering still mounts the complete grid. Solid computations, DOM creation, user/relation display components, and garbage collection remain substantial even when every row is cached. Further progress needs another focused pass on shared column setup and display-only rendering without changing editing or accessibility behavior. Scroll painting is improved but remains above a 16.7 ms frame budget in these instrumented runs.
