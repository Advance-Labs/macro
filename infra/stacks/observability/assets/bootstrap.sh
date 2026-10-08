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

python3 - <<'PY'
import base64, json
from pathlib import Path
root = Path('/opt/observability')
root.mkdir(mode=0o755, exist_ok=True)
for name, content in json.loads(base64.b64decode('@@FILES@@')).items():
    (root / name).write_text(content)
PY

# Nitro device order can change. Only touch the explicitly provisioned volume.
volume_id='@@VOLUME_ID@@'
device="/dev/disk/by-id/nvme-Amazon_Elastic_Block_Store_${volume_id//-/}"
for attempt in $(seq 1 120); do
  if [ -b "$device" ]; then break; fi
  sleep 5
done
test -b "$device"
bash /opt/observability/prepare-volume.sh "$device"
uuid=$(blkid -s UUID -o value "$device")
mkdir -p /srv/observability
mount_entry="UUID=$uuid /srv/observability ext4 defaults,nofail 0 2"
grep -Fxq "$mount_entry" /etc/fstab || echo "$mount_entry" >> /etc/fstab
mountpoint -q /srv/observability || mount /srv/observability
mountpoint -q /srv/observability
test "$(findmnt --noheadings --output UUID --target /srv/observability)" = "$uuid"
install -d -o 472 -g 472 /srv/observability/grafana
install -d -o 65534 -g 65534 /srv/observability/prometheus
install -d -o 10001 -g 10001 /srv/observability/{loki,tempo,alloy}

# Docker may run only with the data volume mounted. Container on-failure
# policies leave boot startup to observability.service after secrets are ready.
mkdir -p /etc/systemd/system/docker.service.d
cat > /etc/systemd/system/docker.service.d/observability.conf <<'EOF'
[Unit]
RequiresMountsFor=/srv/observability
[Service]
ExecStartPre=/usr/bin/mountpoint -q /srv/observability
EOF
cat > /etc/systemd/system/observability.service <<'EOF'
[Unit]
Description=Macro observability pilot
Requires=docker.service
PartOf=docker.service
After=docker.service network-online.target
Wants=network-online.target
RequiresMountsFor=/srv/observability
StartLimitIntervalSec=0

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
