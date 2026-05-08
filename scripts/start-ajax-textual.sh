#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
source "${SCRIPT_DIR}/start-ajax-textual-lib.sh"

PYTHON_BIN="${PYTHON_BIN:-python3}"
TEXTUAL_APP="${AJAX_TEXTUAL_APP:-${REPO_ROOT}/frontends/textual/ajax_textual.py}"
TEXTUAL_VENV="${AJAX_TEXTUAL_VENV:-${HOME}/.cache/ajax/textual-venv}"

if ! "${PYTHON_BIN}" -c "import textual" >/dev/null 2>&1; then
  if [[ ! -x "${TEXTUAL_VENV}/bin/python" ]]; then
    mkdir -p "$(dirname -- "${TEXTUAL_VENV}")"
    python3 -m venv "${TEXTUAL_VENV}"
  fi

  PYTHON_BIN="${TEXTUAL_VENV}/bin/python"
  "${PYTHON_BIN}" -m pip install -e "${REPO_ROOT}/frontends/textual"
fi

if [[ -z "${AJAX_BIN:-}" ]]; then
  AJAX_BIN="${REPO_ROOT}/target/debug/ajax"
  if ajax_binary_needs_build "${AJAX_BIN}" "${REPO_ROOT}"; then
    cargo build --manifest-path "${REPO_ROOT}/Cargo.toml" -p ajax-cli
  fi
fi

if [[ ! -x "${AJAX_BIN}" ]]; then
  echo "Ajax binary is not executable: ${AJAX_BIN}" >&2
  exit 1
fi

exec "${PYTHON_BIN}" "${TEXTUAL_APP}" --ajax-bin "${AJAX_BIN}" "$@"
