#!/usr/bin/env bash
set -euo pipefail

# snapshot-only helper for legacy Docker deployments. The default Compose
# service now bind-mounts the host dev Ajax home for live state instead.

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
volume="web-server_ajax-web-dev-home"
source_dir="${AJAX_DOCKER_DEV_HOME:-$HOME/.ajax-dev}"

if [[ ! -d "$source_dir" ]]; then
  echo "missing Ajax dev home: $source_dir" >&2
  exit 1
fi

docker volume create "$volume" >/dev/null

docker run --rm -v "$volume:/ajax-dev" ajax-web-dev:local /bin/sh -lc \
  'find /ajax-dev -mindepth 1 -maxdepth 1 -exec rm -rf {} + && mkdir -p /ajax-dev/cache /ajax-dev/logs /ajax-dev/worktrees'

entries=(config.toml ajax.db)
for optional in web-tls-cert.pem web-tls-key.pem web-push-vapid.pem web-push-subscriptions.json; do
  if [[ -e "$source_dir/$optional" ]]; then
    entries+=("$optional")
  fi
done

COPYFILE_DISABLE=1 tar -C "$source_dir" -cf - "${entries[@]}" \
  | docker run --rm -i -v "$volume:/ajax-dev" ajax-web-dev:local \
      tar --warning=no-unknown-keyword -C /ajax-dev -xf -

echo "Seeded $volume from $source_dir"
