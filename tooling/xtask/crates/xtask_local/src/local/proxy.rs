//! The per-instance single-origin reverse proxy (Caddy).
//!
//! One backend origin the frontend points at via `VITE_LOCAL_BACKEND_ORIGIN`.
//! Path prefixes mirror the `serverHostLocal` keys in `servers.ts`; Caddy
//! `reverse_proxy` upgrades WebSockets transparently (connection-gateway,
//! websocket-service, sync-service). The `/static-file/*` block reproduces the
//! nginx CDN fan-out (S3 via LocalStack + the static-file service).
//!
//! The host-facing listener is HTTPS using a machine certificate signed by
//! the development CA in `infra/local/certs`. Local mode also stamps wildcard CORS on the generated
//! Caddyfile so any browser origin (`https://`, `*.localhost`, tunnels) can
//! call the proxy without updating every service allowlist.

use std::path::PathBuf;

use anyhow::{Context, Result};

use super::gen_compose::{caddyfile_path, tls_certs_dir};
use super::instance::{Instance, Port};
use super::{Mode, inventory, resources};

/// The host-facing proxy origin. The local reverse proxy speaks HTTPS using
/// a machine certificate signed by the checked-in development CA.
pub fn url(instance: &Instance) -> String {
    format!("https://localhost:{}", instance.port(Port::Proxy))
}

/// WebSocket origin for the same proxy.
pub fn ws_url(instance: &Instance) -> String {
    format!("wss://localhost:{}", instance.port(Port::Proxy))
}

/// Path to the checked-in local CA. Pass to `curl --cacert` when probing the
/// proxy so health checks verify TLS instead of skipping it.
pub fn ca_pem() -> PathBuf {
    tls_certs_dir().join("ca.pem")
}

/// Extra curl arguments that trust the local proxy CA. Empty for `http://`.
pub fn curl_ca_args(url: &str) -> Vec<String> {
    if url.starts_with("https://") {
        vec!["--cacert".into(), ca_pem().display().to_string()]
    } else {
        Vec::new()
    }
}

/// Vite forwards only backend routes, leaving assets, SPA navigation and HMR
/// with the frontend. Keep inventoried service paths sourced from the same
/// inventory as Caddy; the remaining paths are this module's special routes.
pub fn frontend_path_prefixes() -> Vec<&'static str> {
    inventory::RUST_SERVICES
        .iter()
        .filter_map(|svc| svc.path_prefix)
        .chain([
            "/websocket",
            "/sync",
            "/i",
            "/lexical",
            "/ai-editing",
            "/static-file",
            "/local-storage",
        ])
        .collect()
}

/// Write the instance Caddyfile and return its path. Both local and dev keep a
/// single frontend origin; Local fans every inventory prefix to a local
/// container, while Dev fans Local-only prefixes (services that must not run
/// against shared-dev) to the deployed gateway. The static-file block also
/// differs (dev has no local LocalStack). With `static_frontend` the proxy also
/// serves the built app bundle at `/app`, making it the one origin for the
/// whole product.
pub fn write_caddyfile(instance: &Instance, mode: Mode, static_frontend: bool) -> Result<PathBuf> {
    let path = caddyfile_path(instance);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("creating proxy dir {}", dir.display()))?;
    }
    let public = mode.spec().runs_local_infra && super::exposure::enabled(instance)?;
    std::fs::write(&path, render(mode, static_frontend, public))
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

/// [`render`] for a private (not publicly exposed) stack.
#[cfg(test)]
fn caddyfile(mode: Mode, static_frontend: bool) -> String {
    render(mode, static_frontend, false)
}

/// Assemble the Caddyfile: the listener head (HTTPS + optional local CORS),
/// the generated per-service routes (from the inventory), the special
/// non-inventory routes, the mode's static-file block, the optional
/// static-frontend block, then the tail. `public` applies the
/// [`super::exposure`] hardening: no wildcard CORS, a same-origin guard, no
/// Mailpit route or API docs, and LocalStack storage and every HTTP service
/// except authentication only for signed-in sessions.
fn render(mode: Mode, static_frontend: bool, public: bool) -> String {
    let static_block = if !mode.spec().static_files_via_localstack {
        STATIC_FILE_DEV.to_owned()
    } else if public {
        static_file_local_public()
    } else {
        STATIC_FILE_LOCAL.to_owned()
    };
    let mailpit_block = if static_frontend && mode.spec().runs_local_infra && !public {
        MAILPIT_ROUTE
    } else {
        ""
    };
    let frontend_block = if static_frontend {
        FRONTEND_STATIC.to_owned()
    } else {
        FRONTEND_VITE.replace("BACKEND_PREFIXES", &frontend_path_prefixes().join(" "))
    };
    let preview_block = if mode.spec().runs_local_infra {
        "\nhttps://*.preview.localhost:8443 {\n    tls internal\n    reverse_proxy preview_gateway:8111\n}\n"
    } else {
        ""
    };
    // Wildcard CORS is a local-stack overlay: `run_local` / `stack up` stamp
    // it on Caddy so HTTPS and `*.localhost` origins work without touching
    // every service allowlist. `run_dev` still fans out to the shared-dev
    // gateway, so it keeps service CORS as-is.
    let cors_block = if public {
        PUBLIC_ORIGIN_GUARD
    } else if mode == Mode::Local {
        LOCAL_CORS
    } else {
        ""
    };
    // Sync actively rejects unknown origins, including HTTPS machine names,
    // before upgrading a socket. Local CORS is owned by this proxy; normalize
    // only the local worker's upstream origin to its existing dev allowlist.
    let sync_origin = if mode == Mode::Local {
        "            header_up Origin http://localhost:3000\n"
    } else {
        ""
    };
    // The upstream WebSocket echo service and the sync worker's HTTP endpoints
    // (document existence, wake-up, peer → user lookups) answer without a
    // session; on a public stack only signed-in pages reach them. Browsers send
    // the session cookie on same-origin WebSocket upgrades.
    let session_gate = if public {
        SPECIAL_ROUTE_SESSION_GATE
    } else {
        ""
    };
    let special_routes = SPECIAL_ROUTES
        .replace("SYNC_ORIGIN_HEADER", sync_origin)
        .replace("SESSION_GATE", session_gate)
        + &worker_routes(public);
    // Anything no route claims gets an explicit 404 on a public stack instead
    // of Caddy's empty 200.
    let fallback_block = if public && static_frontend {
        PUBLIC_FALLBACK
    } else {
        ""
    };
    format!(
        "{CADDY_HEAD}{cors_block}{routes}{special_routes}{mailpit_block}{static_block}{frontend_block}{fallback_block}{CADDY_TAIL}{preview_block}",
        routes = service_routes(mode, public)
    )
}

/// Shared-dev gateway origin for Local-only inventory prefixes under `run-dev`.
/// Keep the path (no strip) — gateway tenants are mounted under this prefix.
const DEV_GATEWAY_ORIGIN: &str = "https://dev-gateway.macro.com";

/// Generate the reverse-proxy routes for every inventoried service that exposes
/// a path prefix. The inventory is the single source, so adding a service's
/// proxy route is one field there — not a hand-edit here that can drift.
fn service_routes(mode: Mode, public: bool) -> String {
    let mut out = String::new();
    for svc in inventory::RUST_SERVICES {
        let Some(prefix) = svc.path_prefix else {
            continue;
        };
        if public && PUBLIC_UNROUTED_PREFIXES.contains(&prefix) {
            continue;
        }
        if svc.in_mode(mode) {
            let gated = public && !SESSION_EXEMPT_PREFIXES.contains(&prefix);
            out.push_str(&local_route_block(
                prefix,
                svc.compose_name,
                svc.is_websocket,
                gated,
            ));
        } else if mode == Mode::Dev && svc.in_mode(Mode::Local) {
            // Local-only: do not start the binary against shared-dev, but keep
            // the single-origin proxy by fanning out to the deployed gateway.
            out.push_str(&dev_gateway_route_block(prefix, svc.is_websocket));
        }
    }
    out
}

/// Inventory prefixes a publicly exposed stack serves without a session. Every
/// other HTTP service sits behind [`session_gated_block`]: several answer
/// anonymous requests by design (link-shared document metadata and edits,
/// entity previews and permission probes, harness pairing, the unfurl URL
/// fetcher), which is fine inside a private network but not on the internet.
/// The authentication service is the gate itself (login, code exchange, token
/// refresh), and the WebSocket services check the session on upgrade.
const SESSION_EXEMPT_PREFIXES: &[&str] = &["/auth"];

/// Prefixes a publicly exposed stack does not route at all: agent sandbox
/// previews and the Lexical worker, which the web app does not call. Their
/// paths fall through to the public catch-all 404.
const PUBLIC_UNROUTED_PREFIXES: &[&str] = &["/preview", "/lexical"];

/// A `handle_path` route that only forwards requests whose session the
/// authentication service accepts (`/permissions/me`; it reads the same cookie
/// or bearer token the services do). The app calls these routes from signed-in
/// pages, and same-origin fetches and `<img>` loads carry the session cookie.
/// `/health` stays open for uptime probes.
fn session_gated_block(prefix: &str, upstream: &str) -> String {
    let m = format!("{}_session", matcher_name(prefix));
    format!(
        "    handle_path {prefix}/* {{
        {m} not path /health
        forward_auth {m} authentication-service:8080 {{
            uri /permissions/me
        }}
        reverse_proxy {upstream}
    }}
"
    )
}

/// One Caddy route to a local service container (always on `:8080`). HTTP uses
/// `handle_path` (which strips the prefix); WebSocket needs the bare-prefix
/// `@matcher` + explicit strip so the frontend's trailing-slash-less connect URL
/// still matches. The target is the canonical compose service name, which always
/// resolves on the proxy's networks. `gated` puts a session check (see
/// [`session_gated_block`]) in front of an HTTP route.
fn local_route_block(prefix: &str, target: &str, is_websocket: bool, gated: bool) -> String {
    if gated && !is_websocket {
        return session_gated_block(prefix, &format!("{target}:8080"));
    }
    if is_websocket {
        let m = matcher_name(prefix);
        format!(
            "    {m} path {prefix} {prefix}/*\n    handle {m} {{\n        uri strip_prefix {prefix}\n        reverse_proxy {target}:8080\n    }}\n"
        )
    } else {
        format!("    handle_path {prefix}/* {{\n        reverse_proxy {target}:8080\n    }}\n")
    }
}

/// Dev route to the shared gateway: keep the path prefix (gateway mounts are
/// prefixed) and set `Host` so TLS/SNI + ALB host routing work.
fn dev_gateway_route_block(prefix: &str, is_websocket: bool) -> String {
    let host = DEV_GATEWAY_ORIGIN
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    if is_websocket {
        let m = matcher_name(prefix);
        format!(
            "    {m} path {prefix} {prefix}/*\n    handle {m} {{\n        reverse_proxy {DEV_GATEWAY_ORIGIN} {{\n            header_up Host {host}\n        }}\n    }}\n"
        )
    } else {
        format!(
            "    handle {prefix}/* {{\n        reverse_proxy {DEV_GATEWAY_ORIGIN} {{\n            header_up Host {host}\n        }}\n    }}\n"
        )
    }
}

/// A Caddy named-matcher token from a path prefix (drop the slash; hyphens →
/// underscores so it's a single bare token).
fn matcher_name(prefix: &str) -> String {
    format!("@{}", prefix.trim_start_matches('/').replace('-', "_"))
}

/// Caddy listens on `{$PROXY_PORT}` (set in the compose service env) with the
/// generated machine certificate. Automatic HTTP→HTTPS redirects stay off —
/// this port is HTTPS-only. The per-service routes (generated from the
/// inventory), the special routes, the static-file block, and the closing
/// brace follow.
const CADDY_HEAD: &str = r#"# GENERATED by `cargo x` — do not edit.
{
    auto_https disable_redirects
    admin off
}

:{$PROXY_PORT} {
    tls /etc/caddy/certs/server.pem /etc/caddy/certs/server-key.pem
    # Caddy requires the block body on its own lines (no single-line `{ ... }`).
"#;

/// Reflect any `Origin` on the local proxy. Preflight is answered here so
/// OPTIONS never depends on a service CORS layer.
///
/// Do not `header { -Access-Control-* }` at site scope: Caddy's `-Field`
/// is a deferred delete of the final response, so it also strips the
/// headers this overlay sets. `defer` applies our replacements after
/// `reverse_proxy` copies upstream CORS, overwriting them.
/// Leave exposed headers to the upstream service: `*` is not a wildcard
/// for credentialed requests and would hide headers such as `Retry-After`.
const LOCAL_CORS: &str = r#"    @cors header Origin *
    header @cors {
        Access-Control-Allow-Origin "{http.request.header.Origin}"
        Access-Control-Allow-Credentials true
        Access-Control-Max-Age 86400
        Vary Origin
        defer
    }
    @cors_preflight {
        method OPTIONS
        header Origin *
    }
    handle @cors_preflight {
        header Access-Control-Allow-Origin "{http.request.header.Origin}"
        header Access-Control-Allow-Credentials true
        header Access-Control-Allow-Methods "GET, HEAD, POST, PUT, PATCH, DELETE, OPTIONS"
        header Access-Control-Allow-Headers "{http.request.header.Access-Control-Request-Headers}"
        header Access-Control-Max-Age 86400
        header Vary Origin
        respond 204
    }
"#;

/// HTTP workers that aren't in the Rust inventory (own ports, not `:8080`).
/// They render and transform document content for signed-in pages, so a
/// public stack gates them like the inventory services.
const WORKER_ROUTES: &[(&str, &str)] = &[
    ("/lexical", "lexical-service:8096"),
    ("/ai-editing", "ai-editing-worker:8933"),
];

fn worker_routes(public: bool) -> String {
    WORKER_ROUTES
        .iter()
        .filter(|(prefix, _)| !(public && PUBLIC_UNROUTED_PREFIXES.contains(prefix)))
        .map(|(prefix, upstream)| {
            if public {
                session_gated_block(prefix, upstream)
            } else {
                format!("    handle_path {prefix}/* {{\n        reverse_proxy {upstream}\n    }}\n")
            }
        })
        .collect()
}

/// Routes for services that aren't in the Rust inventory (external / base-compose
/// services on their own ports), so they can't be generated from it. The
/// WebSocket routes match the bare prefix too (the frontend connects without a
/// trailing slash, which `handle_path /x/*` would miss).
const SPECIAL_ROUTES: &str = r#"    @websocket path /websocket /websocket/*
    handle @websocket {
SESSION_GATE        uri strip_prefix /websocket
        reverse_proxy websocket-service:6969
    }
    @sync path /sync /sync/*
    handle @sync {
SESSION_GATE        uri strip_prefix /sync
        reverse_proxy sync-service:8787 {
SYNC_ORIGIN_HEADER        }
    }
    # Analytics/telemetry proxy worker (PostHog and OTLP traces/logs).
    # No prefix strip: the worker itself routes on the /i/{ph,dd,otlp} prefix,
    # and it listens on 8098, not the :8080 the generated routes assume.
    # Set CF-Connecting-IP (absent without Cloudflare's edge in front) so the
    # worker's rate-limit keying has a client IP instead of erroring.
    handle /i/* {
        reverse_proxy analytics-proxy:8098 {
            header_up CF-Connecting-IP {http.request.remote.host}
        }
    }
"#;

/// The session check [`SPECIAL_ROUTES`] gets on a public stack (indented for
/// the `handle` blocks it lands in).
const SPECIAL_ROUTE_SESSION_GATE: &str = "        forward_auth authentication-service:8080 {
            uri /permissions/me
        }
";

/// Public headless stacks answer unrouted paths with 404. A `handle` without a
/// matcher sorts after every other `handle`, so this only sees leftovers.
const PUBLIC_FALLBACK: &str = r#"    handle {
        respond 404
    }
"#;

const MAILPIT_ROUTE: &str = r#"    # Mailpit serves itself under /mailpit (MP_WEBROOT), so no prefix strip —
    # this is how a headless stack reads its passwordless login codes.
    handle /mailpit/* {
        reverse_proxy mailpit:8025
    }
    redir /mailpit /mailpit/ 308

"#;

/// Local: /api and /internal go to the service, everything else to the S3 bucket
/// via LocalStack (mirrors infra/local/nginx/static-file-cdn.conf).
const STATIC_FILE_LOCAL: &str = r#"    handle_path /local-storage/* {
        reverse_proxy localstack:4566
    }
    handle_path /static-file/* {
        # Keep service dispatch before the S3 rewrite inside this exclusive handle.
        route {
            @svc path /api/* /internal/*
            reverse_proxy @svc static-file-service:8080
            rewrite * /static-file-storage{uri}
            reverse_proxy localstack:4566
        }
    }
"#;

/// Public exposure: the browser and this proxy share one origin, so a request
/// carrying any other `Origin` is a cross-site call riding the user's cookies.
/// Refuse it before any route sees it (handle blocks run in file order).
const PUBLIC_ORIGIN_GUARD: &str = r#"    @foreign_origin expression `{http.request.header.Origin} != "" && {http.request.header.Origin} != "https://" + {http.request.hostport}`
    handle @foreign_origin {
        respond "cross-origin request refused" 403
    }
    # Service API maps (OpenAPI JSON, Swagger UI) stay off the public origin.
    @api_docs path */api-doc */api-doc/* */swagger-ui */swagger-ui/*
    handle @api_docs {
        respond 404
    }
"#;

/// [`STATIC_FILE_LOCAL`] for a publicly exposed stack. LocalStack is the whole
/// AWS API (every S3 bucket, SQS, KMS, DynamoDB) and answers it without
/// checking credentials, so the public origin forwards only signed-in
/// requests, and only the S3 object requests the app itself makes:
///
/// - `/local-storage/<bucket>/<key>`: `GET`/`HEAD`/`PUT` of one object in a
///   catalogued bucket, carrying an S3 presigned-URL signature (the services
///   presign uploads and downloads). The one unsigned form is `GET`/`HEAD` in
///   doc-storage, because local stacks hand out document URLs without the
///   CloudFront signature production uses.
/// - `/static-file/<key>`: `GET`/`HEAD` of one static-file object.
///
/// Everything else (bucket listings and other bucket- or object-level
/// sub-resources, other AWS protocols selected by `X-Amz-Target`, a form or
/// JSON body, a header signature or a non-S3 credential scope, LocalStack's
/// own `/_localstack` API, dot segments) is refused with 403. LocalStack sees
/// `Host: localstack:4566`, the host the services address it by, so it never
/// interprets the public hostname.
///
/// The presigned-signature requirement is a shape check, not authorization:
/// local services sign with the AWS SDK's fixed test credentials, which
/// LocalStack cannot verify (so its signature validation stays off) and which
/// anyone could reproduce. What it buys is that a signed-in user can reach one
/// object by its full key, never list a bucket or reach another AWS API.
fn static_file_local_public() -> String {
    let buckets = resources::BUCKETS
        .iter()
        .map(|bucket| bucket.name)
        .collect::<Vec<_>>()
        .join("|");
    let object =
        format!("{STORAGE_COMMON} && {{http.request.uri.path}}.matches('^/({buckets})/[^/]')");
    let local_storage = format!(
        "{object} && {{http.request.method}} in ['GET', 'HEAD', 'PUT'] && ({PRESIGNED} || ({{http.request.method}} in ['GET', 'HEAD'] && {{http.request.uri.path}}.startsWith('/{doc}/')))",
        doc = resources::DOC_STORAGE_BUCKET,
    );
    let static_file = format!(
        "{STORAGE_COMMON} && {{http.request.uri.path}}.matches('^/[^/]') && {{http.request.method}} in ['GET', 'HEAD']"
    );
    format!(
        r#"    handle_path /local-storage/* {{
        route {{
            forward_auth authentication-service:8080 {{
                uri /permissions/me
            }}
            @storage_refused not expression `{local_storage}`
            respond @storage_refused "storage request refused" 403
            reverse_proxy localstack:4566 {{
                header_up Host localstack:4566
            }}
        }}
    }}
    handle_path /static-file/* {{
        # Keep service dispatch before the S3 rewrite inside this exclusive handle.
        route {{
            @svc path /api/* /internal/*
            reverse_proxy @svc static-file-service:8080
            forward_auth authentication-service:8080 {{
                uri /permissions/me
            }}
            @static_refused not expression `{static_file}`
            respond @static_refused "storage request refused" 403
            rewrite * /static-file-storage{{uri}}
            reverse_proxy localstack:4566 {{
                header_up Host localstack:4566
            }}
        }}
    }}
"#
    )
}

/// CEL conditions every public storage request must meet: no other AWS
/// protocol (`X-Amz-Target`, form or JSON bodies, a header signature, a
/// non-S3 credential scope), no dot segments or backslashes in the key, and
/// only presigned-URL / response-override query parameters.
const STORAGE_COMMON: &str = concat!(
    "{http.request.header.X-Amz-Target} == ''",
    " && !{http.request.header.Authorization}.startsWith('AWS')",
    " && !{http.request.header.Content-Type}.matches('(?i)(x-www-form-urlencoded|amz-json)')",
    " && !{http.request.uri.path}.matches('(^|/)[.][.]?(/|$)')",
    " && !{http.request.uri.path}.contains('\\\\')",
    " && {http.request.uri.query}.matches('^(((?i:x-amz-[a-z0-9-]+)=[^&]*|x-id=(GetObject|PutObject|HeadObject)|response-[a-z-]+=[^&]*)(&|$))*$')",
    " && !{http.request.uri.query}.matches('(?i)(^|&)x-amz-target=')",
    " && (!{http.request.uri.query}.matches('(?i)(^|&)x-amz-credential=') || {http.request.uri.query}.matches('(?i)(^|&)x-amz-credential=[^&]*(%2F|/)s3(%2F|/)aws4_request(&|$)'))",
);

/// CEL: the request carries an S3 presigned-URL signature.
const PRESIGNED: &str = "{http.request.uri.query}.matches('(^|&)X-Amz-Signature=[^&]+')";

/// Dev: no local LocalStack — route all static-file paths through the local
/// static-file-service (which is pointed at dev S3).
const STATIC_FILE_DEV: &str = r#"    handle_path /static-file/* {
        reverse_proxy static-file-service:8080
    }
"#;

/// Headless mode: the proxy serves the built app bundle (mounted at
/// `/srv/frontend` — see `gen_compose::add_proxy_service`). The bundle is built
/// with `base: /app`, so URL space `/app/*` maps onto the dist root after the
/// prefix strip; unknown paths fall back to `index.html` (SPA routing). Caddy
/// sorts `redir` before `handle_path`, so the exact-path redirects win first.
const FRONTEND_STATIC: &str = r#"    redir / "/app/?{query}" 302
    redir /app /app/ 308
    handle_path /app/* {
        root * /srv/frontend
        try_files {path} /index.html
        file_server
    }
"#;

const CADDY_TAIL: &str = r#"
}
"#;

/// Catch only non-backend paths, including Vite assets and HMR upgrades.
/// Forward the original Host so Vite enforces the detected machine allowlist.
const FRONTEND_VITE: &str = r#"    handle {
        # Bare HTTP service prefixes have no handler. Do not send them to
        # Vite, whose backend proxy would send them straight back here.
        @backend_root path BACKEND_PREFIXES
        respond @backend_root 404
        reverse_proxy host.docker.internal:{$VITE_PORT} {
            # Vite loads many modules concurrently. Bound Docker-to-host
            # connections so the burst cannot exhaust the host listener and
            # fail module imports with dial timeouts / 502s.
            transport http {
                max_conns_per_host 16
            }
        }
    }
"#;

#[cfg(test)]
mod test;
