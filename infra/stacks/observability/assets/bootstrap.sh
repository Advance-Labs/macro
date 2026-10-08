#!/bin/bash
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y ca-certificates curl python3 unzip
install -m 0755 -d /etc/apt/keyrings
curl -fsSL https://download.docker.com/linux/ubuntu/gpg -o /etc/apt/keyrings/docker.asc
chmod a+r /etc/apt/keyrings/docker.asc
echo 'deb [arch=amd64 signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/ubuntu noble stable' > /etc/apt/sources.list.d/docker.list
apt-get update
apt-get install -y docker-ce docker-ce-cli containerd.io docker-compose-plugin
curl -fsSL https://awscli.amazonaws.com/awscli-exe-linux-x86_64.zip -o /tmp/awscliv2.zip
unzip -q /tmp/awscliv2.zip -d /tmp
/tmp/aws/install

# Nitro device order can change. Only touch the explicitly provisioned volume.
volume_id='@@VOLUME_ID@@'
device="/dev/disk/by-id/nvme-Amazon_Elastic_Block_Store_${volume_id//-/}"
for attempt in $(seq 1 120); do
  if [ -b "$device" ]; then break; fi
  sleep 5
done
test -b "$device"
filesystem=$(blkid -s TYPE -o value "$device" || true)
if [ -z "$filesystem" ]; then
  # A disk with any existing signature/partition table is not a fresh volume.
  test -z "$(wipefs --no-act --noheadings --output TYPE "$device")"
  mkfs.ext4 "$device"
elif [ "$filesystem" != ext4 ]; then
  echo 'Refusing to format an existing data volume' >&2
  exit 1
fi
uuid=$(blkid -s UUID -o value "$device")
mkdir -p /srv/observability
echo "UUID=$uuid /srv/observability ext4 defaults,nofail 0 2" >> /etc/fstab
mount /srv/observability
mountpoint -q /srv/observability
install -d -o 472 -g 472 /srv/observability/grafana
install -d -o 65534 -g 65534 /srv/observability/prometheus
install -d -o 10001 -g 10001 /srv/observability/{loki,tempo,alloy}

python3 - <<'PY'
import base64, json
from pathlib import Path
root = Path('/opt/observability')
root.mkdir(mode=0o755, exist_ok=True)
for name, content in json.loads(base64.b64decode('@@FILES@@')).items():
    (root / name).write_text(content)
PY

# Prevent Docker's restart policy from starting containers on the root disk
# before the data volume is mounted after a reboot.
mkdir -p /etc/systemd/system/docker.service.d
cat > /etc/systemd/system/docker.service.d/observability.conf <<'EOF'
[Unit]
RequiresMountsFor=/srv/observability
[Service]
ExecStartPre=/usr/bin/mountpoint -q /srv/observability
ExecStartPre=/usr/bin/python3 /opt/observability/refresh-secrets.py
EOF
cat > /etc/systemd/system/observability.service <<'EOF'
[Unit]
Description=Macro observability pilot
Requires=docker.service
After=docker.service network-online.target
Wants=network-online.target
RequiresMountsFor=/srv/observability

[Service]
Type=oneshot
RemainAfterExit=yes
WorkingDirectory=/opt/observability
ExecStartPre=/usr/bin/mountpoint -q /srv/observability
ExecStartPre=/usr/bin/python3 /opt/observability/refresh-secrets.py
ExecStart=/usr/bin/docker compose -f compose.json up -d --remove-orphans --force-recreate
ExecStop=/usr/bin/docker compose -f compose.json down --timeout 60
TimeoutStartSec=900
TimeoutStopSec=120
Restart=on-failure
RestartSec=30

[Install]
WantedBy=multi-user.target
EOF
cat > /etc/systemd/system/observability-health.service <<'EOF'
[Unit]
Description=Publish observability disk usage to CloudWatch
After=network-online.target
[Service]
Type=oneshot
ExecStart=/usr/bin/python3 /opt/observability/publish-health.py
EOF
cat > /etc/systemd/system/observability-health.timer <<'EOF'
[Unit]
Description=Check observability storage every five minutes
[Timer]
OnBootSec=2min
OnUnitActiveSec=5min
[Install]
WantedBy=timers.target
EOF
systemctl daemon-reload
systemctl enable --now observability.service
systemctl enable --now observability-health.timer
