import { expect, test } from 'bun:test';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { gunzipSync } from 'node:zlib';
import { renderFiles, renderUserData } from './render';
import { validateSettings, type Settings } from './settings';

const fixture: Settings = {
  region: 'us-east-1',
  grafanaHost: 'grafana-dev.macro.com',
  otlpHost: 'otlp-dev.macro.com',
  allowedEmails: ['reader@macro.com', 'admin@macro.com'],
  adminEmails: ['admin@macro.com'],
  secretArn:
    'arn:aws:secretsmanager:us-east-1:123456789012:secret:observability-test',
  volumeId: 'vol-0123456789abcdef0',
  logsBucket: 'observability-logs-test',
  tracesBucket: 'observability-traces-test',
};

test('invalid access lists and config injection fail closed', () => {
  for (const change of [
    { allowedEmails: [] },
    { adminEmails: [] },
    { adminEmails: ['unapproved@macro.com'] },
    { allowedEmails: ['outside@gmail.com'] },
    { allowedEmails: ["x' || 'GrafanaAdmin'@macro.com"] },
    { grafanaHost: 'macro.com\nfoo' },
    { otlpHost: fixture.grafanaHost },
    { secretArn: 'not-an-arn' },
  ]) {
    expect(() => validateSettings({ ...fixture, ...change })).toThrow();
  }
});

test('EC2 bootstrap fits its limit and is valid shell', () => {
  const compressed = Buffer.from(renderUserData(fixture), 'base64');
  expect(compressed.length).toBeLessThan(16384);
  const script = gunzipSync(compressed).toString();
  expect(script).not.toContain('@@');
  const result = spawnSync('bash', ['-n'], { input: script, encoding: 'utf8' });
  expect(result.status).toBe(0);
  const compose = JSON.parse(renderFiles(fixture)['compose.json']);
  for (const service of Object.values(compose.services) as {
    restart: string;
  }[]) {
    expect(service.restart).toBe('on-failure');
  }
  expect(compose.services.proxy.ports).toEqual(['8080:8080']);
});

test('volume preparation refuses inspection failures and existing signatures', () => {
  const result = spawnSync('python3', [join(__dirname, 'tests', 'host.py')], {
    stdio: 'inherit',
  });
  expect(result.status).toBe(0);
});

test.skipIf(process.env.OBSERVABILITY_SYSTEMD !== '1')(
  'systemd recovers from secret outages and daemon restarts',
  () => {
    const result = spawnSync(
      'python3',
      [join(__dirname, 'tests', 'host.py'), '--systemd'],
      {
        stdio: 'inherit',
        timeout: 30_000,
      }
    );
    expect(result.status).toBe(0);
  },
  35_000
);

// Explicit opt-in: creates an isolated Docker project with fake credentials.
test.skipIf(process.env.OBSERVABILITY_SMOKE !== '1')(
  'authentication, all three signals, S3 flush and restart recovery',
  () => {
    const directory = mkdtempSync(join(tmpdir(), 'observability-smoke-'));
    try {
      for (const [name, content] of Object.entries(renderFiles(fixture))) {
        writeFileSync(join(directory, name), content);
      }
      const result = spawnSync(
        'python3',
        [join(__dirname, 'tests', 'smoke.py'), directory],
        {
          stdio: 'inherit',
          timeout: 240_000,
        }
      );
      expect(result.status).toBe(0);
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
  250_000
);
