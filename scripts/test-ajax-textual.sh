#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"

PYTHON_BIN="${PYTHON_BIN:-python3}"
TEXTUAL_TEST_VENV="${AJAX_TEXTUAL_TEST_VENV:-${HOME}/.cache/ajax/textual-test-venv}"

if [[ ! -x "${TEXTUAL_TEST_VENV}/bin/python" ]]; then
  mkdir -p "$(dirname -- "${TEXTUAL_TEST_VENV}")"
  "${PYTHON_BIN}" -m venv "${TEXTUAL_TEST_VENV}"
fi

PYTHON_BIN="${TEXTUAL_TEST_VENV}/bin/python"
"${PYTHON_BIN}" -m pip install -e "${REPO_ROOT}/frontends/textual"
"${PYTHON_BIN}" -m unittest discover -s "${REPO_ROOT}/frontends/textual" -p 'test_*.py'
