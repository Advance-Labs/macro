"""Exercise the real IMDS validator/renderer without AWS or writes to /opt."""
import gzip
import io
import json
from pathlib import Path
import runpy
import tempfile
from unittest.mock import patch

runtime = runpy.run_path(str(Path(__file__).resolve().parents[1] / 'nixos/read-user-data.py'))
settings = {
    'region': 'us-east-2', 'grafanaHost': 'grafana-dev.macro.com',
    'otlpHost': 'otlp-dev.macro.com', 'allowedEmails': ['reader@macro.com', 'admin@macro.com'],
    'adminEmails': ['admin@macro.com'],
    'secretArn': 'arn:aws:secretsmanager:us-east-2:123456789012:secret:observability-test',
    'volumeId': 'vol-0123456789abcdef0', 'logsBucket': 'observability-logs-test',
    'tracesBucket': 'observability-traces-test',
}
fixture = {'version': 3, 'settings': settings}


def check(payload, succeeds, unknown_parameter=False):
    with tempfile.TemporaryDirectory() as directory:
        templates = Path(directory) / 'templates'
        templates.mkdir()
        output = Path(directory) / 'output'
        for name in runtime['CONFIG_FILES']:
            (templates / name).write_text('@@REGION@@')
        (templates / 'compose.json').write_text(json.dumps({
            'environment': {'GRAFANA_HOST': '@@GRAFANA_HOST@@', 'ROLE': '@@ROLE_EXPRESSION@@'},
        }))
        if unknown_parameter:
            (templates / 'nginx.conf').write_text('@@UNKNOWN@@')
        with patch.dict(runtime['main'].__globals__, {'TEMPLATE_ROOT': templates, 'CONFIG_ROOT': output}), \
             patch('urllib.request.build_opener') as opener:
            opener.return_value.open.side_effect = [
                io.BytesIO(b'test-imds-token'),
                io.BytesIO(gzip.compress(json.dumps(payload).encode())),
            ]
            try:
                runtime['main']()
            except ValueError:
                assert not succeeds
                assert not output.exists(), 'Invalid input wrote configuration'
            else:
                assert succeeds
                assert (output / 'volume-id').read_text() == settings['volumeId']
                assert set(p.name for p in output.iterdir()) == runtime['CONFIG_FILES'] | {'bootstrap.json', 'volume-id'}
                rendered = json.loads((output / 'compose.json').read_text())
                assert rendered['environment']['GRAFANA_HOST'] == settings['grafanaHost']
                assert rendered['environment']['ROLE'] == (
                    'contains(`["admin@macro.com"]`, email) && \'GrafanaAdmin\' || '
                    'contains(`["reader@macro.com","admin@macro.com"]`, email) && \'Viewer\' || \'Denied\''
                )
                requests = opener.return_value.open.call_args_list
                assert requests[0].args[0].method == 'PUT'
                assert requests[1].args[0].get_header('X-aws-ec2-metadata-token') == 'test-imds-token'


check(fixture, True)
for version in [1, 2, 4]:
    check({**fixture, 'version': version}, False)
check({**fixture, 'files': {'compose.json': 'override services'}}, False)
for change in [
    {'volumeId': '/dev/nvme0n1'}, {'region': 'us-east-1'},
    {'grafanaHost': 'bad.macro.com\nserver injected'},
    {'otlpHost': settings['grafanaHost']},
    {'logsBucket': 'bucket\ninjected'},
    {'allowedEmails': []}, {'adminEmails': ['unapproved@macro.com']},
    {'allowedEmails': ['outside@gmail.com']},
    {'allowedEmails': ["x' || 'GrafanaAdmin'@macro.com"]},
    {'secretArn': settings['secretArn'].replace('us-east-2', 'us-east-1')},
    {'unknownSetting': 'injected'},
]:
    check({**fixture, 'settings': {**settings, **change}}, False)
check(fixture, False, unknown_parameter=True)
print('PASS: IMDSv2, runtime schema, identities, safe JSON rendering and fail-closed validation')
