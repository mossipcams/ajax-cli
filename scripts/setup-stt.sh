#!/usr/bin/env bash
# Install Moonshine STT host dependencies and ensure [stt] is configured for both
# Ajax profiles on this machine:
#   stable -> ~/.config/ajax/config.toml
#   dev    -> ~/.ajax-dev/config.toml
#
# Shared provider paths live under ~/.ajax-dev so one venv/sidecar serves both.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STT_HOME="${AJAX_STT_HOME:-$HOME/.ajax-dev}"
VENV="$STT_HOME/stt-venv"
PYTHON="$VENV/bin/python"
SIDECAR_SRC="$REPO_ROOT/scripts/ajax-moonshine-sidecar"
SIDECAR_BIN="$STT_HOME/bin/ajax-moonshine-sidecar"

STABLE_CONFIG="${AJAX_STT_STABLE_CONFIG:-$HOME/.config/ajax/config.toml}"
DEV_CONFIG="${AJAX_STT_DEV_CONFIG:-$STT_HOME/config.toml}"

usage() {
  cat <<'EOF'
Usage: scripts/setup-stt.sh

Installs useful-moonshine-onnx into ~/.ajax-dev/stt-venv, copies the reference
sidecar to ~/.ajax-dev/bin/ajax-moonshine-sidecar, and ensures an [stt] block
exists in both stable and dev Ajax config files.

Environment overrides:
  AJAX_STT_HOME          Shared STT install root (default: ~/.ajax-dev)
  AJAX_STT_STABLE_CONFIG Stable config path (default: ~/.config/ajax/config.toml)
  AJAX_STT_DEV_CONFIG    Dev config path (default: ~/.ajax-dev/config.toml)
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

if [[ ! -f "$SIDECAR_SRC" ]]; then
  echo "setup-stt: missing sidecar at $SIDECAR_SRC" >&2
  exit 1
fi

mkdir -p "$STT_HOME/bin"

if [[ ! -x "$PYTHON" ]]; then
  echo "Creating STT virtualenv at $VENV ..."
  python3 -m venv "$VENV"
fi

echo "Installing useful-moonshine-onnx into $VENV ..."
"$PYTHON" -m pip install -q -U pip
"$PYTHON" -m pip install -q useful-moonshine-onnx numpy

install -m 755 "$SIDECAR_SRC" "$SIDECAR_BIN"

PROVIDER_COMMAND="$PYTHON $SIDECAR_BIN"
export PROVIDER_COMMAND STABLE_CONFIG DEV_CONFIG

python3 <<'PY'
import os
import pathlib
import re
import sys

provider_command = os.environ["PROVIDER_COMMAND"]
stable_config = pathlib.Path(os.environ["STABLE_CONFIG"])
dev_config = pathlib.Path(os.environ["DEV_CONFIG"])

stt_block = f"""[stt]
# Two whitespace-separated tokens: provider_command is split, not shell-parsed.
provider_command = "{provider_command}"
language = "en-US"
phrase_end_silence_ms = 700
pause_grace_period_ms = 9000
max_buffered_audio_ms = 2000
finalization_timeout_ms = 5000
"""

section_pattern = re.compile(r"\n\[stt\][\s\S]*?(?=\n\[|\Z)")


def ensure_config(path: pathlib.Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    text = path.read_text() if path.exists() else ""
    if not text.endswith("\n") and text:
        text += "\n"
    if "[stt]" in text:
        text = section_pattern.sub("\n" + stt_block.rstrip() + "\n", text, count=1)
        action = "updated"
    else:
        if text and not text.endswith("\n\n"):
            text += "\n"
        text += stt_block
        action = "added"
    path.write_text(text)
    print(f"setup-stt: {action} [stt] in {path}")


for config_path in (stable_config, dev_config):
    ensure_config(config_path)
PY

echo "Verifying Moonshine import ..."
"$PYTHON" -c "from moonshine_onnx import MoonshineOnnxModel, load_tokenizer"

echo "setup-stt: ready"
echo "  provider: $PROVIDER_COMMAND"
echo "  stable config: $STABLE_CONFIG"
echo "  dev config: $DEV_CONFIG"
