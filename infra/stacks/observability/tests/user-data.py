"""Exercise the image's real IMDS parser without contacting AWS or writing /opt."""
import gzip
import io
import json
from pathlib import Path
import runpy
import tempfile
from unittest.mock import patch

script = Path(__file__).resolve().parents[1] / 'nixos/read-user-data.py'
files = {name: '' for name in [
    'compose.json', 'nginx.conf', 'loki.yaml', 'tempo.yaml', 'prometheus.yaml',
]}
files['bootstrap.json'] = json.dumps({'region': 'us-east-2'})
fixture = {'version': 2, 'volumeId': 'vol-0123456789abcdef0', 'files': files}

def check(payload, succeeds):
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory) / 'config'
        def path(value):
            assert value == '/opt/observability'
            return root
        with patch('pathlib.Path', side_effect=path), \
             patch('urllib.request.build_opener') as opener:
            opener.return_value.open.side_effect = [
                io.BytesIO(b'test-imds-token'),
                io.BytesIO(gzip.compress(json.dumps(payload).encode())),
            ]
            try:
                runpy.run_path(str(script))
            except SystemExit:
                assert not succeeds
                assert not root.exists(), 'Invalid input wrote configuration'
            else:
                assert succeeds
                assert (root / 'volume-id').read_text() == fixture['volumeId']
                assert set(p.name for p in root.iterdir()) == set(files) | {'volume-id'}
                requests = opener.return_value.open.call_args_list
                assert requests[0].args[0].method == 'PUT'
                assert requests[1].args[0].get_header('X-aws-ec2-metadata-token') == 'test-imds-token'

check(fixture, True)
check({**fixture, 'version': 1}, False)
check({**fixture, 'volumeId': '/dev/nvme0n1'}, False)
check({**fixture, 'files': {**files, '../escape': 'invalid'}}, False)
check({**fixture, 'files': {**files, 'grafana.ini': 'override baked auth'}}, False)
check({**fixture, 'files': {**files, 'bootstrap.json': '{"region":"us-east-1"}'}}, False)
print('PASS: IMDSv2, version, volume identity, file allowlist and region validation')
