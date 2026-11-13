"""Exact-candidate Task30 helper lock provenance regression.

The helper's compatibility-only wasm-bindgen edge must follow the approved
candidate Cargo.lock; it may not resurrect the historical 0.2.100.
This is a local/offline test and authorizes no monetary runtime or launch.
"""
from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from patch_task30_v24_runtime_compat import (
    emit_relay_helper,
    exact_candidate_wasm_bindgen_version,
    load_lock_packages,
    verify_helper_lock,
)

ROOT = Path(__file__).resolve().parents[2]


class Task30CandidateLockTests(unittest.TestCase):
    def test_helper_manifest_uses_exact_resolved_wasm_bindgen(self) -> None:
        packages = load_lock_packages(ROOT / "Cargo.lock")
        wasm = exact_candidate_wasm_bindgen_version(packages)
        self.assertEqual(wasm, "0.2.108")
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder) / "task30-helper"
            emit_relay_helper(ROOT, target)
            manifest = (target / "Cargo.toml").read_text(encoding="utf-8")
            self.assertIn(f'wasm-bindgen = "={wasm}"', manifest)
            self.assertNotIn('wasm-bindgen = "=0.2.100"', manifest)
            # A root-local package only; no dependency may escape the
            # verified exact-Cargo.lock package/checksum graph.
            with (target / "Cargo.lock").open("a", encoding="utf-8") as out:
                out.write(
                    '\n[[package]]\nname = "task30-tx-helper"\nversion = "0.1.0"\n'
                )
            verify_helper_lock(ROOT / "Cargo.lock", target / "Cargo.lock")

    def test_missing_and_duplicate_wasm_bindgen_fail_closed(self) -> None:
        for packages in (
            [],
            [{"name": "wasm-bindgen", "version": "0.2.108", "source": ""}] * 2,
        ):
            with self.subTest(packages=packages), self.assertRaises(SystemExit):
                exact_candidate_wasm_bindgen_version(packages)

    def test_helper_wasm_version_not_in_candidate_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder) / "task30-helper"
            emit_relay_helper(ROOT, target)
            helper = target / "Cargo.lock"
            original = helper.read_text(encoding="utf-8")
            # Simulate an injected old version without allowing a handpicked
            # compatibility fallback, even if all other packages still match.
            old = 'name = "wasm-bindgen"\nversion = "0.2.108"'
            self.assertEqual(original.count(old), 1)
            helper.write_text(
                original.replace(old, 'name = "wasm-bindgen"\nversion = "0.2.100"')
                + '\n[[package]]\nname = "task30-tx-helper"\nversion = "0.1.0"\n',
                encoding="utf-8",
            )
            with self.assertRaises(SystemExit):
                verify_helper_lock(ROOT / "Cargo.lock", helper)


if __name__ == "__main__":
    unittest.main()
