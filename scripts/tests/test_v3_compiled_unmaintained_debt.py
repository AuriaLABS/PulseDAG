"""Offline #1387 insurance: unmaintained advisory exposure must stay explicit.

No upstream migration or waiver is authorized by these standard-library tests.
"""
from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from validate_v3_dependency_security import (  # noqa: E402
    COMPILE_ROOTS,
    REQUIRED_COMPILED_UNMAINTAINED_DEBT,
    REQUIRED_VISIBLE_BLOCKERS,
    require_compiler_reachable_debt,
)

KNOWN = {("async-std", "1.13.2"), ("pin-utils", "0.1.0")}


def fake_compiled() -> dict[str, set[tuple[str, str]]]:
    return {root: KNOWN | {("pulsedag-core", "3.0.0")} for root in COMPILE_ROOTS}


class CompilerReachableDebtTests(unittest.TestCase):
    def test_two_unmaintained_exact_versions_and_advisory_ids_visible(self) -> None:
        self.assertEqual(
            REQUIRED_COMPILED_UNMAINTAINED_DEBT,
            {
                ("async-std", "1.13.2"): "RUSTSEC-2025-0052",
                ("pin-utils", "0.1.0"): "RUSTSEC-2026-0441",
            },
        )
        self.assertIn(("atty", "0.2.14"), REQUIRED_VISIBLE_BLOCKERS)
        rows = require_compiler_reachable_debt(fake_compiled())
        self.assertEqual(set(rows), set(COMPILE_ROOTS))
        for root in COMPILE_ROOTS:
            self.assertEqual(
                rows[root],
                [
                    "async-std@1.13.2:RUSTSEC-2025-0052",
                    "pin-utils@0.1.0:RUSTSEC-2026-0441",
                ],
            )

    def test_removal_of_either_known_compiled_debt_requires_review(self) -> None:
        for package in KNOWN:
            for root in COMPILE_ROOTS:
                compiled = fake_compiled()
                compiled[root].remove(package)
                with self.subTest(package=package, root=root):
                    with self.assertRaises(SystemExit):
                        require_compiler_reachable_debt(compiled)

    def test_silent_patch_version_substitution_is_not_a_fix(self) -> None:
        compiled = fake_compiled()
        compiled["pulsedagd"].remove(("pin-utils", "0.1.0"))
        compiled["pulsedagd"].add(("pin-utils", "0.1.1"))
        with self.assertRaises(SystemExit):
            require_compiler_reachable_debt(compiled)

    def test_absent_or_extra_launch_root_is_not_a_clean_pass(self) -> None:
        compiled = fake_compiled()
        compiled.pop("pulsedag-wallet")
        with self.assertRaises(SystemExit):
            require_compiler_reachable_debt(compiled)
        compiled = fake_compiled()
        compiled["unknown-launch-root"] = KNOWN.copy()
        with self.assertRaises(SystemExit):
            require_compiler_reachable_debt(compiled)

    def test_missing_compiler_evidence_never_means_lock_only(self) -> None:
        with self.assertRaises(SystemExit):
            require_compiler_reachable_debt({root: set() for root in COMPILE_ROOTS})


if __name__ == "__main__":
    unittest.main()
