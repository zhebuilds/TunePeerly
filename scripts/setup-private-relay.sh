#!/usr/bin/env bash
# Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
# Private Ubuntu relay setup for the Windows validation exercise.
set -euo pipefail
if [[ ${EUID} -ne 0 ]]; then
  echo "Run with sudo: sudo bash $0 <public IPv4 address>" >&2
  exit 1
fi
if [[ $# -ne 1 ]]; then
  echo "Usage: sudo bash $0 <public IPv4 address>" >&2
  exit 1
fi
IFS=. read -r -a octets <<< "$1"
if [[ ${#octets[@]} -ne 4 || ! $1 =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "A public IPv4 address is required." >&2; exit 1
fi
for octet in "${octets[@]}"; do
  if [[ ${#octet} -gt 3 ]] || (( 10#$octet > 255 )); then
    echo "Invalid IPv4 address." >&2; exit 1
  fi
done
export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y syncthing-relaysrv openssl
systemctl stop syncthing-relaysrv.service
install -d -o syncthing -g syncthing -m 750 /var/lib/syncthing/relaysrv
install -d -m 755 /etc/systemd/system/syncthing-relaysrv.service.d
umask 077
if [[ ! -s /etc/tunepeerly-relay.env ]]; then
  printf 'RELAY_TOKEN=%s\n' "$(openssl rand -hex 24)" > /etc/tunepeerly-relay.env
fi
chmod 600 /etc/tunepeerly-relay.env
cat > /etc/systemd/system/syncthing-relaysrv.service.d/override.conf <<EOF
[Service]
EnvironmentFile=
EnvironmentFile=/etc/tunepeerly-relay.env
ExecStart=
ExecStart=/usr/bin/strelaysrv -nat=false -protocol=tcp4 -listen=:22067 -ext-address=$1:22067 -keys=/var/lib/syncthing/relaysrv -pools= -status-srv=127.0.0.1:22070 -token=\${RELAY_TOKEN}
Restart=on-failure
RestartSec=5
NoNewPrivileges=true
UMask=0077
EOF
systemctl daemon-reload
systemctl enable syncthing-relaysrv.service
systemctl restart syncthing-relaysrv.service
sleep 3
systemctl is-active --quiet syncthing-relaysrv.service
printf 'Relay active; token stored in /etc/tunepeerly-relay.env (not printed).\n'
systemctl is-enabled syncthing-relaysrv.service
ss -lnt | grep -E ':(22067|22070)\b' || true
printf 'Allow inbound TCP 22067 in cloud rules; restrict SSH to your admin IP.\n'
