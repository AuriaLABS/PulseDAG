#!/usr/bin/env python3
"""Compatibility entrypoint for the superseded Hickory-only Task31 validator."""

from pathlib import Path
import runpy

runpy.run_path(
    str(Path(__file__).with_name("validate_v2_4_0_lock_only_rustsec_exceptions.py")),
    run_name="__main__",
)
