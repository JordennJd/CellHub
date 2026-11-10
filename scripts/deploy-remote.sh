#!/usr/bin/env bash
# Install/update the authoritative server behind the system cellhub.service.
#
# Invoked over SSH as the cellhub deploy user:
#   bash -s -- /var/lib/cellhub 8081 cellhub.service
#
# The unit itself is installed once by scripts/bootstrap-system-service.sh.
# The frontend is already rsynced into $home. The new binary is expected at
# $home/.local/bin/cellhub-server.new; it replaces the running binary only
# after the service is stopped.
set -euo pipefail

home="${1:?home/static directory}"
serve_port="${2:?health port}"
service="${3:-cellhub.service}"

binary="$home/.local/bin/cellhub-server"
incoming="${binary}.new"
previous="${binary}.previous"
systemctl_bin=$(command -v systemctl)

if [ ! -f "$home/index.html" ]; then
  echo "✗ missing $home/index.html" >&2
  exit 1
fi
if [ ! -x "$incoming" ]; then
  echo "✗ missing executable $incoming" >&2
  exit 1
fi
if [ -z "$systemctl_bin" ]; then
  echo "✗ systemctl not found" >&2
  exit 1
fi
if ! systemctl cat "$service" >/dev/null 2>&1; then
  echo "✗ system service $service is not installed" >&2
  echo "  run scripts/bootstrap-system-service.sh as root on the VM first" >&2
  exit 1
fi

unit_exec=$(systemctl show "$service" -p ExecStart --value 2>/dev/null || true)
if ! grep -q 'cellhub-server' <<<"$unit_exec"; then
  echo "✗ $service does not run cellhub-server yet" >&2
  echo "  current ExecStart: $unit_exec" >&2
  echo "  apply the NixOS config or run scripts/bootstrap-system-service.sh" >&2
  exit 1
fi

stopped=0
installed=0
deployed=0
rollback_done=0

rollback() {
  [ "$rollback_done" = 1 ] && return 0
  rollback_done=1
  if [ "$installed" = 1 ] && [ -x "$previous" ]; then
    echo "↩ restoring previous binary" >&2
    install -m 755 "$previous" "$binary"
  fi
  if [ "$stopped" = 1 ]; then
    sudo -n "$systemctl_bin" start "$service" || true
  fi
}

on_exit() {
  status=$?
  trap - EXIT
  if [ "$deployed" != 1 ]; then
    rollback
  fi
  exit "$status"
}
trap on_exit EXIT

echo "▸ stopping $service"
if ! sudo -n "$systemctl_bin" stop "$service"; then
  echo "✗ cannot stop $service via passwordless sudo" >&2
  echo "  run scripts/bootstrap-system-service.sh as root on the VM first" >&2
  exit 1
fi
stopped=1
systemctl reset-failed "$service" 2>/dev/null || true

if [ -x "$binary" ]; then
  cp -a "$binary" "$previous"
fi
mv -f "$incoming" "$binary"
chmod 755 "$binary"
installed=1

echo "▸ starting $service"
sudo -n "$systemctl_bin" start "$service"

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
  systemctl status --no-pager "$service" >&2 || true
  journalctl -u "$service" -n 40 --no-pager >&2 || true
  exit 1
fi

deployed=1
echo "✓ cellhub-server: 0.0.0.0:$serve_port, static $home, service $service"
