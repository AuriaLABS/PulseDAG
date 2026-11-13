"""Fail-closed negative fixtures for external Kaspa PoW and patched public root.

The repo keeps original 480 upstream PoW comparisons and benchmark in an
independently locked tool workspace. The public lock must not inherit the
historical unmaintained/advisory-bearing Kaspa tree.
"""
from __future__ import annotations

import tempfile
import tomllib
import unittest
from pathlib import Path
from unittest.mock import patch
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import validate_v3_dependency_security as gate  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]


def packages(path: Path) -> set[tuple[str, str]]:
    return {
        (p["name"], p["version"])
        for p in tomllib.loads(path.read_text(encoding="utf-8"))["package"]
    }


def compiled_patched() -> dict[str, set[tuple[str, str]]]:
    values = {root: {(root, "3.0.0"), ("pulsedag-core", "3.0.0")}
              for root in gate.COMPILE_ROOTS}
    for root in gate.EXPECTED_PATCHED_RUSTIX_ROOTS:
        values[root].add(("rustix", "1.1.6"))
    return values


class PublicLockMigrationTests(unittest.TestCase):
    def test_main_lock_has_exact_patched_parents_no_old_kaspa_chain(self) -> None:
        locked = packages(ROOT / "Cargo.lock")
        self.assertTrue(gate.REQUIRED_HICKORY <= locked)
        self.assertTrue(gate.REQUIRED_PATCHED <= locked)
        self.assertIn(("libp2p", "0.57.1"), locked)
        self.assertEqual(
            sorted((n, v) for n, v in locked if n in gate.RETIRED_PUBLIC_ROOT_NAMES),
            [],
        )
        self.assertTrue(gate.FORBIDDEN_LOCKED.isdisjoint(locked))
        self.assertNotIn(("rustix", "1.1.4"), locked)
        self.assertNotIn(("hickory-proto", "0.25.2"), locked)
        self.assertIn(("quinn-proto", "0.11.16"), locked)

    def test_external_lock_preserves_pinned_kaspa_without_polluting_main(self) -> None:
        locked = packages(ROOT / "tools/pow-upstream-reference/Cargo.lock")
        self.assertTrue(gate.REQUIRED_KASPA <= locked)
        self.assertIn(("js-sys", "0.3.77"), locked)
        self.assertIn(("js-sys", "0.3.85"), packages(ROOT / "Cargo.lock"))
        self.assertNotIn(("js-sys", "0.3.77"), packages(ROOT / "Cargo.lock"))

    def test_exact_launch_root_compiler_rustix_proof_and_fail_closed(self) -> None:
        proof = compiled_patched()
        self.assertEqual(
            gate.require_patched_rustix_compiler_evidence(proof)["pulsedagd"],
            ["1.1.6"],
        )
        for root in gate.COMPILE_ROOTS:
            for version in ("1.1.4", "1.1.5", "1.1.7"):
                bad = compiled_patched()
                bad[root].add(("rustix", version))
                with self.subTest(root=root, version=version), self.assertRaises(SystemExit):
                    gate.require_patched_rustix_compiler_evidence(bad)
        for root in gate.EXPECTED_PATCHED_RUSTIX_ROOTS:
            bad = compiled_patched()
            bad[root].remove(("rustix", "1.1.6"))
            with self.subTest(root=root), self.assertRaises(SystemExit):
                gate.require_patched_rustix_compiler_evidence(bad)
        bad = compiled_patched()
        bad.pop("pulsedagd")
        with self.assertRaises(SystemExit):
            gate.require_patched_rustix_compiler_evidence(bad)

    def test_external_oracle_provenance_and_live_pow_linkage_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            core = root / "crates/pulsedag-core"
            (core / "src").mkdir(parents=True)
            (core / "Cargo.toml").write_text(
                '[package]\nname = "pulsedag-core"\nversion = "3.0.0"\n',
                encoding="utf-8",
            )
            (core / "src/pow.rs").write_text(
                "// native_kheavyhash_digest NativeKHeavyHasher\n", encoding="utf-8",
            )
            (core / "src/lib.rs").write_text(
                "mod pow_native_kheavyhash;\n", encoding="utf-8",
            )
            (core / "src/pow_native_kheavyhash.rs").write_text(
                "// native implementation fixture\n", encoding="utf-8",
            )
            oracle = root / "tools/pow-upstream-reference"
            oracle.mkdir(parents=True)
            original_manifest = (ROOT / "tools/pow-upstream-reference/Cargo.toml").read_text(
                encoding="utf-8"
            )
            original_lock = (ROOT / "tools/pow-upstream-reference/Cargo.lock").read_text(
                encoding="utf-8"
            )
            (oracle / "Cargo.toml").write_text(original_manifest, encoding="utf-8")
            (oracle / "Cargo.lock").write_text(original_lock, encoding="utf-8")
            metadata = {"packages": []}
            with patch.object(gate, "ROOT", root), patch.object(gate, "run"):
                self.assertEqual(
                    gate.validate_kaspa_parent_stack(metadata),
                    "native_runtime_external_reference",
                )
                poisoned = original_manifest.replace(
                    gate.EXPECTED_KASPA_REV, "0" * 40
                )
                (oracle / "Cargo.toml").write_text(poisoned, encoding="utf-8")
                with self.assertRaises(SystemExit):
                    gate.validate_kaspa_parent_stack(metadata)
                (oracle / "Cargo.toml").write_text(original_manifest, encoding="utf-8")
                source = core / "src/pow.rs"
                source.write_text(
                    "use kaspa_pow; // native_kheavyhash_digest NativeKHeavyHasher",
                    encoding="utf-8",
                )
                with self.assertRaises(SystemExit):
                    gate.validate_kaspa_parent_stack(metadata)
                source.write_text(
                    "// native_kheavyhash_digest NativeKHeavyHasher", encoding="utf-8"
                )
                with self.assertRaises(SystemExit):
                    gate.validate_kaspa_parent_stack({
                        "packages": [{"name": "kaspa-pow"}]
                    })
                lock_file = oracle / "Cargo.lock"
                lock_file.write_text(
                    original_lock.replace('name = "kaspa-pow"\nversion = "2.0.1"',
                                          'name = "kaspa-pow"\nversion = "2.0.0"'),
                    encoding="utf-8",
                )
                with self.assertRaises(SystemExit):
                    gate.validate_kaspa_parent_stack(metadata)


if __name__ == "__main__":
    unittest.main()
