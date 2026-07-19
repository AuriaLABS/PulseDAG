#!/usr/bin/env python3
"""Compatibility entrypoint for the active v2.3.0 observability validator."""

from __future__ import annotations

import runpy
from pathlib import Path


if __name__ == "__main__":
    validator = Path(__file__).with_name("validate_v2_3_0_observability.py")
    runpy.run_path(str(validator), run_name="__main__")
