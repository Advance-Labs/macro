import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { gzipSync } from 'node:zlib';
import { roleExpression, type Settings, validateSettings } from './settings';

export const images = {
  grafana:
    'grafana/grafana:13.2.3@sha256:b28bae15e219c998fb0e0424ed724930cc61b1f61fb404d47c862f9a23f9e572',
  loki: 'grafana/loki:3.7.8@sha256:1107dd5274e0ada47e42472b7a7e71f3b2a2fe878878108f3e2f9e51528f0193',
  // The maintained 2.x line supports a single process without Kafka.
  tempo:
    'grafana/tempo:2.10.8@sha256:f0561deb1c68ec44d6e6e7e4487f30106c4e5e768642077695b37958b105812a',
  prometheus:
    'prom/prometheus:v3.15.0@sha256:efd719c99d83b060d9daefdcf00360461adf279f45ef5391f8d111892118753e',
  alloy:
    'grafana/alloy:v1.20.1@sha256:2aa2099af76c0098d4af7a4d6e48f86cb66dc1a000222ad927a1c67c6542d13f',
  nginx:
    'nginx:1.30.5-alpine@sha256:0985e772fb9f729e6fa0980da05fca5d9c468e870eed43071545afa9d2e27d94',
};

export function renderFiles(settings: Settings): Record<string, string> {
  validateSettings(settings);
  const values: Record<string, string> = {
    REGION: settings.region,
    GRAFANA_HOST: settings.grafanaHost,
    OTLP_HOST: settings.otlpHost,
    ROLE_EXPRESSION: roleExpression(settings),
    LOGS_BUCKET: settings.logsBucket,
    TRACES_BUCKET: settings.tracesBucket,
  };
  const files: Record<string, string> = {};
  for (const name of [
    'grafana.ini',
    'nginx.conf',
    'loki.yaml',
    'tempo.yaml',
    'prometheus.yaml',
    'datasources.yaml',
    'config.alloy',
    'refresh-secrets.py',
    'publish-health.py',
  ]) {
    files[name] = readFileSync(join(__dirname, 'assets', name), 'utf8').replace(
      /@@([A-Z_]+)@@/g,
      (_, key: string) => {
        if (!(key in values)) throw new Error(`Unknown template value ${key}`);
        return values[key];
      }
    );
  }
  files['bootstrap.json'] = JSON.stringify({
    region: settings.region,
    secretArn: settings.secretArn,
    host: settings.grafanaHost,
  });
  const common = {
    restart: 'unless-stopped',
    stop_grace_period: '60s',
    read_only: true,
    cap_drop: ['ALL'],
    security_opt: ['no-new-privileges:true'],
    tmpfs: ['/tmp:rw,noexec,nosuid,size=128m'],
    logging: {
      driver: 'local',
      options: { 'max-size': '10m', 'max-file': '3' },
    },
  };
  const config = (name: string, target: string) => `./${name}:${target}:ro`;
  const data = (name: string, target: string) =>
    `/srv/observability/${name}:${target}`;
  const secret = (name: string) =>
    `/run/macro-observability/${name}:/run/secrets/${name}:ro`;
  files['compose.json'] = JSON.stringify(
    {
      name: 'macro-observability',
      services: {
        grafana: {
          ...common,
          image: images.grafana,
          user: '472:472',
          group_add: ['10001'],
          mem_limit: '1g',
          volumes: [
            config('grafana.ini', '/etc/grafana/grafana.ini'),
            config(
              'datasources.yaml',
              '/etc/grafana/provisioning/datasources/main.yaml'
            ),
            data('grafana', '/var/lib/grafana'),
            ...[
              'google_client_id',
              'google_client_secret',
              'grafana_secret_key',
            ].map(secret),
          ],
        },
        loki: {
          ...common,
          image: images.loki,
          user: '10001:10001',
          mem_limit: '4g',
          command: ['-config.file=/etc/loki/config.yaml'],
          volumes: [
            config('loki.yaml', '/etc/loki/config.yaml'),
            data('loki', '/var/lib/loki'),
          ],
        },
        tempo: {
          ...common,
          image: images.tempo,
          user: '10001:10001',
          mem_limit: '3g',
          command: ['-config.file=/etc/tempo/config.yaml'],
          volumes: [
            config('tempo.yaml', '/etc/tempo/config.yaml'),
            data('tempo', '/var/lib/tempo'),
          ],
        },
        prometheus: {
          ...common,
          image: images.prometheus,
          user: '65534:65534',
          mem_limit: '3g',
          command: [
            '--config.file=/etc/prometheus/prometheus.yaml',
            '--storage.tsdb.path=/prometheus',
            '--storage.tsdb.retention.time=30d',
            '--storage.tsdb.retention.size=100GB',
            '--web.enable-remote-write-receiver',
          ],
          volumes: [
            config('prometheus.yaml', '/etc/prometheus/prometheus.yaml'),
            data('prometheus', '/prometheus'),
          ],
        },
        alloy: {
          ...common,
          image: images.alloy,
          user: '10001:10001',
          mem_limit: '1536m',
          command: [
            'run',
            '--storage.path=/var/lib/alloy',
            '--server.http.listen-addr=0.0.0.0:12345',
            '/etc/alloy/config.alloy',
          ],
          volumes: [
            config('config.alloy', '/etc/alloy/config.alloy'),
            data('alloy', '/var/lib/alloy'),
            secret('otlp_token'),
          ],
        },
        proxy: {
          ...common,
          image: images.nginx,
          user: '101:101',
          mem_limit: '256m',
          entrypoint: ['nginx', '-g', 'daemon off;'],
          ports: ['8080:8080'],
          volumes: [config('nginx.conf', '/etc/nginx/nginx.conf')],
        },
      },
    },
    null,
    2
  );
  return files;
}

export function renderUserData(settings: Settings): string {
  const files = Buffer.from(JSON.stringify(renderFiles(settings))).toString(
    'base64'
  );
  const script = readFileSync(join(__dirname, 'assets', 'bootstrap.sh'), 'utf8')
    .replace('@@FILES@@', files)
    .replaceAll('@@VOLUME_ID@@', settings.volumeId);
  const compressed = gzipSync(script);
  if (compressed.length > 16 * 1024) {
    throw new Error('Bootstrap exceeds the EC2 user-data limit');
  }
  return compressed.toString('base64');
}
