{ lib }:
let
  # Keep virtual hosts and locations structured while targeting the pinned nginx
  # container. The NixOS nginx module also emits host-specific paths and units.
  renderLocation = name: location: ''
    location ${name} {
      ${location.extraConfig or ""}
      ${lib.optionalString (location ? proxyPass) "proxy_pass ${location.proxyPass};"}
    }
  '';
  renderHost = host: ''
    server {
      listen ${host.listen};
      server_name ${host.serverName};
      ${host.extraConfig or ""}
      ${lib.concatStrings (lib.mapAttrsToList renderLocation host.locations)}
    }
  '';
  deny = {
    extraConfig = "return 404;";
  };
  websocketHeaders = ''
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection $connection_upgrade;
  '';
  queryLocation = backend: methods: {
    # Variables make nginx re-resolve Docker DNS after a backend is recreated.
    proxyPass = "$backend";
    extraConfig = ''
      limit_except ${methods} { deny all; }
      set $backend http://${backend};
      rewrite ^/[^/]+(/.*)$ $1 break;
    '';
  };
  virtualHosts = [
    {
      listen = "8080 default_server";
      serverName = "_";
      locations = {
        "= /healthz" = {
          proxyPass = "$grafana_health";
          extraConfig = ''
            access_log off;
            set $grafana_health http://grafana:3000/api/health;
            proxy_set_header Host @@GRAFANA_HOST@@;
          '';
        };
        "/" = deny;
      };
    }
    {
      listen = "8080";
      serverName = "@@GRAFANA_HOST@@";
      extraConfig = "add_header Strict-Transport-Security 'max-age=31536000' always;";
      locations."/" = {
        proxyPass = "$grafana";
        extraConfig = ''
          set $grafana http://grafana:3000;
          proxy_set_header Host $host;
          proxy_set_header X-Forwarded-Proto https;
          proxy_set_header X-Forwarded-For $remote_addr;
          ${websocketHeaders}
        '';
      };
    }
    {
      listen = "8080";
      serverName = "@@OTLP_HOST@@";
      locations = {
        "~ ^/v1/(traces|logs|metrics)$" = {
          proxyPass = "$alloy";
          extraConfig = ''
            limit_except POST { deny all; }
            limit_req zone=ingest burst=200 nodelay;
            limit_req_status 429;
            set $alloy http://alloy:4318;
            proxy_read_timeout 30s;
          '';
        };
        "/" = deny;
      };
    }
    {
      # Viewers can call Grafana's datasource proxy with arbitrary paths. Some
      # maintenance actions accept GET, so allow paths as well as methods here.
      # This listener is Docker-internal and is not published on the host.
      listen = "8081";
      serverName = "_";
      locations = {
        "~ ^/prometheus/api/v1/(query|query_range|query_exemplars|series|labels|label/[^/]+/values)$" =
          queryLocation "prometheus:9090" "GET POST";
        "~ ^/prometheus/api/v1/(metadata|status/buildinfo|status/config|rules|alerts|targets|targets/metadata)$" =
          queryLocation "prometheus:9090" "GET";
        "~ ^/loki/loki/api/v1/(query|query_range)$" = queryLocation "loki:3100" "GET POST";
        "~ ^/loki/loki/api/v1/(labels|label/[^/]+/values|series|index/stats|index/volume|index/volume_range|patterns|tail|status/buildinfo|format_query|detected_fields|detected_labels|detected_field/[^/]+/values)$" =
          let
            location = queryLocation "loki:3100" "GET";
          in
          location // { extraConfig = location.extraConfig + websocketHeaders; };
        "~ ^/tempo/api/(echo|search|search/tags|search/tag/[^/]+/values|traces/[a-fA-F0-9]+|v2/traces/[a-fA-F0-9]+|v2/search/tags|v2/search/tag/[^/]+/values|metrics/query|metrics/query_range|status/buildinfo)$" =
          queryLocation "tempo:3200" "GET";
        "/" = deny;
      };
    }
  ];
in
''
  pid /tmp/nginx.pid;
  worker_processes auto;
  error_log /dev/stderr warn;
  events { worker_connections 1024; }
  http {
    # Do not log query strings, request bodies or Authorization headers.
    log_format safe '$remote_addr $host $request_method $uri $status';
    access_log /dev/stdout safe;
    server_tokens off;
    client_body_temp_path /tmp/client_temp;
    proxy_temp_path /tmp/proxy_temp;
    fastcgi_temp_path /tmp/fastcgi_temp;
    uwsgi_temp_path /tmp/uwsgi_temp;
    scgi_temp_path /tmp/scgi_temp;
    client_max_body_size 8m;
    limit_req_zone $server_name zone=ingest:1m rate=100r/s;
    map $http_upgrade $connection_upgrade { default upgrade; ''' close; }
    resolver 127.0.0.11 valid=10s ipv6=off;
    ${lib.concatMapStrings renderHost virtualHosts}
  }
''
