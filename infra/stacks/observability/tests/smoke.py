#!/usr/bin/env python3
"""Exercise the rendered stack using isolated local S3 and fake credentials."""
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

root = Path(sys.argv[1])
project = root.name.lower()
compose = json.loads((root / 'compose.json').read_text())
compose['name'] = project
services = compose['services']
for name, service in services.items():
    service['restart'] = 'no'
    volumes = []
    for mount in service['volumes']:
        source, target, *mode = mount.split(':')
        if source.startswith('/srv/observability/'):
            source = str(root / 'data' / name)
            Path(source).mkdir(parents=True, mode=0o777)
            os.chmod(source, 0o777)
        elif source.startswith('/run/macro-observability/'):
            key = Path(source).name
            source = str(root / key)
            Path(source).write_text('smoke-test-credential-not-real-00000000')
            os.chmod(source, 0o444)
        volumes.append(':'.join([source, target, *mode]))
    service['volumes'] = volumes
    if name in ['loki', 'tempo']:
        service['environment'] = {
            'AWS_ACCESS_KEY_ID': 'test', 'AWS_SECRET_ACCESS_KEY': 'test',
            'AWS_EC2_METADATA_DISABLED': 'true',
        }
for name, port in [('proxy', 8080), ('loki', 3100), ('tempo', 3200), ('prometheus', 9090)]:
    services[name]['ports'] = [f'127.0.0.1::{port}']
services['s3'] = {
    'image': 'localstack/localstack:4',
    'environment': {'SERVICES': 's3', 'AWS_DEFAULT_REGION': 'us-east-1'},
}
(root / 'compose.json').write_text(json.dumps(compose))
loki = (root / 'loki.yaml').read_text().replace(
    '  aws:\n', '  aws:\n    endpoint: s3:4566\n    insecure: true\n    s3forcepathstyle: true\n')
(root / 'loki.yaml').write_text(loki)
tempo = (root / 'tempo.yaml').read_text().replace(
    'endpoint: s3.us-east-1.amazonaws.com',
    'endpoint: s3:4566\n      insecure: true\n      forcepathstyle: true')
(root / 'tempo.yaml').write_text(tempo)

def docker(*args):
    result = subprocess.run(
        ['docker', 'compose', '-p', project, '-f', str(root / 'compose.json'), *args],
        text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
    )
    if result.returncode:
        print(result.stdout, file=sys.stderr)
        result.check_returncode()
    return result.stdout.strip()

def url(name, port):
    return 'http://' + docker('port', name, str(port))

class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None

opener = urllib.request.build_opener(NoRedirect)

def request(endpoint, path, data=None, headers=None, method=None):
    request_headers = {'Content-Type': 'application/json', **(headers or {})}
    body = json.dumps(data).encode() if data is not None else None
    req = urllib.request.Request(endpoint + path, data=body, headers=request_headers, method=method)
    try:
        response = opener.open(req, timeout=5)
    except urllib.error.HTTPError as error:
        response = error
    return response.status, response.read().decode(), response.headers

def eventually(check, seconds=60):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        try:
            if check():
                return
        except (OSError, subprocess.CalledProcessError):
            pass
        time.sleep(1)
    raise AssertionError('Timed out waiting for stack condition')

try:
    docker('up', '-d', 's3')
    eventually(lambda: 'Buckets' in docker('exec', '-T', 's3', 'awslocal', 's3api', 'list-buckets'))
    # Evaluate the actual configured JMESPath, including non-approved staff.
    expression = next(line.split(' = ', 1)[1] for line in
                      (root / 'grafana.ini').read_text().splitlines()
                      if line.startswith('role_attribute_path = '))
    role_check = '''import jmespath, sys
expression = sys.argv[1]
for email, role in [('admin@macro.com', 'GrafanaAdmin'),
                    ('reader@macro.com', 'Viewer'),
                    ('unapproved@macro.com', 'Denied'),
                    ('admin@gmail.com', 'Denied')]:
    assert jmespath.search(expression, {'email': email}) == role
assert jmespath.search(expression, {}) == 'Denied'
'''
    docker('exec', '-T', 's3', 'python', '-c', role_check, expression)
    for bucket in ['observability-logs-test', 'observability-traces-test']:
        docker('exec', '-T', 's3', 'awslocal', 's3', 'mb', 's3://' + bucket)
    docker('up', '-d')
    proxy = url('proxy', 8080)
    loki_url = url('loki', 3100)
    tempo_url = url('tempo', 3200)
    prometheus = url('prometheus', 9090)
    eventually(lambda: request(proxy, '/healthz')[0] == 200)
    eventually(lambda: request(loki_url, '/ready')[0] == 200)
    eventually(lambda: request(tempo_url, '/ready')[0] == 200)
    ui = {'Host': 'grafana-dev.macro.com'}
    ingest = {'Host': 'otlp-dev.macro.com'}
    approved = {**ingest, 'Authorization': 'Bearer smoke-test-credential-not-real-00000000'}
    assert request(proxy, '/api/search', headers=ui)[0] == 401
    assert request(proxy, '/api/search', headers={**ui, 'Authorization': 'Basic YWRtaW46YWRtaW4='})[0] == 401
    login = request(proxy, '/login/google', headers=ui)
    assert login[0] == 302
    redirect = urllib.parse.urlparse(login[2]['Location'])
    params = urllib.parse.parse_qs(redirect.query)
    assert redirect.hostname == 'accounts.google.com'
    assert params['redirect_uri'] == ['https://grafana-dev.macro.com/login/google']
    assert params['hd'] == ['macro.com']
    assert params['code_challenge_method'] == ['S256']
    assert 'state' in params
    for path in ['/v1/logs', '/v1/traces', '/v1/metrics']:
        assert request(proxy, path, {}, ingest)[0] in [401, 403]
        assert request(proxy, path, {}, {**ingest, 'Authorization': 'Bearer wrong'})[0] in [401, 403]
        assert request(proxy, path, headers=approved)[0] == 403
        assert request(proxy, path, {}, approved)[0] == 200
    for path in ['/loki/api/v1/query', '/api/traces/123', '/api/v1/query', '/metrics', '/debug/pprof/']:
        assert request(proxy, path, headers=approved)[0] == 404
    assert request(proxy, '/api/search', headers={'Host': 'unknown.macro.com'})[0] == 404
    now = time.time_ns()
    resource = {'attributes': [{'key': 'service.name', 'value': {'stringValue': 'smoke-service'}}]}
    trace_id = '0123456789abcdef0123456789abcdef'
    trace = {'resourceSpans': [{'resource': resource, 'scopeSpans': [{'spans': [{
        'traceId': trace_id, 'spanId': '0123456789abcdef', 'name': 'smoke-span',
        'startTimeUnixNano': str(now - 1_000_000), 'endTimeUnixNano': str(now),
        'kind': 2,
    }]}]}]}
    logs = {'resourceLogs': [{'resource': resource, 'scopeLogs': [{'logRecords': [{
        'timeUnixNano': str(now), 'body': {'stringValue': 'smoke-log-persisted'},
    }]}]}]}
    metrics = {'resourceMetrics': [{'resource': resource, 'scopeMetrics': [{'metrics': [{
        'name': 'smoke_gauge', 'gauge': {'dataPoints': [{'timeUnixNano': str(now), 'asDouble': 42}]},
    }]}]}]}
    for signal, body in [('traces', trace), ('logs', logs), ('metrics', metrics)]:
        assert request(proxy, '/v1/' + signal, body, approved)[0] == 200
    log_query = '/loki/api/v1/query_range?' + urllib.parse.urlencode({'query': '{service_name="smoke-service"}'})
    metric_query = '/api/v1/query?query=smoke_gauge'
    def readable():
        return (
            'smoke-log-persisted' in request(loki_url, log_query)[1]
            and request(tempo_url, '/api/traces/' + trace_id)[0] == 200
            and '42' in request(prometheus, metric_query)[1]
        )
    eventually(readable)
    for endpoint in [loki_url, tempo_url]:
        assert request(endpoint, '/flush', method='POST')[0] in [200, 204]
    def stored(bucket, trace=False):
        listing = json.loads(docker('exec', '-T', 's3', 'awslocal', 's3api', 'list-objects-v2', '--bucket', bucket))
        return any(item['Key'].endswith('/data.parquet') if trace else item['Key'].startswith('fake/')
                   for item in listing.get('Contents', []))
    eventually(lambda: stored('observability-logs-test'))
    eventually(lambda: stored('observability-traces-test', trace=True))
    docker('restart', 'loki', 'tempo', 'prometheus')
    # Docker can assign new ephemeral host ports on restart.
    loki_url = url('loki', 3100)
    tempo_url = url('tempo', 3200)
    prometheus = url('prometheus', 9090)
    eventually(readable)
    print('PASS: anonymous access, Google OAuth redirect, ingestion auth, write-only routes, logs/traces/metrics, S3 flush, restart recovery')
except BaseException:
    print(docker('logs', '--tail', '35'), file=sys.stderr)
    raise
finally:
    docker('down', '--volumes', '--remove-orphans')
    # Container users own local fixture data; clean only this test's directory.
    subprocess.run(['docker', 'run', '--rm', '-v', f'{root}:/fixture', 'nginx:1.30.5-alpine',
                    'sh', '-c', 'rm -rf /fixture/data'], check=True, capture_output=True)
