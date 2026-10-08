{ pkgs }:
let
  grafana = (pkgs.formats.ini { }).generate "grafana.ini" {
    "server" = {
      domain = "$__env{GRAFANA_HOST}";
      root_url = "$__env{GRAFANA_ROOT_URL}";
      enforce_domain = true;
    };
    "security" = {
      disable_initial_admin_creation = true;
      secret_key = "$__file{/run/secrets/grafana_secret_key}";
      cookie_secure = true;
      cookie_samesite = "lax";
      strict_transport_security = true;
      content_security_policy = true;
    };
    "users" = {
      allow_sign_up = false;
      allow_org_create = false;
      viewers_can_edit = false;
    };
    "auth" = {
      disable_login_form = true;
      login_maximum_inactive_lifetime_duration = "1h";
      login_maximum_lifetime_duration = "8h";
    };
    "auth.basic" = {
      enabled = false;
    };
    "auth.anonymous" = {
      enabled = false;
    };
    "auth.google" = {
      enabled = true;
      allow_sign_up = true;
      client_id = "$__file{/run/secrets/google_client_id}";
      client_secret = "$__file{/run/secrets/google_client_secret}";
      scopes = "openid email profile";
      auth_url = "https://accounts.google.com/o/oauth2/v2/auth";
      token_url = "https://oauth2.googleapis.com/token";
      api_url = "https://openidconnect.googleapis.com/v1/userinfo";
      allowed_domains = "macro.com";
      hosted_domain = "macro.com";
      validate_hd = true;
      use_pkce = true;
      use_refresh_token = true;
      validate_id_token = true;
      jwk_set_url = "https://www.googleapis.com/oauth2/v3/certs";
      role_attribute_path = "$__env{GRAFANA_ROLE_EXPRESSION}";
      role_attribute_strict = true;
      allow_assign_grafana_admin = true;
      skip_org_role_sync = false;
    };
    "analytics" = {
      reporting_enabled = false;
      check_for_updates = false;
    };
    "plugins" = {
      preinstall_disabled = true;
      preinstall_auto_update = false;
    };
  };
  datasources = (pkgs.formats.yaml { }).generate "datasources.yaml" {
    apiVersion = 1;
    datasources = [
      {
        name = "Prometheus";
        uid = "prometheus";
        type = "prometheus";
        access = "proxy";
        url = "http://proxy:8081/prometheus";
        editable = false;
        isDefault = true;
      }
      {
        name = "Loki";
        uid = "loki";
        type = "loki";
        access = "proxy";
        url = "http://proxy:8081/loki";
        editable = false;
      }
      {
        name = "Tempo";
        uid = "tempo";
        type = "tempo";
        access = "proxy";
        url = "http://proxy:8081/tempo";
        editable = false;
      }
    ];
  };
  alloy = pkgs.writeText "config.alloy" ''
    local.file "otlp_token" {
      filename = "/run/secrets/otlp_token"
      is_secret = true
    }

    otelcol.auth.bearer "ingest" {
      token = local.file.otlp_token.content
    }

    otelcol.receiver.otlp "ingest" {
      http {
        endpoint = "0.0.0.0:4318"
        auth = otelcol.auth.bearer.ingest.handler
        max_request_body_size = "8MiB"
      }
      output {
        logs = [otelcol.processor.memory_limiter.ingest.input]
        traces = [otelcol.processor.memory_limiter.ingest.input]
        metrics = [otelcol.processor.memory_limiter.ingest.input]
      }
    }

    otelcol.processor.memory_limiter "ingest" {
      check_interval = "1s"
      limit = "768MiB"
      spike_limit = "128MiB"
      output {
        logs = [otelcol.processor.batch.ingest.input]
        traces = [otelcol.processor.batch.ingest.input]
        metrics = [otelcol.processor.batch.ingest.input]
      }
    }

    otelcol.processor.batch "ingest" {
      timeout = "1s"
      send_batch_size = 512
      send_batch_max_size = 1024
      output {
        logs = [otelcol.exporter.otlphttp.loki.input]
        traces = [otelcol.exporter.otlp.tempo.input]
        metrics = [otelcol.exporter.prometheus.metrics.input]
      }
    }

    otelcol.exporter.otlphttp "loki" {
      client { endpoint = "http://loki:3100/otlp" }
      sending_queue { queue_size = 256 }
    }

    otelcol.exporter.otlp "tempo" {
      client {
        endpoint = "tempo:4317"
        tls { insecure = true }
      }
      sending_queue { queue_size = 256 }
    }

    otelcol.exporter.prometheus "metrics" {
      forward_to = [prometheus.remote_write.local.receiver]
    }

    prometheus.remote_write "local" {
      endpoint { url = "http://prometheus:9090/api/v1/write" }
    }
  '';
in
pkgs.linkFarm "observability-application-config" [
  {
    name = "grafana.ini";
    path = grafana;
  }
  {
    name = "datasources.yaml";
    path = datasources;
  }
  {
    name = "config.alloy";
    path = alloy;
  }
]
