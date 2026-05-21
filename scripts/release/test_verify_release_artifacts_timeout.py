#!/usr/bin/env python3
import subprocess
import tempfile
from pathlib import Path

from verify_release_artifacts import run_smoke


def _write_executable(path: Path, content: str) -> None:
    path.write_text(content, encoding="utf-8")
    path.chmod(0o755)


def test_smoke_timeout_fails_quickly() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        fake = Path(tmp) / "pulsedagd"
        _write_executable(
            fake,
            "#!/usr/bin/env python3\nimport time\ntime.sleep(2)\n",
        )
        try:
            run_smoke(fake, "pulsedagd", smoke_timeout_secs=1)
            raise AssertionError("expected timeout failure")
        except SystemExit as exc:
            assert "Smoke command timed out" in str(exc)


def test_smoke_version_passes() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        fake = Path(tmp) / "pulsedagd"
        _write_executable(
            fake,
            "#!/usr/bin/env python3\nimport sys\n"
            "if '--version' in sys.argv:\n    print('pulsedagd 0.0.0')\n    sys.exit(0)\n"
            "if '--help' in sys.argv:\n    print('usage: pulsedagd')\n    sys.exit(0)\n"
            "sys.exit(0)\n",
        )
        run_smoke(fake, "pulsedagd", smoke_timeout_secs=1)


def test_legacy_miner_help_code1_is_accepted() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        fake = Path(tmp) / "pulsedag-miner"
        _write_executable(
            fake,
            "#!/usr/bin/env python3\nimport sys\n"
            "if '--help' in sys.argv:\n    print('usage: pulsedag-miner')\n    sys.exit(1)\n"
            "if '--version' in sys.argv:\n    print('pulsedag-miner 0.0.0')\n    sys.exit(0)\n"
            "sys.exit(0)\n",
        )
        run_smoke(fake, "pulsedag-miner", smoke_timeout_secs=1)


if __name__ == "__main__":
    test_smoke_timeout_fails_quickly()
    test_smoke_version_passes()
    test_legacy_miner_help_code1_is_accepted()
    print("ok")
