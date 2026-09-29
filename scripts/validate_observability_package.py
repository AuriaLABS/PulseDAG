#!/usr/bin/env python3
"""Active observability validator entrypoint for the v3.0.0 candidate."""

from __future__ import annotations

import runpy
from pathlib import Path


if __name__ == "__main__":
    validator = Path(__file__).with_name("validate_v3_0_0_observability.py")
    runpy.run_path(str(validator), run_name="__main__")
