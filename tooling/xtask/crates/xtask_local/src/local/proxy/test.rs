use super::*;
use crate::local::{inventory, repo_root};

#[test]
fn vite_routes_cover_every_backend_prefix_and_no_frontend_routes() {
    let prefixes = frontend_path_prefixes();
    for svc in inventory::RUST_SERVICES {
        if let Some(prefix) = svc.path_prefix {
            assert!(prefixes.contains(&prefix));
        }
    }
    for prefix in [
        "/websocket",
        "/sync",
        "/i",
        "/lexical",
        "/ai-editing",
        "/static-file",
        "/local-storage",
    ] {
        assert!(prefixes.contains(&prefix));
    }
    assert!(!prefixes.contains(&"/app"));
    assert!(!prefixes.contains(&"/"));
    let unique: std::collections::HashSet<_> = prefixes.iter().collect();
    assert_eq!(unique.len(), prefixes.len());
}

/// Every inventoried service that declares a path prefix must get a route in the
/// generated Caddyfile, targeting its canonical compose service name. This is
/// the guarantee that replaces the old hand-maintained route list.
#[test]
fn generates_a_route_for_every_prefixed_service() {
    let caddy = caddyfile(Mode::Local, false);
    for svc in inventory::RUST_SERVICES {
        let Some(prefix) = svc.path_prefix else {
            continue;
        };
        assert!(
            caddy.contains(&format!("reverse_proxy {}:8080", svc.compose_name)),
            "Caddyfile is missing a route to {}",
            svc.compose_name
        );
        assert!(
            caddy.contains(&format!("{prefix}/*")),
            "Caddyfile is missing the {prefix} prefix"
        );
    }
}

/// Dev keeps a single origin: services that run in Dev hit local containers;
/// Local-only prefixed services fan out to the shared-dev gateway (no strip).
#[test]
fn dev_fans_local_only_prefixes_to_the_gateway() {
    let caddy = caddyfile(Mode::Dev, false);
    for svc in inventory::RUST_SERVICES {
        let Some(prefix) = svc.path_prefix else {
            continue;
        };
        if svc.in_mode(Mode::Dev) {
            assert!(
                caddy.contains(&format!("reverse_proxy {}:8080", svc.compose_name)),
                "Dev Caddyfile missing local route for {}",
                svc.compose_name
            );
            continue;
        }
        if !svc.in_mode(Mode::Local) {
            continue;
        }
        assert!(
            !caddy.contains(&format!("reverse_proxy {}:8080", svc.compose_name)),
            "Dev must not route {} to an absent local container",
            svc.compose_name
        );
        assert!(
            caddy.contains(&format!("handle {prefix}/* {{"))
                || caddy.contains(&format!("path {prefix} {prefix}/*")),
            "Dev Caddyfile missing gateway handle for {prefix}"
        );
        assert!(
            caddy.contains(&format!("reverse_proxy {DEV_GATEWAY_ORIGIN}")),
            "Dev Caddyfile missing gateway upstream for {}",
            svc.compose_name
        );
    }
    // Concrete regression for the scheduled-action / agent-harness pair.
    assert!(caddy.contains("handle /scheduled-action/* {"));
    assert!(caddy.contains("handle /agent-harness/* {"));
    assert!(!caddy.contains("reverse_proxy scheduled_action_service:8080"));
    assert!(!caddy.contains("reverse_proxy agent_harness_service:8080"));
}

/// WebSocket services use the bare-prefix `@matcher` + explicit strip; HTTP
/// services use `handle_path`.
#[test]
fn websocket_services_use_a_matcher() {
    let caddy = caddyfile(Mode::Local, false);
    // connection-gateway is the inventoried WebSocket service.
    assert!(caddy.contains("@connection_gateway path /connection-gateway /connection-gateway/*"));
    assert!(caddy.contains("uri strip_prefix /connection-gateway"));
    // A plain HTTP service uses handle_path.
    assert!(caddy.contains("handle_path /auth/* {"));
}

/// The analytics-proxy worker is reached through the single origin at `/i/*`
/// (PostHog and OTLP traces/logs), un-stripped, on its own :8098 port.
#[test]
fn analytics_proxy_route_is_present() {
    let caddy = caddyfile(Mode::Local, false);
    assert!(caddy.contains("handle /i/* {"));
    assert!(caddy.contains("reverse_proxy analytics-proxy:8098"));
}

#[test]
fn document_content_services_are_available_through_the_proxy() {
    let caddy = caddyfile(Mode::Local, false);

    assert!(caddy.contains("uri strip_prefix /sync"));
    assert!(caddy.contains("reverse_proxy sync-service:8787"));
    assert!(caddy.contains("header_up Origin http://localhost:3000"));
    assert!(!caddyfile(Mode::Dev, false).contains("header_up Origin"));
    assert!(caddy.contains("handle_path /lexical/*"));
    assert!(caddy.contains("reverse_proxy lexical-service:8096"));
    assert!(caddy.contains("handle_path /ai-editing/*"));
    assert!(caddy.contains("reverse_proxy ai-editing-worker:8933"));
}

/// The static-file block is the one route that differs by mode: LocalStack S3
/// fan-out locally, the dev-pointed service in dev.
#[test]
fn static_file_block_is_mode_specific() {
    let local = caddyfile(Mode::Local, false);
    assert!(local.contains("/static-file-storage"));
    // A route at site scope sorts after the Vite catch-all handle and loops.
    assert!(local.contains("handle_path /static-file/*"));
    assert!(!local.contains("route /static-file/*"));
    assert!(caddyfile(Mode::Dev, false).contains("handle_path /static-file/*"));
    assert!(!caddyfile(Mode::Dev, false).contains("/static-file-storage"));
    assert!(local.contains("handle_path /local-storage/*"));
    assert!(!caddyfile(Mode::Dev, false).contains("handle_path /local-storage/*"));
}

/// Drift gate across the Rust↔TypeScript seam: every proxied service's prefix
/// must be wired into `proxyServers()` in `servers.ts`, or the frontend can't
/// reach it through the single-origin proxy. servers.ts can't be derived from
/// Rust, so this test is what keeps the two in sync.
#[test]
fn frontend_wires_every_inventory_prefix() {
    let servers = repo_root().join("apps/web/src/lib/core/constant/servers.ts");
    let src = std::fs::read_to_string(&servers)
        .unwrap_or_else(|e| panic!("reading {}: {e}", servers.display()));
    for svc in inventory::RUST_SERVICES {
        let Some(prefix) = svc.path_prefix else {
            continue;
        };
        let http = format!("${{proxyOrigin}}{prefix}");
        let ws = format!("${{wsProxyOrigin}}{prefix}");
        assert!(
            src.contains(&http) || src.contains(&ws),
            "servers.ts proxyServers() is missing prefix {prefix} (for {}); \
             the frontend can't reach it through the proxy",
            svc.compose_name
        );
    }
}

/// The static-frontend block only appears in headless mode, and serves the
/// mounted bundle under `/app` with an SPA fallback. Attached `run_local` keeps
/// forwards frontend requests to Vite instead of serving a bundle.
#[test]
fn static_frontend_block_is_opt_in() {
    let headless = caddyfile(Mode::Local, true);
    assert!(headless.contains("handle_path /app/* {"));
    assert!(headless.contains("root * /srv/frontend"));
    assert!(headless.contains("try_files {path} /index.html"));
    assert!(headless.contains("redir / \"/app/?{query}\" 302"));
    assert!(headless.contains("handle /mailpit/*"));

    let attached = caddyfile(Mode::Local, false);
    assert!(!attached.contains("/srv/frontend"));
    assert!(attached.contains("reverse_proxy host.docker.internal:{$VITE_PORT}"));
    assert!(attached.contains(&format!(
        "@backend_root path {}",
        frontend_path_prefixes().join(" ")
    )));
    assert!(attached.contains("respond @backend_root 404"));
    assert!(!headless.contains("host.docker.internal"));
    assert!(!attached.contains("redir / /app/ 302"));
    assert!(!attached.contains("handle /mailpit/*"));

    let headless_dev = caddyfile(Mode::Dev, true);
    assert!(!headless_dev.contains("handle /mailpit/*"));
}

/// Only host-run Vite needs protection from concurrent module connection bursts.
#[test]
fn vite_connection_limit_is_scoped_to_frontend() {
    for mode in [Mode::Local, Mode::Dev] {
        let attached = caddyfile(mode, false);
        assert_eq!(attached.matches("max_conns_per_host").count(), 1);
        let (backend_routes, frontend_route) = attached
            .split_once("reverse_proxy host.docker.internal:{$VITE_PORT} {")
            .expect("attached frontend must proxy to Vite");
        assert!(!backend_routes.contains("max_conns_per_host"));
        assert!(frontend_route.contains("transport http {\n                max_conns_per_host 16"));
        assert!(!caddyfile(mode, true).contains("max_conns_per_host"));
    }
}

/// Local Caddy speaks HTTPS with a machine certificate and stamps wildcard CORS
/// on every response. Dev still uses TLS (same proxy) but does not overlay
/// CORS, because it fans out to the shared-dev gateway.
#[test]
fn local_proxy_uses_tls_and_wildcard_cors() {
    let local = caddyfile(Mode::Local, false);
    assert!(local.contains("tls /etc/caddy/certs/server.pem /etc/caddy/certs/server-key.pem"));
    // Keep main's internal certificates for the separate preview listener.
    assert!(local.contains("auto_https disable_redirects"));
    assert!(local.contains("https://*.preview.localhost:8443"));
    assert!(local.contains("@cors header Origin *"));
    assert!(local.contains("@cors_preflight"));
    assert!(local.contains("Access-Control-Allow-Origin \"{http.request.header.Origin}\""));
    assert!(
        local.contains("defer"),
        "CORS overlay must defer so reverse_proxy cannot overwrite the reflected Origin"
    );
    assert!(
        !local.contains("-Access-Control-Allow-Origin"),
        "deferred -Access-Control-* deletes strip the CORS headers this overlay sets"
    );

    let dev = caddyfile(Mode::Dev, false);
    assert!(dev.contains("tls /etc/caddy/certs/server.pem /etc/caddy/certs/server-key.pem"));
    assert!(!dev.contains("@cors header Origin *"));
    assert!(!dev.contains("@cors_preflight"));
}

#[test]
fn proxy_origin_is_https() {
    let instance = crate::local::instance::Instance::derive(None, None).unwrap();
    assert_eq!(url(&instance), "https://localhost:8090");
    assert_eq!(ws_url(&instance), "wss://localhost:8090");
    assert!(
        ca_pem().is_file(),
        "checked-in CA is missing at {}",
        ca_pem().display()
    );
}

#[test]
fn public_exposure_drops_mailpit_and_wildcard_cors() {
    let private = render(Mode::Local, true, false);
    assert!(private.contains("handle /mailpit/*"));
    assert!(private.contains("@cors header Origin *"));

    let public = render(Mode::Local, true, true);
    assert!(!public.contains("mailpit"), "{public}");
    assert!(!public.contains("Access-Control-Allow-Origin"), "{public}");
    assert!(public.contains("@foreign_origin"));
    // The guard must precede every route: Caddy runs handle blocks in order.
    let guard = public.find("handle @foreign_origin").unwrap();
    let first_route = public.find("handle_path").unwrap();
    assert!(guard < first_route, "{public}");
}

#[test]
fn public_exposure_gates_localstack_storage_behind_a_session() {
    let public = render(Mode::Local, true, true);
    for block in ["handle_path /local-storage/*", "handle_path /static-file/*"] {
        let start = public.find(block).unwrap();
        let rest = &public[start..];
        let auth = rest
            .find("forward_auth authentication-service:8080")
            .unwrap();
        let s3 = rest.find("reverse_proxy localstack:4566").unwrap();
        assert!(
            auth < s3,
            "{block} must authenticate before reaching S3: {rest}"
        );
    }
    assert!(!render(Mode::Local, true, false).contains("forward_auth"));
}

/// The `handle_path` block for `prefix`, up to its closing brace.
fn route_block<'a>(caddy: &'a str, prefix: &str) -> &'a str {
    let start = caddy
        .find(&format!("handle_path {prefix}/* {{"))
        .unwrap_or_else(|| panic!("no route for {prefix}: {caddy}"));
    let rest = &caddy[start..];
    &rest[..rest.find("\n    }\n").unwrap()]
}

#[test]
fn public_exposure_gates_every_http_service_but_auth_behind_a_session() {
    let public = render(Mode::Local, true, true);
    let http_prefixes = inventory::RUST_SERVICES
        .iter()
        .filter(|svc| !svc.is_websocket && svc.in_mode(Mode::Local))
        .filter_map(|svc| svc.path_prefix)
        .chain(["/lexical", "/ai-editing"])
        .filter(|prefix| !PUBLIC_UNROUTED_PREFIXES.contains(prefix));
    for prefix in http_prefixes {
        let block = route_block(&public, prefix);
        if prefix == "/auth" {
            assert!(!block.contains("forward_auth"), "{block}");
            continue;
        }
        let m = format!("{}_session", matcher_name(prefix));
        assert!(block.contains(&format!("{m} not path /health")), "{block}");
        let auth = block
            .find(&format!("forward_auth {m} authentication-service:8080"))
            .unwrap_or_else(|| panic!("{prefix} is not gated: {block}"));
        let upstream = block.find("reverse_proxy").unwrap();
        assert!(auth < upstream, "{prefix} must authenticate first: {block}");
    }

    let private = render(Mode::Local, true, false);
    assert!(!private.contains("forward_auth"), "{private}");
    assert!(private.contains(
        "handle_path /dss/* {
        reverse_proxy document_storage_service:8080"
    ));
    assert!(private.contains(
        "handle_path /lexical/* {
        reverse_proxy lexical-service:8096"
    ));
}

#[test]
fn public_exposure_hides_api_docs() {
    let public = render(Mode::Local, true, true);
    let docs = public.find("handle @api_docs").unwrap();
    let first_route = public.find("handle_path").unwrap();
    assert!(docs < first_route, "{public}");
    assert!(public.contains("@api_docs path */api-doc */api-doc/* */swagger-ui */swagger-ui/*"));
    assert!(!render(Mode::Local, true, false).contains("@api_docs"));
}

#[test]
fn public_exposure_sets_browser_hardening_headers() {
    let public = render(Mode::Local, true, true);
    let headers = public.find("    header {\n        defer\n").unwrap();
    let first_route = public.find("handle_path").unwrap();
    assert!(headers < first_route, "{public}");
    for h in [
        "Strict-Transport-Security \"max-age=31536000\"",
        "X-Content-Type-Options \"nosniff\"",
        "X-Frame-Options \"SAMEORIGIN\"",
    ] {
        assert!(public.contains(h), "{h}");
    }
    assert!(!render(Mode::Local, true, false).contains("Strict-Transport-Security"));
}

#[test]
fn public_exposure_drops_unrouted_prefixes_and_answers_404_for_the_rest() {
    let public = render(Mode::Local, true, true);
    for prefix in PUBLIC_UNROUTED_PREFIXES {
        assert!(
            !public.contains(&format!("{prefix}/*")),
            "{prefix}: {public}"
        );
    }
    assert!(!public.contains("preview_gateway:8080"), "{public}");
    assert!(!public.contains("lexical-service"), "{public}");
    assert!(
        public.contains("    handle {\n        respond 404\n    }\n"),
        "{public}"
    );

    let private = render(Mode::Local, true, false);
    assert!(private.contains("handle_path /preview/*"), "{private}");
    assert!(private.contains("handle_path /lexical/*"), "{private}");
    assert!(
        !private.contains("    handle {\n        respond 404"),
        "{private}"
    );
}

#[test]
fn public_exposure_gates_websocket_and_sync_behind_a_session() {
    let public = render(Mode::Local, true, true);
    for (handle, upstream) in [
        (
            "handle @websocket {",
            "reverse_proxy websocket-service:6969",
        ),
        ("handle @sync {", "reverse_proxy sync-service:8787"),
    ] {
        let rest = &public[public.find(handle).unwrap()..];
        let block = &rest[..rest.find("\n    }\n").unwrap()];
        let auth = block
            .find("forward_auth authentication-service:8080")
            .unwrap_or_else(|| panic!("{handle} is not gated: {block}"));
        assert!(auth < block.find(upstream).unwrap(), "{block}");
    }
    let private = render(Mode::Local, true, false);
    assert!(private.contains("handle @websocket {\n        uri strip_prefix /websocket"));
    assert!(private.contains("handle @sync {\n        uri strip_prefix /sync"));
}

/// Public storage forwards only S3 object requests: signed in, then the CEL
/// allowlist, then LocalStack with the host the services sign for.
#[test]
fn public_storage_admits_only_s3_object_requests() {
    let public = render(Mode::Local, true, true);
    let storage = &public[public.find("handle_path /local-storage/* {").unwrap()..];
    let storage = &storage[..storage.find("handle_path /static-file/*").unwrap()];
    let auth = storage
        .find("forward_auth authentication-service:8080")
        .unwrap();
    let refuse = storage.find("respond @storage_refused").unwrap();
    let upstream = storage.find("reverse_proxy localstack:4566").unwrap();
    assert!(auth < refuse && refuse < upstream, "{storage}");
    assert!(
        storage.contains("header_up Host localstack:4566"),
        "{storage}"
    );
    for bucket in resources::BUCKETS {
        assert!(
            storage.contains(bucket.name),
            "{} missing: {storage}",
            bucket.name
        );
    }
    for needle in [
        "{http.request.method} in ['GET', 'HEAD', 'PUT']",
        "{http.request.header.X-Amz-Target} == ''",
        "X-Amz-Signature=",
        "startsWith('/doc-storage/')",
        "s3(%2F|/)aws4_request",
        "(^|/)[.][.]?(/|$)",
    ] {
        assert!(storage.contains(needle), "{needle} missing: {storage}");
    }

    let static_file = &public[public.find("handle_path /static-file/* {").unwrap()..];
    let refuse = static_file.find("respond @static_refused").unwrap();
    assert!(refuse < static_file.find("rewrite * /static-file-storage").unwrap());
    assert!(static_file.contains("{http.request.method} in ['GET', 'HEAD']"));

    let private = render(Mode::Local, true, false);
    assert!(!private.contains("expression"), "{private}");
    assert!(!private.contains("header_up Host localstack"), "{private}");
}
