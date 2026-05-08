from __future__ import annotations

import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
LIB = REPO_ROOT / "scripts" / "start-ajax-textual-lib.sh"


class StartupBuildDecisionTests(unittest.TestCase):
    def test_build_is_needed_when_binary_is_missing(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "Cargo.toml").write_text("[workspace]\n", encoding="utf-8")

            self.assertTrue(needs_build(root / "target" / "debug" / "ajax", root))

    def test_build_is_skipped_when_binary_is_newer_than_sources(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "crates" / "ajax-cli" / "src" / "main.rs"
            binary = root / "target" / "debug" / "ajax"
            source.parent.mkdir(parents=True)
            binary.parent.mkdir(parents=True)
            source.write_text("fn main() {}\n", encoding="utf-8")
            binary.write_text("binary\n", encoding="utf-8")
            binary.chmod(0o755)
            set_mtime(source, 1_000)
            set_mtime(binary, 2_000)

            self.assertFalse(needs_build(binary, root))

    def test_build_is_needed_when_source_is_newer_than_binary(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "crates" / "ajax-cli" / "src" / "main.rs"
            binary = root / "target" / "debug" / "ajax"
            source.parent.mkdir(parents=True)
            binary.parent.mkdir(parents=True)
            binary.write_text("binary\n", encoding="utf-8")
            binary.chmod(0o755)
            source.write_text("fn main() {}\n", encoding="utf-8")
            set_mtime(binary, 1_000)
            set_mtime(source, 2_000)

            self.assertTrue(needs_build(binary, root))


def needs_build(binary: Path, repo_root: Path) -> bool:
    script = textwrap.dedent(
        f"""
        source {shell_quote(LIB)}
        if ajax_binary_needs_build {shell_quote(binary)} {shell_quote(repo_root)}; then
          exit 0
        else
          exit 1
        fi
        """
    )
    result = subprocess.run(["bash", "-c", script], check=False)
    return result.returncode == 0


def shell_quote(path: Path) -> str:
    return "'" + str(path).replace("'", "'\\''") + "'"


def set_mtime(path: Path, timestamp: int) -> None:
    import os

    os.utime(path, (timestamp, timestamp))


if __name__ == "__main__":
    unittest.main()
