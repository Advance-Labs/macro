# Slack → Macro migration through Pipedream: scope and implementation plan

This document scopes what Macro needs so that a user can connect Slack through
the existing Pipedream Connect stack and migrate Slack **channels** (with
message history) and/or **people** ("contacts") into Macro. It records what
exists today with file references, the external constraints that shape the
design, the proposed architecture, data-model and API changes, a phased
delivery plan, a test plan, and the open questions that need a product or
infrastructure decision. Where a question is open, the plan states the default
assumption it was written against.

---

## 1. Summary

- **Connecting Slack via Pipedream already works end to end** in dev
  (`POST /pipedream/mcp/token` → hosted Connect iframe → `POST
  /pipedream/mcp/complete` → `pipedream_mcp_connections`). It is hidden in
  production by a single frontend gate (`pipedreamAppAvailableInEnv('slack')`),
  and the backend uses the Pipedream app slug `slack`, which Pipedream has since
  superseded with `slack_v2`.
- **Slack channel import already exists but is shallow.** The onboarding gather
  (`gather_slack_direct`) lists channels through the connector's MCP tool, stages
  up to 15 by member count, and the import creates an **empty** Macro channel
  (name + email-matched teammates). No messages, threads, reactions, files, or
  people are migrated, and there is no import surface outside onboarding.
- **The database is ready for history; the services are not.** `comms_messages`
  already has `imported_author`, `import_metadata`, and `import_order`, the UI
  already renders `imported_author`, and there is a precedent for idempotent
  backdated inserts (`macro_db_migrator` comment import). But no channel/message
  service accepts an explicit `created_at`, there is no bulk writer, and the
  only quiet mode (`PostMessageNotificationPolicy::Silent`) still fans out
  realtime events.
- **The main design choice is how to read Slack.** MCP tools are built for
  agents (truncation, prose output, name resolution) and are a poor bulk-read
  surface. Pipedream's **Connect API Proxy** lets the backend call the Slack Web
  API (`conversations.list/history/replies/members`, `users.list`, `files.info`)
  with the user's connected credentials and no token handling — Slack is listed
  as `proxy_enabled`. The plan uses the proxy for migration and keeps MCP for
  agent tooling.
- **Slack's May 2025 rate-limit change is the biggest external risk.** For
  non-Marketplace, commercially distributed apps, `conversations.history` and
  `conversations.replies` are limited to 1 request/minute returning at most 15
  messages. Marketplace-approved and internal apps keep Tier 3 (50+/min, up to
  ~1000 messages/page). Which limit applies depends on whether we use
  Pipedream's shared OAuth client (Pipedream's Marketplace-listed app) or a
  custom OAuth client backed by Macro's own Slack app. This must be verified in
  the dev project before Phase 1 (see §3 and §10).
- **Proposed shape:** new `SlackSource` port + Pipedream-proxy adapter, a
  `ChannelHistoryImporter` write path in the `channels` domain, a resumable
  per-channel history job table, an identity map (Slack user → Macro user /
  email / display name) that drives attribution, mention rewriting, team
  invites, and CRM contacts, plus a "Migrate from Slack" surface for existing
  users. Delivery is phased so channels + members ship first, then history, then
  people, then files/reactions and an optional Slack export-ZIP path for very
  large workspaces.

---

## 2. What exists today

### 2.1 Pipedream Connect stack

| Concern | Where | Notes |
| --- | --- | --- |
| Domain models/ports | `crates/pipedream_mcp/src/domain/{models.rs,ports.rs}` | `PipedreamConnection { user_id, app_slug, server_name, account_id, enabled }`; ports `ConnectionStore`, `McpConnection`, `PipedreamConnect`, `ConnectorDirectory`. |
| Connect flow | `crates/pipedream_mcp/src/domain/service/connect.rs` | `create_connect_token`, `complete_pipedream_connection` (verifies `external_user_id` matches caller), `disconnect_mcp_server`. |
| HTTP | `crates/pipedream_mcp/src/inbound/axum_router.rs` | `GET/PUT/DELETE /pipedream/mcp/connections`, `POST /pipedream/mcp/token`, `POST /pipedream/mcp/complete`, `GET /pipedream/mcp/catalog`, `POST /pipedream/mcp/webhook?secret=`. `PipedreamAuthCompletedHook` runs after a completed connection. |
| Outbound | `crates/pipedream_mcp/src/outbound/api.rs` | `PipedreamClient`: client-credentials bearer (cached), `X-PD-Environment`, Connect APIs under `/v1/connect/{project_id}/…`, remote MCP at `https://remote.mcp.pipedream.net` with `x-pd-*` scoping headers (`McpUpstreamCall`). **No proxy support yet.** |
| Persistence | `crates/macro_db_client/migrations/20260811160239_create_pipedream_mcp_connections.sql` | `pipedream_mcp_connections (user_id, app_slug, server_name, account_id, enabled, …)` PK `(user_id, app_slug)`. `account_id` is the `apn_…` id the proxy needs. |
| Config | `services/document_cognition_service/src/config.rs`, wired in `src/main.rs` | `PIPEDREAM_CLIENT_ID`, `PIPEDREAM_CLIENT_SECRET`, `PIPEDREAM_PROJECT_ID`, `PIPEDREAM_ENVIRONMENT`, `PIPEDREAM_API_URL`, `PIPEDREAM_MCP_URL`, `PIPEDREAM_ALLOWED_ORIGINS`, `PIPEDREAM_WEBHOOK_URI`, `PIPEDREAM_WEBHOOK_SECRET`. Missing credentials → endpoints answer 501. |
| Stack selection | `crates/mcp_select/src/lib.rs` | If a user has **any** Pipedream connection, only Pipedream tools are used; otherwise native MCP (`mcp_client`). `ConnectorRef { pipedream_app_slug, native_server_url }` identifies the same product on both stacks. |
| Frontend | `apps/web/src/lib/core/pipedream/connect-ui.ts`, `apps/web/src/features/settings/connections/*`, `apps/web/src/features/setup/*` | Vendored hosted-Connect iframe (`token`, `app` params only; no `oauthAppId`). Slack hidden in prod by `pipedreamAppAvailableInEnv` in `apps/web/src/lib/core/component/AI/constant/mcpServers.ts`; `QUICK_CONNECT_SERVERS` lists Slack only when `DEV_MODE_ENV`. |

### 2.2 Import pipeline and the current Slack path

| Concern | Where | Notes |
| --- | --- | --- |
| Model | `crates/import/src/domain/models.rs` | `ImportSource::{Linear, Notion, Slack}`; `Slack → pipedream_app_slug "slack"`, `mcp_server_url "https://mcp.slack.com/mcp"`, `entity_type "channel"`. `SlackChannelMeta { name, channel_id, purpose, participants: Vec<SlackParticipant{name,email}> }` capped at 25 participants. One run per `(user, source)`. |
| Ports | `crates/import/src/domain/ports.rs` | `ImportRepo` (CAS state machine: staged → importing → imported / discarded, heartbeat `touch_importing`, `fail_stale_importing`), `EntityCreator::{create_task, create_markdown_doc, create_channel}`. |
| Service | `crates/import/src/domain/service.rs` | `run_gather_session` → `gather_slack_direct` (connector channel-search tool, ≤5 pages, stages top 15 non-archived by member count, **participants always empty**), agent fallback. `import_deterministic` → `creator.create_channel(user, name, team_id, emails)`. In-process `tokio::spawn` jobs; no durable job state beyond the ledger row. |
| HTTP | `crates/import/src/inbound/axum_router.rs` | `GET /import/state`, `POST /import/run {import_ids, discard_ids}`, `POST /import/runs/{source}/retry`, `POST /import/runs/{source}/dismiss`. |
| AI tools | `crates/import/src/inbound/toolset.rs` | `CreateImportEntity`, `ImportNotionPage`, `DeleteImportEntity`, `ListImportEntities`, `FinalizeImport`. No Slack-specific tool. |
| Entity creation | `crates/ai_tools/src/tool_context.rs` (`ToolEntityCreator::create_channel`, `team_roster`) | `ChannelType::Team` when the user has a team else `Public`; `auto_join_team: false`; participants = creator + roster members matched by email (case-insensitive). |
| Persistence | `crates/macro_db_client/migrations/20260720221050_import_entities.sql`, `…20260723150434_support_auto_import_runs.sql` | `import_entity` has `CHECK (source IN ('linear','notion','slack'))` (plus `status`/`initiator` checks; `entity_type` is an unconstrained text column); `import_run` has the same `source` check and an `import_run_status` enum. Adding a source requires a migration. |
| Trigger | `crates/onboarding/src/domain/service.rs` (`start_due_gathers`, `reconcile`) | Only while onboarding is `Active`: for each connected `ImportSource`, `start_gather(user, source, auto_import = true)`. `PipedreamAuthCompletedHook` in DCS `main.rs` calls `reconcile`. Existing users who connect Slack later get **no** import. |
| Frontend | `apps/web/src/features/setup/{selection.ts, flow/SummaryStep.tsx, ImportEntityPill.tsx}`, `apps/web/src/lib/queries/import.ts`, `apps/web/src/lib/service-clients/service-cognition/import.ts` | Import UI exists only inside `/setup`. `SOURCE_SECTIONS` labels Slack items "channels". |

### 2.3 Channels and messages write path

| Concern | Where | Notes |
| --- | --- | --- |
| Models | `crates/channels/src/domain/models.rs` | `ChannelType::{Public, Private, DirectMessage, Team}`, `CreateChannelRequest`, `PostMessageRequest { content, mentions, thread_id, attachments, nonce, notification_policy (skip), triggered_by (skip) }`, `PostReactionRequest`, `AddParticipantsRequest`, `NewChannelAttachment`. `Sender = ChannelSender` (user **or** bot `bot\|<uuid>`; `crates/channel_sender`). |
| Service port | `crates/channels/src/domain/ports.rs` | `ChannelService::{create_channel, create_system_channel, create_channel_on_behalf, post_message, post_reaction, add_participants, …}`; `ChannelRepo::create_message(channel_id, sender, triggered_by, content, thread_id)`. |
| Insert | `crates/channels/src/outbound/pg_channels_repo.rs` (`create_message`) | `INSERT INTO comms_messages (id, channel_id, sender_id, triggered_by_user_id, content, thread_id)` — `created_at` defaults to `now()`; id is `macro_uuid::generate_uuid_v7()`. Reads order by `created_at, id`, so backdated rows sort correctly once `created_at` can be set. |
| Import columns | `crates/macro_db_client/migrations/20260917175816_messages_parent_aware_schema.sql` | `comms_messages.imported_author text`, `import_metadata jsonb`, `import_order bigint`; `comms_message_threads.import_metadata`; index `idx_comms_messages_thread_order (thread_id, import_order, created_at, id)`. |
| Precedent | `crates/macro_db_migrator/src/bin/comment_import/runner.rs` | Idempotent `INSERT … ON CONFLICT (id) DO UPDATE … WHERE updated_at <= EXCLUDED.updated_at` setting `sender_id` = owner, `imported_author` = original author text, explicit `created_at/updated_at`, `import_metadata`, `import_order`. |
| Rendering | `apps/web/src/features/channel/Message/SenderName.tsx`, `apps/web/src/lib/core/messages/types.ts` | `imported_author?.name` wins over the sender's name. |
| Quiet mode | `crates/messages/src/domain/models.rs` (`PostMessageNotificationPolicy::{Default, MentionsOnly, Silent}`) | `Silent` suppresses notifications only; realtime and search indexing still run per message. |
| Mentions | `packages/lexical-core/nodes/UserMentionNode.ts`, `crates/mention_utils/src/{serialize,parse}.rs`, `messages::SimpleMention::user` | Body chip `<m-user-mention>{"userId":"macro\|…","email":"…"}</m-user-mention>`; tracked mentions via `PostMessageRequest.mentions`. |
| Bots | `crates/bots` (`BotKind::{Owned, System}`, `bots`/`bot_tokens` tables), `MACRO_SYSTEM_BOT_ID` | Bots post as distinct senders with `BotSenderProfile`. |

### 2.4 People: roster, invites, contacts

| Concept | Where | Shape |
| --- | --- | --- |
| Team roster | `crates/teams` (`ListTeamMembers` tool, `team_roster` in `ai_tools/tool_context.rs`) | Macro users with emails; used today for Slack participant matching. |
| Team invites | `crates/teams/src/domain/team_service.rs` `invite_users_to_team(EntityAccessReceipt<MemberTeamRole>, NonEmpty<&[Email<Lowercase>]>)`; HTTP `POST /teams/{id}/invite` | Billing/seat and non-admin-invite policy live in the domain service; emits `TeamTopicEvent::InviteCreated`. |
| CRM contacts | `crates/crm/src/domain/{model.rs, service.rs}`; `/crm/*` routes | `CrmContact { id, company_id, email, name, hidden, first_interaction, last_interaction }` — team-scoped, email-keyed, must belong to a company (domain-derived). Provenance today is `crm_contact_sources (contact_id, link_id)` tied to email mailbox links. No phone/avatar/external-id fields. |
| Social graph | `crates/contacts` (`contacts_connections(user1,user2)`) | Edges between Macro user ids only; channels enqueue connections via `channels/src/outbound/contacts_dispatcher.rs`. |
| Address book | `email_contacts` (Gmail People sync) | Per-mailbox, not a migration target. |

`<m-contact-mention>` chips point at CRM contacts. There is no Slack → people import today.

### 2.5 Native Slack MCP app (for context)

`crates/mcp_client/src/domain/provider_registry/slack/{README.md,manifest.json}`
describes Macro's own Slack app (user-token scopes incl. `channels:history`,
`groups:history`, `channels:read`, `users:read`, `users:read.email`,
`files:read`, `search:read.*`) used by the native MCP stack
(`SLACK_MCP_CLIENT_ID/SECRET`). Slack's MCP server is restricted to
Marketplace-published or internal apps, which is the likely reason Slack is
dev-only in production today.

---

## 3. External constraints (verified)

### 3.1 Pipedream

- **Connect API Proxy.** `POST|GET https://api.pipedream.com/v1/connect/{project_id}/proxy/{url_safe_base64(url)}?external_user_id=…&account_id=apn_…` with the project bearer and `x-pd-environment`; Pipedream injects the user's upstream credentials. Headers prefixed `x-pd-proxy-` are forwarded. 30 s max per request (504 otherwise). Slack is `proxy_enabled: true` with `allowed_domains: ["slack.com"]`, so full URLs like `https://slack.com/api/conversations.history` work. Requires the `connect:proxy` OAuth scope on our Pipedream client.
- **App slug.** `pipedream.com/apps/slack` now redirects to `slack-v2`; the current API slug is **`slack_v2`** ("use in MCP headers and tool keys"). Prebuilt read actions exist: `slack_v2-list-channels`, `slack_v2-list-members-in-channel`, `slack_v2-list-users`, `slack_v2-get-channel-history`, `slack_v2-get-thread-replies`, `slack_v2-find-user-by-email`. Our code and `pipedream_mcp_connections` rows use `slack`. Both slugs must be handled during transition.
- **Managed OAuth scopes.** The public `slack_v2` page lists managed scopes `chat:write`, `chat:write.customize`, `chat:write.public`, `files:read`. That list is probably incomplete (the same app ships list/history actions), but it is a real risk that Pipedream's shared client does **not** grant `users:read.email` or private-channel scopes. Verify empirically (§3.3).
- **Custom OAuth clients.** Connect supports bringing our own Slack app (`POST /v1/connect/{project_id}/oauth_apps` → `oa_…`, register Pipedream's redirect URI in the Slack app; pass `oauthAppId` to the Connect UI / Connect Link). Custom clients also unlock credential retrieval, which we do not need.
- **Pipedream-managed MCP.** Remote MCP at `remote.mcp.pipedream.net` (our adapter) with `x-pd-app-slug`; tool names mangle to `mcp__{slug}__{tool}` (`crates/mcp_toolset/src/mangle.rs`). `is_slack_channel_search_tool_name` already matches `slack_v2-list-channels`.

### 3.2 Slack

- **Rate limits (changed 29 May 2025).** For apps created after that date and distributed outside the Slack Marketplace (and new installs of existing unlisted apps), `conversations.history` and `conversations.replies` are limited to **1 request/minute, ≤15 messages per request**. Marketplace-approved apps and internal customer-built apps keep **Tier 3 (50+/min)** with up to ~1000 messages per page. Other methods we need: `conversations.list` Tier 2 (20+/min, ≤1000/page), `conversations.members` Tier 4 (100+/min), `users.list` Tier 2, `users.info` Tier 4, `files.info` Tier 4, `conversations.join` Tier 3. Per-method, per-workspace, per-minute windows; `429` carries `Retry-After`.
- **Scopes (user token).** Channel list `channels:read`/`groups:read`; history `channels:history`/`groups:history`; members `channels:read`/`groups:read`; people `users:read` + `users:read.email`; files `files:read`; joining public channels `channels:join` (or `channels:write`). A user token only reads channels the connecting user can access (private channels require membership).
- **Slack MCP server.** Only Marketplace-published or internal apps may use it; same per-method rate limits as the Web API; outputs are prose-oriented. Fine for agents, not for bulk migration.
- **Export alternative.** Workspace owners can export public-channel history as a ZIP on every plan (private/DM data needs Business+ and approval). The format (`channels.json`, `users.json`, `<channel>/<YYYY-MM-DD>.json`) is stable and bypasses rate limits entirely.

### 3.3 Verification to do before Phase 1 (cheap, decisive)

1. In the **development** Pipedream project, connect a Slack account via the
   existing dev flow and record `app_slug` returned by `GET
   /v1/connect/{project_id}/accounts/{apn}` (`slack` vs `slack_v2`) and whether
   `GET /v1/connect/apps?q=slack` still lists `slack`.
2. Through the proxy, call `GET https://slack.com/api/auth.test` and read the
   `x-oauth-scopes` response header: this is the authoritative scope list for
   Pipedream's shared Slack client. Confirm presence of `channels:read`,
   `channels:history`, `groups:read`, `groups:history`, `users:read`,
   `users:read.email`, `files:read`.
3. Call `conversations.history?limit=200` on a busy channel 3× in a minute. If
   the second call 429s or `limit` is clamped to 15, Pipedream's app is
   subject to the unlisted-app limits for this install and the history design
   must assume the slow path (§9).
4. Confirm proxy call cost/credits with Pipedream account management (§10).

Outcome decides between **(A)** Pipedream shared client — zero Slack app work,
scopes fixed by Pipedream; **(B)** custom OAuth client backed by Macro's Slack app
(`A0B3XEX55GB`) — full control of scopes, but history limits depend on Macro's
app being Marketplace-approved or installed as an internal app.

---

## 4. Target architecture

Hexagonal boundaries checked against
`.claude/skills/cloud-storage-hexagonal-architecture/SKILL.md`: all new ports live
in a domain, adapters implement them, and wiring happens in
`services/document_cognition_service/src/main.rs`. No crate imports another
crate's `outbound` module.

```text
                    ┌──────────────── frontend ────────────────┐
                    │ Settings › Connections › Slack › Migrate │
                    │ /setup connector step (existing)          │
                    └───────────────┬──────────────────────────┘
                                    │ /import/* , /pipedream/mcp/*
┌───────────────────────────────────▼───────────────────────────────────────┐
│ document_cognition_service (composition root)                              │
│                                                                           │
│  import::domain::service::ImportServiceImpl                               │
│   ├─ gather: SlackSource::list_channels / list_users  (port)              │
│   ├─ channel import: EntityCreator::create_channel (existing)             │
│   ├─ history job: SlackSource::history/replies/members/files              │
│   │               SlackMessageConverter (pure, domain)                    │
│   │               ChannelHistoryWriter (port → channels domain)           │
│   ├─ people import: PeopleImporter (port → teams / crm)                   │
│   └─ identity map: SlackIdentityMap (domain value object)                 │
│                                                                           │
│  import::outbound::pipedream_slack::PipedreamSlackSource                  │
│      └─ uses pipedream_mcp::domain::ports::ConnectProxy                   │
│  pipedream_mcp::outbound::api::PipedreamClient  (impl ConnectProxy)       │
│  ai_tools::ToolEntityCreator (+ ChannelHistoryWriter, PeopleImporter)     │
│      └─ channels::ChannelService::import_history (new)                    │
│      └─ teams::TeamService::invite_users_to_team, crm::CrmService         │
└───────────────────────────────────────────────────────────────────────────┘
```

### 4.1 Pipedream: make Slack connectable in production

Files: `crates/pipedream_mcp`, `crates/import/src/domain/models.rs`,
`crates/mcp_select`, `apps/web/src/lib/core/component/AI/constant/mcpServers.ts`,
`apps/web/src/features/settings/connections/*`, `apps/web/src/features/setup/*`.

1. **Slug transition.** Add a per-source list of accepted Pipedream slugs:
   `ImportSource::pipedream_app_slugs(self) -> &'static [&'static str]`
   (`Slack → ["slack_v2", "slack"]`, preferred first) and extend
   `mcp_select::ConnectorRef` to carry `pipedream_app_slugs: &[&str]`
   (`connector_toolset`/`connector_connected` match any). New connections use
   the preferred slug; existing `slack` rows keep working. Frontend constants
   (`QUICK_CONNECT_SERVERS`, `PIPEDREAM_ICON_MAP`, `CURATED_AI`,
   `PipedreamAiProvider`, `onboardingConnectorConfig`) switch `app_slug` to the
   preferred slug via one shared helper so icons and status rows resolve either.
2. **Optional custom OAuth client.** Add config `PIPEDREAM_SLACK_OAUTH_APP_ID`
   (loaded through `macro_config`, registered in Doppler) and return it from
   `POST /pipedream/mcp/token` (or a new `GET /pipedream/mcp/apps/{slug}/connect-options`)
   so `openPipedreamConnectUI` can pass `oauthAppId`. Only needed if §3.3 picks
   option B.
3. **Connection health.** `PipedreamAccount.healthy` is fetched but not
   persisted. Persist `healthy`/`last_verified_at` on completion and before a
   migration run (`get_account`) so the UI can show "reconnect Slack" instead of
   a failing job.
4. **Production gate.** Replace the hard-coded `appSlug !== 'slack'` in
   `pipedreamAppAvailableInEnv` with a `defineFlag`-based feature flag
   (`slack-pipedream`, per `.claude/skills/define-feature-flag/SKILL.md`) so
   rollout is per-team/user rather than per-environment. Keep the native Slack
   MCP entry gated separately (it depends on Marketplace status of Macro's app).

### 4.2 `SlackSource` port and Pipedream-proxy adapter

Files: `crates/import/src/domain/ports.rs` (port),
`crates/import/src/outbound/pipedream_slack.rs` (adapter),
`crates/pipedream_mcp/src/domain/ports.rs` + `outbound/api.rs` (`ConnectProxy`).

```rust
// pipedream_mcp::domain::ports — generic, app-agnostic
pub trait ConnectProxy: Send + Sync + 'static {
    /// Forward one HTTP request to an upstream API with the connected
    /// account's credentials injected by Pipedream.
    fn proxy(&self, req: ProxyRequest<'_>) -> impl Future<Output = anyhow::Result<ProxyResponse>> + Send;
}
pub struct ProxyRequest<'a> { pub connection: &'a PipedreamConnection, pub method: Method, pub url: &'a str, pub query: &'a [(&'a str, String)], pub body: Option<serde_json::Value> }
pub struct ProxyResponse { pub status: u16, pub headers: Vec<(String, String)>, pub body: serde_json::Value }

// import::domain::ports — Slack-shaped, transport-agnostic
pub trait SlackSource: Send + Sync + 'static {
    fn workspace(&self, user: &MacroUserIdStr<'static>) -> … Result<SlackWorkspace>;          // auth.test (team id/name/domain, granted scopes)
    fn list_channels(&self, user, cursor: Option<&str>, kinds: ChannelKinds) -> … Result<Page<SlackChannel>>;   // conversations.list
    fn channel_members(&self, user, channel: &str, cursor) -> … Result<Page<String>>;        // conversations.members
    fn list_users(&self, user, cursor) -> … Result<Page<SlackUser>>;                          // users.list
    fn history(&self, user, channel: &str, oldest: Option<SlackTs>, latest: Option<SlackTs>, cursor) -> … Result<Page<SlackMessage>>; // conversations.history
    fn replies(&self, user, channel: &str, thread_ts: &SlackTs, cursor) -> … Result<Page<SlackMessage>>;        // conversations.replies
    fn file(&self, user, file_id: &str) -> … Result<SlackFile>;                               // files.info
    fn download(&self, user, url_private: &str) -> … Result<Bytes>;                           // url_private_download via proxy (needs files:read)
    fn join_channel(&self, user, channel: &str) -> … Result<()>;                              // conversations.join (optional)
}
```

Adapter behaviour (`PipedreamSlackSource`):

- Resolves the user's Slack `PipedreamConnection` via `ConnectionStore` (any
  accepted slug), builds proxy calls, decodes Slack's `{ok, error, …}` envelope
  into typed errors (`SlackError::{RateLimited{retry_after}, MissingScope(scope),
  NotInChannel, ChannelNotFound, TokenRevoked, Other}`), and surfaces Slack's
  `response_metadata.next_cursor`.
- A **rate governor** keyed by `(workspace, method)`: token bucket seeded from
  the tier table in §3.2, honouring `Retry-After` on 429, with jittered backoff.
  Lives in the adapter (infrastructure), configured by the composition root.
- Caps page sizes (`limit=200` for history/replies, `1000` for lists) and
  measures effective page size on the first history call so the job scheduler
  can detect the slow path (15/min) and switch strategy/ETA.
- Zero Slack tokens in Macro: only `account_id` + `external_user_id` are sent.

Also implement `SlackSource` for the **agent MCP path**? No: agents keep using
MCP tools; the import pipeline stops depending on MCP tool-name heuristics
(`slack_channel_search_tool_name`) once the proxy adapter exists. Keep the MCP
gather as a fallback behind a feature flag during rollout, then delete it.

### 4.3 Channel import (what exists, extended)

Files: `crates/import/src/domain/{models.rs,service.rs}`,
`crates/ai_tools/src/tool_context.rs`.

- **Gather = full enumeration, not top-15.** `gather_slack` stages every
  non-archived channel the user can see (public + private the user is in), with
  richer `SlackChannelMeta`:
  `{ name, channel_id, purpose, topic, is_private, is_member, is_archived, member_count, created, last_message_ts, participants: [] }`.
  Keep the 15-channel default **selection** for onboarding auto-import (UI
  pre-ticks the top 15 by member count; everything else is staged but
  unticked), so onboarding behaviour is unchanged while the migration picker
  gets the full list. Paging: `conversations.list` 1000/page → one or two calls.
- **Members at import time, not gather time.** On accept,
  `conversations.members` + the identity map (§4.5) decide participants:
  roster matches join; others are recorded for the people step. Removes the
  current "participants always empty" gap without bloating staged rows.
- **Channel type mapping.** Public Slack channel → `ChannelType::Team` (as
  today) when the user has a team; private Slack channel → `ChannelType::Private`
  with explicit participants. Archived channels are importable only from the
  migration picker and are created as Team/Private channels with a
  `(archived)` suffix option (Macro has no archived state). Default: skip.
- **Idempotency.** `foreign_id` = Slack channel id (already normalised). Team-wide
  dedup via `import_entity_team_imported_idx` stays as is.
- **Purpose/topic → channel description.** `PatchChannelRequest` has no
  description field today; if channels gain one, set it; otherwise post the
  purpose as the first imported system message (`imported_author: "Slack"`).

### 4.4 Channel history import

#### 4.4.1 Write path in the `channels` domain

Files: `crates/channels/src/domain/{models.rs,ports.rs,service.rs}`,
`crates/channels/src/outbound/pg_channels_repo.rs`, `crates/messages`.

Add an explicit bulk import use case rather than overloading `post_message`:

```rust
// channels::domain::models
pub struct ImportedMessage {
    pub external_id: String,                 // Slack ts (unique within channel)
    pub external_thread_id: Option<String>,  // Slack thread_ts of the parent, when a reply
    pub sender: ChannelSender<'static>,      // policy decided by the import domain (§4.5)
    pub imported_author: Option<String>,     // Slack display name when sender is not the real author
    pub content: String,                     // Macro markdown
    pub mentions: Vec<SimpleMention>,
    pub created_at: DateTime<Utc>,
    pub edited_at: Option<DateTime<Utc>>,
    pub import_order: i64,                   // position within its thread (0 = parent)
    pub import_metadata: serde_json::Value,  // {"source":"slack","team":"T…","channel":"C…","ts":"…","user":"U…","subtype":…,"permalink":…}
    pub reactions: Vec<ImportedReaction>,    // {emoji, external_user_ids}
    pub attachments: Vec<NewChannelAttachment>,
}
pub struct ImportHistoryBatch { pub channel_id: Uuid, pub messages: Vec<ImportedMessage> }
pub struct ImportHistoryOutcome { pub inserted: u64, pub skipped_existing: u64, pub id_map: Vec<(String, Uuid)> }

// channels::domain::ports::ChannelService
fn import_history(&self, actor: EntityAccessReceipt<OwnerParticipantRole>, batch: ImportHistoryBatch)
    -> impl Future<Output = Result<ImportHistoryOutcome, ChannelMutationErr>> + Send;

// channels::domain::ports::ChannelRepo
fn insert_imported_messages(&self, channel_id: Uuid, rows: &[ImportedMessageRow]) -> … Result<Vec<InsertedImportedMessage>>;
fn insert_imported_reactions(&self, rows: &[ImportedReactionRow]) -> … Result<u64>;
```

Behaviour:

- Insert sets `id` (UUIDv7 derived from the Slack timestamp so ids sort with
  time; add `macro_uuid::uuid_v7_at(DateTime)`), `created_at`, `updated_at`,
  `edited_at`, `sender_id`, `imported_author`, `import_metadata`, `import_order`,
  `thread_id` (resolved from `external_thread_id` via the batch's own id map or a
  lookup on `import_metadata->>'ts'`), `parent_entity_type='channel'`.
- **Idempotent** with `ON CONFLICT DO NOTHING` on a new unique partial index
  (`comms_messages (parent_entity_id, (import_metadata->>'source'), (import_metadata->>'ts')) WHERE import_metadata ? 'ts'`),
  so a resumed job can replay a page safely.
- **Side effects:** no notifications, no per-message realtime fanout, no
  per-message search indexing. Instead the service emits one
  `channel_updated` nudge at the end of each batch and enqueues a bulk
  re-index for the channel (check the search indexer's existing message
  indexing hook and give it a batch entry point). Mentions are recorded in
  `comms_entity_mentions` for the matched Macro users but **do not notify**.
- Thread roots: when a Slack parent has replies, write a `comms_message_threads`
  row with `import_metadata` (reply count, last reply ts).
- Authorization: the inbound side (import service) obtains an
  `EntityAccessReceipt<OwnerParticipantRole>` for the channel it just created;
  `import_history` is only callable with it. Policy stays in the channels domain.

#### 4.4.2 History job orchestration in the `import` domain

Files: `crates/import/src/domain/{models.rs,ports.rs,service.rs}`,
`crates/import/src/outbound/pg_import_repo.rs`, new migration.

Today an `import_entity` row flips `importing → imported` in one step and a
failure returns it to `staged`. History is long-running and must not duplicate
the channel on retry, so split it out:

- The channel row becomes `imported` as soon as the Macro channel exists.
- A new table **`import_job`** tracks per-row follow-up work:

  ```sql
  CREATE TABLE import_job (
      id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
      user_id          TEXT NOT NULL,
      import_entity_id UUID NOT NULL REFERENCES import_entity (id) ON DELETE CASCADE,
      kind             TEXT NOT NULL,            -- 'slack_history' | 'slack_files'
      status           TEXT NOT NULL DEFAULT 'queued',  -- queued | running | done | failed | cancelled
      options          JSONB NOT NULL DEFAULT '{}',     -- {"since": "...", "until": "...", "include_files": bool, "include_reactions": bool}
      cursor           JSONB NOT NULL DEFAULT '{}',     -- {"phase":"history|threads|files","history_cursor":"…","oldest_done":"…","thread_queue":[…],"thread_idx":n}
      stats            JSONB NOT NULL DEFAULT '{}',     -- {"messages":n,"threads":n,"replies":n,"files":n,"skipped":n,"rate_limited_s":n}
      error            TEXT,
      attempts         INT  NOT NULL DEFAULT 0,
      heartbeat_at     TIMESTAMPTZ,
      created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
      updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
      UNIQUE (import_entity_id, kind)
  );
  CREATE INDEX import_job_user_status_idx ON import_job (user_id, status);
  ```

- `ImportRepo` gains `create_job`, `claim_next_job(user)` (CAS `queued|failed →
  running`, sets `heartbeat_at`), `save_job_cursor`, `heartbeat_job`,
  `finish_job`, `fail_job`, `list_jobs(user)`, `reap_stale_jobs(older_than)`.
  Reaping reuses the existing heartbeat idea (`IMPORT_HEARTBEAT`,
  `STALE_IMPORT_AFTER`).
- **Runner.** `HistoryJobRunner` (in `import::domain::service`) runs per user
  with bounded concurrency (default 2 channels at once) and resumes from
  `cursor`:
  1. `phase = history`: walk `conversations.history` newest → oldest (Slack's
     natural order) with `latest = oldest_done`, converting and writing each
     page as one `ImportHistoryBatch`; record parents with `reply_count > 0`
     in `thread_queue`; advance `cursor` **after** the batch is written.
  2. `phase = threads`: for each queued parent, page `conversations.replies`
     and write replies (skip the parent echo); `import_order` = reply index.
  3. `phase = files` (if enabled): §4.6.
  4. `done`: final `channel_updated` nudge + search re-index.
- **Window and size guards.** `options.since` (default: all history; onboarding
  default: last 90 days — question Q6), a hard cap per channel
  (`SLACK_IMPORT_MAX_MESSAGES_PER_CHANNEL`, default 50 000) and a per-user daily
  budget, both configured through `macro_config`.
- **Slow-path detection.** If the adapter reports the 15-message clamp, the
  runner sets `stats.slow_path = true`, lowers concurrency to 1 and publishes an
  ETA (`messages_remaining / 15 per min`) so the UI can suggest the export
  alternative (§7 Phase 4) instead of silently running for days.
- **Visibility.** `ImportState` gains `jobs: Vec<ImportJobView>` (status,
  stats, eta, error) so the existing poll + `import_updated` websocket nudge
  (`import::outbound::gateway_notifier`) drive progress UI.

#### 4.4.3 Slack message → Macro markdown conversion (pure domain code)

File: `crates/import/src/domain/slack_convert.rs` (+ tests). Works from Slack's
`text` (mrkdwn) with `blocks[].rich_text` as the preferred source when present.

| Slack | Macro |
| --- | --- |
| `*bold*`, `_italic_`, `~strike~`, `` `code` ``, ```` ``` ```` | `**bold**`, `*italic*`, `~~strike~~`, same code spans/fences |
| `<https://x\|label>` / `<https://x>` | `[label](https://x)` / bare URL |
| `<@U123>` / `<@U123\|name>` | `<m-user-mention>{"userId":"macro\|…","email":"…"}</m-user-mention>` when mapped to a Macro user, else `@Display Name` |
| `<#C123\|name>` | link to the imported Macro channel (`/app/channel/<id>`) when that channel was imported in the same run, else `#name` |
| `<!here>`, `<!channel>`, `<!everyone>`, `<!subteam^S…\|@group>` | plain `@here`, `@channel`, `@everyone`, `@group` (no Macro broadcast) |
| `&amp; &lt; &gt;` | unescaped |
| `:emoji:` shortcodes | Unicode via a bundled gemoji table; custom emoji keep `:name:` |
| Quote `&gt; text` lines | `> text` |
| `attachments[]` (legacy unfurls/bot cards) | `title` + `text` + `title_link` appended as a quoted block |
| `files[]` | placeholder line `📎 name (type, size)` until Phase 3 attaches the file |

Subtype policy: import `None`, `thread_broadcast`, `bot_message`
(`imported_author` = bot username), `file_share`, `me_message` (italicised);
import `channel_topic`/`channel_purpose` as system lines; **skip**
`channel_join`, `channel_leave`, `channel_archive`, `channel_unarchive`,
`pinned_item`, `tombstone`, `joiner_notification*`, `ekm_access_denied` and
anything with `hidden: true`. All skips are counted in `stats.skipped`.

Timestamps: `ts = "1699999999.000123"` → `created_at`; `edited.ts` → `edited_at`.

### 4.5 Identity map and attribution

File: `crates/import/src/domain/slack_identity.rs`.

`SlackIdentityMap` is built once per run from `users.list` (+ `users.info` for
`is_stranger` Slack Connect users that `users.list` omits) and the Macro team
roster:

```rust
pub struct SlackIdentity { pub slack_id: String, pub display_name: String, pub real_name: Option<String>, pub email: Option<String>, pub title: Option<String>, pub avatar_url: Option<String>, pub tz: Option<String>, pub kind: SlackUserKind /* Member | Admin | Guest | SingleChannelGuest | External | Bot | Deleted */ }
pub enum Resolution { MacroUser(MacroUserIdStr<'static>), Teammate /* same domain, not yet on team */, External, Unresolvable }
```

Attribution policy for imported messages (decided in the import domain, applied
by `import_history`):

- `sender_id` = a dedicated **system bot "Slack import"** (`BotKind::System`,
  created on first use via the bots service, neutral avatar) and
  `imported_author` = the Slack display name, **for every message** — even
  when the author is a known Macro user. This avoids writing messages "as" a
  teammate (which would also route notifications/permissions through them) and
  matches the comment-import precedent (`sender` = owner, `imported_author` =
  original). `import_metadata.user` keeps the Slack id and resolved email so a
  later "claim my Slack history" feature can re-attribute. Question Q4 covers
  the alternative (attribute directly to matched Macro users).
- Mentions of mapped users become real `<m-user-mention>` chips and
  `SimpleMention::user` rows (no notification).
- The map is cached per `(user, slack team)` in the job cursor; a fresh run
  rebuilds it (Tier 2, one or two calls).

### 4.6 Files and reactions (Phase 3)

- Files: `files.info` → `url_private_download` → fetched through the proxy
  with `files:read` → uploaded via the existing static-file/document upload path
  used by channel attachments → `NewChannelAttachment { entity_type, entity_id,
  width, height }` attached to the imported message (second pass per message to
  keep page writes small). Size cap (default 25 MB/file, 2 GB/channel), MIME
  allow-list, and dedupe by Slack file id in `import_metadata`.
- Reactions: `comms_reactions (message_id, emoji, user_id)` requires a Macro
  user; import reactions only for mapped users (others counted in `stats`), via
  a bulk `insert_imported_reactions`. Emoji shortcodes → Unicode as above.

### 4.7 People ("contacts") migration

Files: `crates/import/src/domain/{models.rs,service.rs}`,
`crates/ai_tools/src/tool_context.rs`, new migration, frontend picker.

Add **`ImportSource::SlackPeople`** (serialised `slack_people`): same connector
(`pipedream_app_slugs` = Slack's, `mcp_server_url` = Slack's), `entity_type`
is decided **per row** by the user's choice, which needs the ledger's fixed
source → entity-type mapping (`ImportSource::entity_type`) to relax for this
source only (`team_invite` or `contact`; the column itself is unconstrained
text, see §5 M1). The alternative — folding
people into the Slack channel run — breaks the "one run per source, one entity
type per source" model the UI and repo rely on; a sibling source is the least
invasive change. The `import_entity`/`import_run` `CHECK (source IN …)`
constraints need a migration either way.

Gather (`users.list`): stage one row per non-bot, non-deleted Slack user who is
**not** already on the Macro team, with:

```rust
pub struct SlackPersonMeta { pub slack_id: String, pub display_name: String, pub real_name: Option<String>, pub email: Option<String>, pub title: Option<String>, pub avatar_url: Option<String>, pub kind: SlackUserKind, pub shared_channels: u32, pub suggestion: PersonAction /* InviteToTeam | AddContact | Skip */ }
pub enum PersonAction { InviteToTeam, AddContact, Skip }
```

Suggestion rules: email domain == team domain and not a guest → `InviteToTeam`;
guest / external / other domain with an email → `AddContact`; no email →
`Skip` (cannot create either). The user confirms per row or per bucket in the
picker; the chosen action is stored on the row (`metadata.action`) when
accepted via `POST /import/run` with per-id options.

Import (`PeopleImporter` port implemented in `ai_tools::ToolEntityCreator`):

- `InviteToTeam` → `TeamService::invite_users_to_team` with a
  `EntityAccessReceipt<MemberTeamRole>` minted for the importing user. Seat,
  billing, and "non-admin invites disabled" policy stay in the teams domain;
  domain errors surface as `last_error` per row (e.g. "team is full"). The
  entity id recorded is the `team_invite_id`.
- `AddContact` → `CrmService::populate_contact`-style upsert with a new
  provenance: add `crm_contact_sources.source_kind text NOT NULL DEFAULT
  'email_link'` + `external_ref text` (migration; `link_id` becomes nullable
  for non-email sources) so Slack-imported contacts show "from Slack" and can be
  re-synced/removed. Company is domain-derived as today. The entity id recorded
  is the `crm_contacts.id`.
- The identity map (§4.5) is the shared input; people rows that resolve to an
  existing Macro user are never staged (they are "already here").

Non-goals for people: creating Macro accounts on someone's behalf, importing
Slack user groups as Macro groups, or writing to the social `contacts` graph
(it is derived from channel participation and will fill itself).

### 4.8 Frontend

Files: `apps/web/src/features/settings/connections/*`, new
`apps/web/src/features/import-slack/*` (layered per
`docs/FRONTEND_FEATURE_ARCHITECTURE.md`), `apps/web/src/features/setup/*`,
`apps/web/src/lib/queries/import.ts`, `apps/web/src/lib/service-clients/service-cognition/import.ts`,
`packages/sdk` (regenerated cognition client).

1. **Enablement:** flag-driven Slack availability (§4.1); Slack row in
   Settings › Connections shows account/workspace name and a **Migrate from
   Slack** action when connected.
2. **Migration view (existing users):** two tabs backed by `/import/state`.
   - *Channels:* searchable list of staged channels (name, members, private
     badge, last activity, "not a member" hint with "Join in Slack" link or
     auto-join toggle), bulk select, per-run options: history (none / since
     date / all), include files, include reactions. Accept → `POST /import/run`
     with `options`. Progress column from `jobs` (messages imported, ETA,
     slow-path warning linking to the export alternative, retry/cancel).
   - *People:* buckets "Invite to team" / "Add as contact" / "Skip" with
     per-row override; shows seat implications from the teams API before
     confirming.
3. **Onboarding:** keep the current Slack step; copy already promises channels
   "with the right participants" (`OnboardingFlow.tsx`), which becomes true via
   §4.3. Add a single toggle "Also bring the last 90 days of messages" (default
   per Q6); history runs as background jobs after onboarding completes, visible
   in the migration view.
4. **Channel UI:** imported messages already render `imported_author`. Add a
   subtle "Imported from Slack · open original" affordance using
   `import_metadata.permalink`, and a channel header banner while a history job
   is running ("Importing history from Slack… 1,240 messages so far").
5. **Docs:** update `docs/AGENT_GUIDE/surfaces.md` (Connections, migration
   view) and the onboarding section, per the repository guardrail on
   user-visible changes.

### 4.9 AI tool surface (optional, Phase 2+)

Add `ImportSlackChannel` to `crates/import/src/inbound/toolset.rs` (mirrors
`ImportNotionPage`): stage-as-chat, create the channel, enqueue a history job
with `since`, return the channel link. Add
`apps/docs/AI/mcp/tools/import-slack-channel.mdx` and a renderer in
`apps/web/src/lib/core/component/AI/component/tool/ImportTools.tsx`. Record the
endpoint/tool per `.claude/skills/add-sdk-endpoint/SKILL.md`.

---

## 5. Data model changes (migrations)

All via `sqlx migrate add --source crates/macro_db_client/migrations <name>`,
compatible with currently deployed code (additive, nullable/defaulted), then
`nix develop --command just prepare_db` and commit `.sqlx`.

| # | Change | Why |
| --- | --- | --- |
| M1 | `import_entity`/`import_run`: drop and recreate `CHECK (source IN …)` to add `slack_people`. `entity_type` is unconstrained text, so `team_invite` / `contact` need no schema change (the Rust `ImportSource::entity_type` mapping is what relaxes). | New source. |
| M2 | New `import_job` table (§4.4.2) + indexes. | Resumable history/files jobs. |
| M3 | `comms_messages`: unique partial index on `(parent_entity_id, (import_metadata->>'source'), (import_metadata->>'ts')) WHERE import_metadata ? 'ts'`. | Idempotent replays. |
| M4 | `pipedream_mcp_connections`: add `healthy boolean NOT NULL DEFAULT true`, `last_verified_at timestamptz`, `account_label text` (workspace name). | Reconnect UX, migration preflight. |
| M5 | `crm_contact_sources`: add `source_kind text NOT NULL DEFAULT 'email_link'`, `external_ref text`; make `link_id` nullable; partial unique on `(contact_id, source_kind, external_ref)`. | Slack provenance for contacts. |
| M6 (optional) | `comms_channels.description text` | Carry Slack purpose/topic. |

No new env-var secrets beyond the optional `PIPEDREAM_SLACK_OAUTH_APP_ID`;
tunables `SLACK_IMPORT_MAX_MESSAGES_PER_CHANNEL`, `SLACK_IMPORT_CONCURRENCY`,
`SLACK_IMPORT_DEFAULT_SINCE_DAYS`, `SLACK_IMPORT_MAX_FILE_BYTES` via
`macro_config` with defaults, registered in Doppler.

---

## 6. API changes

### 6.1 HTTP (document_cognition_service, `/import/*`, `/pipedream/mcp/*`)

| Method | Path | Change |
| --- | --- | --- |
| GET | `/import/state` | Response adds `jobs: ImportJobView[]`; `ImportEntity.metadata` for Slack gains the richer `SlackChannelMeta`; new source `slack_people` rows. |
| POST | `/import/run` | Body adds `options?: { history?: { mode: 'none' \| 'since' \| 'all', since?: string }, include_files?: bool, include_reactions?: bool, people?: Record<uuid, 'invite_to_team' \| 'add_contact'> }`. |
| POST | `/import/runs/{source}/refresh` | New: re-gather for a source outside onboarding (CAS from `ready` / `completed` / `failed` / `dismissed`). Existing `retry` keeps its semantics. |
| POST | `/import/jobs/{id}/retry`, `/import/jobs/{id}/cancel` | New: job controls. |
| GET | `/import/slack/preflight` | New: `auth.test` via proxy → workspace name, granted scopes, missing scopes, detected rate tier; drives the UI's "reconnect with more permissions" and slow-path messaging. |
| POST | `/pipedream/mcp/token` | Response adds optional `oauth_app_id` for the requested app (only if option B). |

All handlers stay thin (`MacroAuthorizationExtractor` → service call → status
mapping) as in `crates/import/src/inbound/axum_router.rs`.

### 6.2 SDK / frontend clients

Regenerate `packages/sdk/generated/cognition/*` from the updated OpenAPI
(`packages/sdk/specs/cognition.json`), update
`apps/web/src/lib/service-clients/service-cognition/import.ts` and
`apps/web/src/lib/queries/import.ts`; run `just coverage` and follow the
`add-sdk-endpoint` skill for new endpoints.

---

## 7. Phased delivery

**Phase 0 — Verify and enable (small, unblocks everything)**
Run §3.3 in the dev project. Slug transition (`slack_v2` + legacy `slack`),
`ConnectProxy` port + `PipedreamClient` implementation (unit-tested against a
recorded Slack envelope), `GET /import/slack/preflight`, connection health
columns, feature flag replacing the env gate. Decide option A vs B and record
it here.

**Phase 1 — Channels done properly**
`SlackSource` + `PipedreamSlackSource`, full-enumeration gather with richer
metadata, members at import time via the identity map, private → `Private`
channel mapping, `/import/runs/{source}/refresh`, Settings › Connections
"Migrate from Slack" with the Channels tab (no history yet). Onboarding keeps
its top-15 default. Delete the MCP tool-name heuristics once the proxy gather
is stable.

**Phase 2 — History**
`channels::import_history` + repo bulk insert + idempotency index + quiet side
effects; `import_job` table and runner with resume/heartbeat/reaping; mrkdwn
converter; attribution via the "Slack import" system bot; progress in
`/import/state`, migration-view progress UI, channel banner; onboarding
"last 90 days" toggle. Threads included; files/reactions behind flags.

**Phase 3 — People**
`ImportSource::SlackPeople`, `SlackPersonMeta`, `PeopleImporter`
(team invites + CRM contacts with Slack provenance), People tab, chat tool
`ImportSlackChannel` (optional).

**Phase 4 — Scale and polish**
Files and reactions (§4.6), search bulk re-index, "open original" affordance,
and — if §3.3 shows the slow path is common — a **Slack export ZIP** upload
that implements `SlackSource` over the export format, reusing the converter,
identity map, and `import_history` unchanged.

Each phase ships behind the `slack-pipedream` flag; Phase 0 and 1 are
prerequisites for the rest, Phases 2 and 3 are independent of each other.

---

## 8. Testing plan

- **Unit (domain, no I/O):** mrkdwn → markdown table-driven tests incl.
  mention/channel/link edge cases and HTML entities; subtype policy; identity
  resolution rules (domain match, guest, external, bot, deleted);
  `select_slack_candidates` ordering; job cursor state machine (resume after
  each phase, replay a page, cancel); rate governor (token bucket, 429 with
  `Retry-After`, slow-path detection) with a fake clock.
- **Service tests with fake ports** (pattern in
  `crates/import/src/domain/service/test.rs`): gather stages all channels with
  correct metadata; accept creates the channel then a `slack_history` job;
  failure after channel creation does not re-create on retry; people import
  maps errors from `TeamService`/`CrmService` to `last_error`; attribution
  always uses the system bot + `imported_author`.
- **Repository tests against the local DB** (`cargo test -p import`,
  `cargo test -p channels`, `SQLX_OFFLINE` unset): `insert_imported_messages`
  idempotency (same batch twice → `skipped_existing`), thread resolution,
  ordering by `created_at`, `import_job` CAS/heartbeat/reap, migrations apply
  on an existing schema.
- **Adapter tests:** `PipedreamClient::proxy` and `PipedreamSlackSource`
  against a local HTTP stub returning recorded Slack envelopes (`ok:false`
  errors, `ratelimited`, pagination); no live Pipedream in CI.
- **Hexagonal lint:** run the skill's `rg` checks on `crates/import`,
  `crates/channels`, `crates/pipedream_mcp` for every PR.
- **Browser verification** (per `apps/web/AGENTS.md`): connect Slack in dev,
  run a migration of a small test workspace, confirm channels, participants,
  message order/threads/mentions, `imported_author` rendering, progress UI,
  and the reconnect/slow-path states; update `docs/AGENT_GUIDE/surfaces.md`.
- **Load sanity:** one channel with ~20k messages and ~500 threads through the
  dev project to measure wall-clock vs. tier and tune concurrency/page sizes.

---

## 9. Risks and mitigations

| Risk | Impact | Mitigation |
| --- | --- | --- |
| Pipedream shared Slack client lacks needed scopes (`users:read.email`, `groups:*`, `files:read`) | No emails → weak identity map; no private channels; no files | Preflight endpoint surfaces missing scopes; option B (custom OAuth client with Macro's app) if confirmed. |
| 1 req/min × 15 messages history limit applies | 50k messages ≈ 55 h; onboarding history impossible | Detect on first page; cap to recent window; show ETA; Phase 4 export-ZIP path; pursue Marketplace listing for Macro's app if option B. |
| Pipedream proxy credits/cost per request | Large migrations could be expensive | Confirm pricing (Q9); budget per user/day; prefer 200–1000-message pages. |
| Duplicate channels/messages on retry | Data corruption | Ledger `imported` before history; `import_job` separate; unique `(channel, source, ts)` index; `ON CONFLICT DO NOTHING`. |
| Notification/realtime storms from bulk inserts | Spam, load | Dedicated `import_history` path with no per-message side effects; one nudge per batch; bulk re-index. |
| Impersonation concerns | Trust | Never set `sender_id` to a real user for imported content; system bot + `imported_author`; keep Slack ids for later opt-in claiming. |
| In-process jobs die on deploy | Partial imports | Durable `import_job` cursor + heartbeat + reap-and-resume on next read/tick, as the ledger does today. |
| Slug transition (`slack` → `slack_v2`) | Existing connections stop matching | Multi-slug `ConnectorRef`; frontend helper; backfill not required. |
| Seat limits / billing when inviting many people | Failed invites | Teams domain already enforces; surface per-row errors and a pre-count in the UI. |
| Privacy (DMs, private channels, guests) | Compliance | DMs are a non-goal; private channels only when the connecting user is a member; require team admin for bulk history/people (Q3). |

---

## 10. Open questions (with the assumption the plan uses)

1. **OAuth client (A: Pipedream shared vs B: Macro's Slack app via custom
   OAuth client)?** Decided by §3.3. *Assumption:* A if scopes and Tier 3 hold;
   otherwise B, which also requires Macro's Slack app to be Marketplace-approved
   or installed as an internal app to avoid the 15-messages/minute limit.
2. **Is Macro's Slack app (`A0B3XEX55GB`) Marketplace-listed, or planned to
   be?** This also gates the native Slack MCP in production. *Assumption:* not
   yet listed.
3. **Who may run a migration?** *Assumption:* any team member can import
   channels they belong to (today's behaviour); history and people migration
   require the team **admin** role (`AdminTeamRole` receipt) because they invite
   people and consume seats.
4. **Attribution:** always the "Slack import" system bot + `imported_author`
   (assumed), or attribute messages directly to matched Macro users?
5. **Scope of channels:** public only, or public + private the connector is a
   member of? Archived channels? *Assumption:* public + private-as-member;
   archived off by default; DMs/group DMs never.
6. **History defaults:** onboarding = last 90 days (toggle), migration view =
   user choice (none / since / all), hard cap 50 000 messages per channel.
   Confirm the window and cap.
7. **"Contacts" semantics:** is the goal (a) invite Slack teammates to the
   Macro team, (b) create CRM contacts for external people, or both?
   *Assumption:* both, with the bucket rules in §4.7 and CRM provenance
   "from Slack". Should contacts also carry title/phone/avatar (CRM has no
   fields today)?
8. **Files:** include file migration in scope now (storage cost, 25 MB/file
   cap) or defer? *Assumption:* Phase 4 behind a flag.
9. **Pipedream cost model** for proxy calls at migration volume (tens of
   thousands of requests per large workspace) — needs confirmation with
   Pipedream account management before enabling "all history" in production.
10. **Export-ZIP fallback:** worth building in this effort or only if the slow
    path turns out to be common? *Assumption:* Phase 4, decided after Phase 0
    measurements.
11. **Search indexing:** does the message search indexer have (or should we
    add) a bulk re-index entry point so imported history becomes searchable
    without per-message events?

---

## 11. Appendix

### 11.1 Slack Web API methods used

| Method | Tier (Marketplace/internal) | Unlisted-app limit | Scopes (user token) | Use |
| --- | --- | --- | --- | --- |
| `auth.test` | Special (unlimited-ish) | — | any | Preflight: team, user, granted scopes (`x-oauth-scopes` header) |
| `conversations.list` | 2 (20+/min, ≤1000/page) | same | `channels:read`, `groups:read` | Gather channels |
| `conversations.members` | 4 (100+/min) | same | `channels:read`, `groups:read` | Participants |
| `conversations.history` | 3 (50+/min, ≤999/page) | **1/min, ≤15** | `channels:history`, `groups:history` | Messages |
| `conversations.replies` | 3 | **1/min, ≤15** | `channels:history`, `groups:history` | Thread replies |
| `conversations.join` | 3 | same | `channels:join` / `channels:write` | Optional auto-join of public channels |
| `users.list` | 2 (≤1000/page) | same | `users:read` (+ `users:read.email`) | People, identity map |
| `users.info` | 4 | same | `users:read` | Slack Connect / missing users |
| `files.info` + `url_private_download` | 4 | same | `files:read` | Attachments |

### 11.2 Affected code map (for estimation)

| Area | Files | Nature of change |
| --- | --- | --- |
| Pipedream crate | `crates/pipedream_mcp/src/domain/ports.rs`, `outbound/api.rs`, `inbound/axum_router.rs`, `outbound/pg_connection_repo.rs` | New `ConnectProxy` port + impl; token response option; health columns. |
| MCP selection | `crates/mcp_select/src/lib.rs` | Multi-slug `ConnectorRef`. |
| Import crate | `crates/import/src/domain/{models.rs,ports.rs,service.rs}`, new `slack_convert.rs`, `slack_identity.rs`, `history_job.rs`; `outbound/{pg_import_repo.rs,pipedream_slack.rs}`; `inbound/{axum_router.rs,toolset.rs}` | Largest change: new source, ports, jobs, runner, converter, routes. |
| Channels crate | `crates/channels/src/domain/{models.rs,ports.rs,service.rs}`, `outbound/pg_channels_repo.rs` | `import_history` use case + bulk repo inserts; quiet side effects. |
| Messages crate | `crates/messages/src/domain/models.rs` | Shared `ImportedMessage`/reaction row types if reused by document discussions. |
| AI tools | `crates/ai_tools/src/tool_context.rs`, `lib.rs` | `ChannelHistoryWriter`, `PeopleImporter` implementations; tool registration. |
| Teams / CRM | `crates/crm/src/domain/service.rs`, `outbound/*`, migration M5 | Contact provenance `source_kind`. |
| Bots | `crates/bots` | System bot "Slack import" bootstrap. |
| Composition | `services/document_cognition_service/src/{main.rs,config.rs}` | Wire proxy, source, runner, tunables. |
| Migrations | `crates/macro_db_client/migrations/*` (M1–M6), `.sqlx/` | Additive schema. |
| Frontend | `apps/web/src/features/import-slack/**` (new), `settings/connections/*`, `setup/*`, `lib/core/pipedream/*`, `lib/queries/import.ts`, service clients, `mcpServers.ts` | Migration view, flag, slug helper, progress UI. |
| SDK & docs | `packages/sdk/**`, `apps/docs/AI/mcp/tools/*`, `docs/AGENT_GUIDE/surfaces.md` | Regenerate, document. |

### 11.3 Sequence: accepting a channel with history

```text
UI ──POST /import/run {import_ids:[row], options:{history:{mode:'since',since}}}──▶ ImportService.run_import
  ├─ repo.mark_importing(row)                                                   (CAS staged→importing)
  ├─ SlackSource.channel_members(C…) + SlackIdentityMap.resolve(…)              (participants)
  ├─ EntityCreator.create_channel(name, team_id, matched users)                 (channels domain)
  ├─ repo.mark_imported(row, channel_id)                                        (channel exists; never re-created)
  ├─ repo.create_job(row, kind=slack_history, options)                          (queued)
  └─ HistoryJobRunner.tick(user)
       ├─ claim job → phase history: SlackSource.history(latest=cursor) → convert → ChannelService.import_history(batch) → save cursor → heartbeat
       ├─ phase threads: for parent in queue: SlackSource.replies → import_history → save cursor
       ├─ phase files (opt): files.info/download → upload → attach
       └─ finish_job → notify(user) + channel_updated nudge + bulk re-index
```
