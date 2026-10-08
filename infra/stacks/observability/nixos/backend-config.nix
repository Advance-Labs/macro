{ pkgs }:
let
  yaml = pkgs.formats.yaml { };
in
{
  loki = yaml.generate "loki.yaml" {
    auth_enabled = false;
    server.http_listen_port = 3100;
    common = {
      path_prefix = "/var/lib/loki";
      replication_factor = 1;
      ring = {
        instance_addr = "127.0.0.1";
        kvstore.store = "inmemory";
      };
    };
    schema_config.configs = [
      {
        from = "2024-01-01";
        store = "tsdb";
        object_store = "s3";
        schema = "v13";
        index = {
          prefix = "index_";
          period = "24h";
        };
      }
    ];
    storage_config = {
      aws = {
        region = "@@REGION@@";
        bucketnames = "@@LOGS_BUCKET@@";
      };
      tsdb_shipper = {
        active_index_directory = "/var/lib/loki/index";
        cache_location = "/var/lib/loki/index-cache";
      };
    };
    ingester.wal = {
      enabled = true;
      dir = "/var/lib/loki/wal";
    };
    compactor = {
      working_directory = "/var/lib/loki/compactor";
      retention_enabled = true;
      delete_request_store = "s3";
    };
    limits_config = {
      retention_period = "720h";
      allow_structured_metadata = true;
      ingestion_rate_mb = 8;
      ingestion_burst_size_mb = 16;
      max_query_parallelism = 4;
    };
    querier.max_concurrent = 4;
    analytics.reporting_enabled = false;
  };
  tempo = yaml.generate "tempo.yaml" {
    server.http_listen_port = 3200;
    distributor.receivers.otlp.protocols.grpc.endpoint = "0.0.0.0:4317";
    ingester.max_block_duration = "5m";
    compactor.compaction.block_retention = "168h";
    storage.trace = {
      backend = "s3";
      wal.path = "/var/lib/tempo/wal";
      s3 = {
        bucket = "@@TRACES_BUCKET@@";
        region = "@@REGION@@";
        endpoint = "s3.@@REGION@@.amazonaws.com";
      };
    };
    usage_report.reporting_enabled = false;
  };
  prometheus = yaml.generate "prometheus.yaml" {
    global.scrape_interval = "30s";
    scrape_configs = [
      {
        job_name = "observability";
        static_configs = [
          {
            targets = [
              "prometheus:9090"
              "loki:3100"
              "tempo:3200"
              "alloy:12345"
            ];
          }
        ];
      }
    ];
  };
}
