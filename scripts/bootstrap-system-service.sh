#!/usr/bin/env bash
# One-time (or after changing bind/limits) root setup for the system service.
#
# Run on the VM as root:
#   bash -s -- /var/lib/cellhub 8081 16 16 < scripts/bootstrap-system-service.sh
#
# On a normal NixOS VM /etc/systemd/system is writable, so this installs a
# regular unit. On the CellHub shell-VM /etc/systemd/system is a read-only
# symlink into the Nix store; there the script installs a runtime drop-in
# under /run/systemd/system instead. In both cases it adds a narrow sudo rule
# so the cellhub deploy user can stop/start only this service.
#
# A nixos-rebuild switch may replace /etc/sudoers and the NixOS-managed unit.
# Declare the same service and rule in the VM's NixOS configuration (see
# cluster/agordaro/opi5-server/modules/shell-vm.nix) for a durable setup.
set -euo pipefail

home="${1:-/var/lib/cellhub}"
serve_port="${2:-8081}"
max_rooms="${3:-16}"
max_players="${4:-16}"

user=cellhub
service=cellhub.service
systemctl_bin=$(command -v systemctl)
visudo_bin=$(command -v visudo || true)

if [ "$(id -u)" -ne 0 ]; then
  echo "✗ bootstrap must run as root" >&2
  exit 1
fi
if ! id "$user" >/dev/null 2>&1; then
  echo "✗ user $user not found" >&2
  exit 1
fi
if [ ! -d "$home" ]; then
  echo "✗ missing $home" >&2
  exit 1
fi
if [ -z "$systemctl_bin" ]; then
  echo "✗ systemctl not found" >&2
  exit 1
fi
if [ -z "$visudo_bin" ]; then
  echo "✗ visudo not found" >&2
  exit 1
fi

install -d -o "$user" -g "$user" "$home/.local/bin"

service_unit() {
  cat <<EOF
[Unit]
Description=CellHub authoritative server
After=network.target
Wants=network.target

[Service]
Type=simple
User=$user
Group=$user
WorkingDirectory=$home
ExecStart=$home/.local/bin/cellhub-server --bind 0.0.0.0:$serve_port --static $home --max-rooms $max_rooms --max-players $max_players
Environment=RUST_LOG=cellhub_server=info,tower_http=warn
Environment=HOME=$home
Restart=on-failure
RestartSec=2
LimitNOFILE=65536
NoNewPrivileges=yes
PrivateTmp=yes

[Install]
WantedBy=multi-user.target
EOF
}

# ── systemd unit ────────────────────────────────────────────────────────────
unit_dir=/etc/systemd/system
if [ -w "$unit_dir" ]; then
  unit="$unit_dir/$service"
  if [ -e "$unit" ] || [ -L "$unit" ]; then
    backup="${unit}.nixos-backup"
    if [ ! -e "$backup" ] && [ ! -L "$backup" ]; then
      cp -a "$unit" "$backup"
    fi
    rm -f "$unit"
  fi
  service_unit > "$unit"
  "$systemctl_bin" daemon-reload
  "$systemctl_bin" enable "$service" >/dev/null
  echo "✓ installed $unit"
else
  # NixOS keeps /etc/systemd/system in the read-only store. Override the
  # existing unit with a runtime drop-in instead of trying to create files
  # there. This survives daemon-reload but not a VM reboot.
  if ! "$systemctl_bin" cat "$service" >/dev/null 2>&1; then
    echo "✗ $unit_dir is read-only and $service is not defined" >&2
    exit 1
  fi
  dropin_dir="/run/systemd/system/$service.d"
  install -d "$dropin_dir"
  cat > "$dropin_dir/override.conf" <<EOF
[Unit]
Description=CellHub authoritative server (:8081)

[Service]
User=$user
Group=$user
WorkingDirectory=$home
ExecStart=
ExecStart=$home/.local/bin/cellhub-server --bind 0.0.0.0:$serve_port --static $home --max-rooms $max_rooms --max-players $max_players
Environment=RUST_LOG=cellhub_server=info,tower_http=warn
Environment=HOME=$home
Restart=on-failure
RestartSec=2
LimitNOFILE=65536
NoNewPrivileges=yes
PrivateTmp=yes
EOF
  "$systemctl_bin" daemon-reload
  echo "⚠ $unit_dir is read-only; installed runtime drop-in $dropin_dir/override.conf" >&2
  echo "  declare the service in the VM's NixOS config for persistence" >&2
fi

# ── narrow sudo rule ────────────────────────────────────────────────────────
# sudoers command matching is literal, so use the absolute path that the deploy
# script also uses.
sudoers=/etc/sudoers
marker_begin="# BEGIN cellhub-deploy"
marker_end="# END cellhub-deploy"
sudoers_tmp=$(mktemp /etc/sudoers.cellhub.XXXXXX)
trap 'rm -f "$sudoers_tmp"' EXIT

awk -v begin="$marker_begin" -v end="$marker_end" '
  $0 == begin { skip = 1; next }
  $0 == end   { skip = 0; next }
  !skip       { print }
' "$sudoers" > "$sudoers_tmp"

cat >> "$sudoers_tmp" <<EOF

$marker_begin
$user ALL=(root) NOPASSWD: $systemctl_bin stop $service, \\
    $systemctl_bin start $service, \\
    $systemctl_bin restart $service
$marker_end
EOF

chown root:root "$sudoers_tmp"
chmod 440 "$sudoers_tmp"
"$visudo_bin" -cf "$sudoers_tmp" >/dev/null
mv -f "$sudoers_tmp" "$sudoers"
trap - EXIT
echo "✓ sudo rule installed in $sudoers"

# ── start, if the binary is already present ─────────────────────────────────
if [ ! -x "$home/.local/bin/cellhub-server" ]; then
  echo "✓ $service installed; binary is missing — run just deploy to install it"
  exit 0
fi

"$systemctl_bin" restart "$service"

ready=0
for _ in {1..40}; do
  if curl -fsS "http://127.0.0.1:$serve_port/health" >/dev/null 2>&1; then
    ready=1
    break
  fi
  sleep 0.25
done
if [ "$ready" != 1 ]; then
  echo "✗ cellhub-server is not responding on :$serve_port" >&2
  "$systemctl_bin" status --no-pager "$service" >&2 || true
  journalctl -u "$service" -n 40 --no-pager >&2 || true
  exit 1
fi

echo "✓ $service: cellhub-server on 0.0.0.0:$serve_port, static $home"
