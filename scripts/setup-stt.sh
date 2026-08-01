#!/usr/bin/env bash
# Install Moonshine v2 (moonshine-voice) host dependencies and ensure [stt] is
# configured for both Ajax profiles on this machine:
#   stable -> ~/.config/ajax/config.toml
#   dev    -> ~/.ajax-dev/config.toml
#
# Shared provider paths live under ~/.ajax-dev so one venv/worker serves both.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STT_HOME="${AJAX_STT_HOME:-$HOME/.ajax-dev}"
VENV="$STT_HOME/stt-venv"
PYTHON="$VENV/bin/python"
WORKER_SRC="$REPO_ROOT/scripts/ajax-moonshine-sidecar"
WORKER_BIN="$STT_HOME/bin/ajax-moonshine-sidecar"

STABLE_CONFIG="${AJAX_STT_STABLE_CONFIG:-$HOME/.config/ajax/config.toml}"
DEV_CONFIG="${AJAX_STT_DEV_CONFIG:-$STT_HOME/config.toml}"

usage() {
  cat <<'EOF'
Usage: scripts/setup-stt.sh

Installs Moonshine v2 (moonshine-voice) into ~/.ajax-dev/stt-venv, copies the
reference worker to ~/.ajax-dev/bin/ajax-moonshine-sidecar, downloads the
English Small Streaming model, and ensures an [stt] block exists in both stable
and dev Ajax config files.

Legacy useful-moonshine-onnx / moonshine/tiny is not used.

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

if [[ ! -f "$WORKER_SRC" ]]; then
  echo "setup-stt: missing worker at $WORKER_SRC" >&2
  exit 1
fi

mkdir -p "$STT_HOME/bin"

if [[ ! -x "$PYTHON" ]]; then
  echo "Creating STT virtualenv at $VENV ..."
  python3 -m venv "$VENV"
fi

echo "Installing moonshine-voice (Moonshine v2) into $VENV ..."
"$PYTHON" -m pip install -q -U pip
# Remove legacy v1 ONNX package if present from older Ajax STT setups.
"$PYTHON" -m pip uninstall -q -y useful-moonshine-onnx 2>/dev/null || true
"$PYTHON" -m pip install -q 'moonshine-voice>=0.1.0' numpy

install -m 755 "$WORKER_SRC" "$WORKER_BIN"

PROVIDER_COMMAND="$PYTHON $WORKER_BIN"
export PROVIDER_COMMAND STABLE_CONFIG DEV_CONFIG

python3 <<'PY'
import os
import pathlib
import re

provider_command = os.environ["PROVIDER_COMMAND"]
stable_config = pathlib.Path(os.environ["STABLE_CONFIG"])
dev_config = pathlib.Path(os.environ["DEV_CONFIG"])

stt_block = f"""[stt]
# Two whitespace-separated tokens: provider_command is split, not shell-parsed.
# Persistent Moonshine v2 worker — model loads once, sessions reuse it.
provider_command = "{provider_command}"
language = "en-US"
phrase_end_silence_ms = 700
pause_grace_period_ms = 9000
max_buffered_audio_ms = 2000
finalization_timeout_ms = 5000
"""

section_pattern = re.compile(r"(?:^|\n)\[stt\][\s\S]*?(?=\n\[|\Z)")


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

echo "Verifying Moonshine v2 import and Small Streaming model ..."
"$PYTHON" - <<'PY'
from moonshine_voice import ModelArch, Transcriber, get_model_for_language

model_path, model_arch = get_model_for_language("en", ModelArch.SMALL_STREAMING)
assert model_arch == ModelArch.SMALL_STREAMING, model_arch
# Constructing Transcriber confirms the ONNX assets load.
Transcriber(model_path=model_path, model_arch=model_arch)
print(f"setup-stt: Moonshine v2 ready ({model_arch}) at {model_path}")
PY

echo "setup-stt: ready"
echo "  provider: $PROVIDER_COMMAND"
echo "  stable config: $STABLE_CONFIG"
echo "  dev config: $DEV_CONFIG"
