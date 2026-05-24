#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

dockerfile="$root/Dockerfile.ajax-web"
compose_file="$root/compose.ajax-web.yml"

if [[ ! -f "$dockerfile" ]]; then
  echo "missing Dockerfile.ajax-web" >&2
  exit 1
fi

grep -q 'cargo build .*--bin ajax-cli' "$dockerfile" || {
  echo "Dockerfile.ajax-web must build the ajax-cli binary" >&2
  exit 1
}

grep -q 'ENV AJAX_WEB_SNAPSHOT_ONLY=1' "$dockerfile" || {
  echo "Dockerfile.ajax-web must run the Docker web API in snapshot-only mode" >&2
  exit 1
}

grep -q 'CMD .*ajax-cli.*--home.*\/ajax-dev.*--config.*\/ajax-dev\/config.toml.*--state.*\/ajax-dev\/ajax.db.*--worktree-root.*\/ajax-dev\/worktrees.*web.*--host.*0.0.0.0.*--port.*8788' "$dockerfile" || {
  echo "Dockerfile.ajax-web must default to host-dev paths and ajax-cli web on 0.0.0.0:8788" >&2
  exit 1
}

if [[ ! -f "$compose_file" ]]; then
  echo "missing compose.ajax-web.yml" >&2
  exit 1
fi

grep -q 'restart: unless-stopped' "$compose_file" || {
  echo "compose.ajax-web.yml must use restart: unless-stopped" >&2
  exit 1
}

for expected in '8788:8788' 'ajax-web-dev-home:/ajax-dev' 'ajax-web-dev-home:'; do
  grep -q "$expected" "$compose_file" || {
    echo "compose.ajax-web.yml missing $expected" >&2
    exit 1
  }
done

seed_script="$root/scripts/seed-docker-web-dev.sh"
if [[ ! -f "$seed_script" ]]; then
  echo "missing scripts/seed-docker-web-dev.sh" >&2
  exit 1
fi

grep -q 'HOME/.ajax-dev' "$seed_script" && grep -q 'tar -C "$source_dir"' "$seed_script" || {
  echo "seed-docker-web-dev.sh must copy host ~/.ajax-dev into the Docker volume" >&2
  exit 1
}

grep -q 'ajax-web Docker runtime' "$root/architecture.md" || {
  echo "architecture.md must document the ajax-web Docker runtime" >&2
  exit 1
}
