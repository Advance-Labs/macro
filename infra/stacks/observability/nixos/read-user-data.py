"""Read only nonsecret, versioned configuration from IMDSv2; never execute it."""
import gzip
import json
from pathlib import Path
import re
import urllib.request

endpoint = 'http://169.254.169.254/latest/'
# Metadata access must never use a configured HTTP proxy.
http = urllib.request.build_opener(urllib.request.ProxyHandler({}))
token_request = urllib.request.Request(
    endpoint + 'api/token', method='PUT',
    headers={'X-aws-ec2-metadata-token-ttl-seconds': '60'},
)
with http.open(token_request, timeout=10) as response:
    token = response.read().decode()
request = urllib.request.Request(
    endpoint + 'user-data', headers={'X-aws-ec2-metadata-token': token},
)
with http.open(request, timeout=10) as response:
    raw = response.read(16385)
if len(raw) > 16384:
    raise SystemExit('User data exceeds EC2 limit')
payload = json.loads(gzip.decompress(raw))
if payload.get('version') != 2:
    raise SystemExit('Unsupported observability user-data version')
volume_id = payload.get('volumeId', '')
if not re.fullmatch(r'vol-[a-f0-9]+', volume_id):
    raise SystemExit('Invalid retained volume ID')
expected = {
    'bootstrap.json', 'compose.json', 'nginx.conf', 'loki.yaml',
    'tempo.yaml', 'prometheus.yaml',
}
files = payload.get('files', {})
if set(files) != expected or not all(isinstance(value, str) for value in files.values()):
    raise SystemExit('Invalid observability configuration file set')
config = json.loads(files['bootstrap.json'])
if config.get('region') != 'us-east-2':
    raise SystemExit('Observability requires Ohio configuration')
root = Path('/opt/observability')
root.mkdir(mode=0o755, parents=True, exist_ok=True)
for name, content in files.items():
    (root / name).write_text(content)
(root / 'volume-id').write_text(volume_id)
