#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"

PYTHON_BIN="${PYTHON_BIN:-python3}"
TEXTUAL_APP="${AJAX_TEXTUAL_APP:-${REPO_ROOT}/frontends/textual/ajax_textual.py}"

if ! "${PYTHON_BIN}" -c "import textual" >/dev/null 2>&1; then
  echo "Textual is not installed for ${PYTHON_BIN}." >&2
  echo "Install it with: ${PYTHON_BIN} -m pip install -e ${REPO_ROOT}/frontends/textual" >&2
  exit 1
fi

if [[ -z "${AJAX_BIN:-}" ]]; then
  cargo build --manifest-path "${REPO_ROOT}/Cargo.toml" -p ajax-cli
  AJAX_BIN="${REPO_ROOT}/target/debug/ajax"
fi

if [[ ! -x "${AJAX_BIN}" ]]; then
  echo "Ajax binary is not executable: ${AJAX_BIN}" >&2
  exit 1
fi

exec "${PYTHON_BIN}" "${TEXTUAL_APP}" --ajax-bin "${AJAX_BIN}" "$@"
