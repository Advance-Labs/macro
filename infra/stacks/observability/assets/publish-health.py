#!/usr/bin/env python3
"""Monitor disk pressure outside the stack it protects."""
import json
from pathlib import Path
import shutil
import subprocess

config = json.loads(Path('/opt/observability/bootstrap.json').read_text())
subprocess.run(['mountpoint', '-q', '/srv/observability'], check=True)
usage = shutil.disk_usage('/srv/observability')
metrics = [{
    'MetricName': 'DataDiskUsedPercent',
    'Dimensions': [{'Name': 'Host', 'Value': config['host']}],
    'Unit': 'Percent',
    'Value': 100 * (usage.total - usage.free) / usage.total,
}]
subprocess.run([
    'aws', 'cloudwatch', 'put-metric-data', '--region', config['region'],
    '--namespace', 'Macro/Observability', '--metric-data', json.dumps(metrics),
], check=True)
