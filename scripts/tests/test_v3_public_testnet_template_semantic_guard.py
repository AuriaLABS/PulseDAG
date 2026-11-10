"""Pre-GO public testnet env templates: active literal guards, never grep hits.

Synthetic mutation tests only. No config is rendered or any network started.
"""
from __future__ import annotations

import sys
import unittest
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parents[1]
ROOT = SCRIPTS.parent
sys.path.insert(0, str(SCRIPTS))

from validate_v3_0_0_public_hardening import (  # noqa: E402
    PUBLIC_MINER_ARGS_REQUIRED,
    PUBLIC_TEMPLATE_REQUIRED,
    validate_public_miner_args_template,
    validate_public_template_assignments,
)

TEMPLATES = (
    "seed.env.template",
    "node.env.template",
    "observer.env.template",
)


class PublicTestnetTemplateSemanticGuardTests(unittest.TestCase):
    def template(self, filename: str) -> str:
        return (ROOT / "configs" / "public-testnet" / filename).read_text(
            encoding="utf-8"
        )

    def reject(self, body: str) -> None:
        with self.assertRaises(SystemExit):
            validate_public_template_assignments(body, "SYNTHETIC-UNTRUSTED.env")

    def test_actual_pre_go_templates_have_active_exact_guards(self) -> None:
        for name in TEMPLATES:
            with self.subTest(name=name):
                env = validate_public_template_assignments(self.template(name), name)
                for key, expected in PUBLIC_TEMPLATE_REQUIRED.items():
                    self.assertEqual(env[key], expected)
                self.assertFalse(env["PULSEDAG_CONTRACTS_ENABLED"] == "true")
                self.assertEqual(env["PULSEDAG_PROTOCOL_CONSENSUS_MODE"], "ghostdag_v1")

    def test_comment_cannot_satisfy_required_false(self) -> None:
        original = self.template(TEMPLATES[0])
        for key in ("PULSEDAG_P2P_ENABLED", "PULSEDAG_CONTRACTS_ENABLED",
                    "PULSEDAG_PUBLIC_TESTNET_READY"):
            with self.subTest(key=key):
                self.reject(original.replace(f"{key}=false", f"# {key}=false", 1))

    def test_duplicate_later_assignment_is_fatal_even_when_initial_false_exists(self) -> None:
        original = self.template(TEMPLATES[1])
        for key in ("PULSEDAG_P2P_ENABLED", "PULSEDAG_ADMIN_ENABLED",
                    "PULSEDAG_CONTRACTS_ENABLED",
                    "PULSEDAG_PUBLIC_TESTNET_READY",
                    "PULSEDAG_THIRTY_DAY_PUBLIC_TESTNET_CLOCK_STARTED"):
            with self.subTest(key=key):
                self.reject(original + f"\n{key}=true\n")
                self.reject(original + f"\n{key}=false\n")

    def test_shell_alias_and_ambiguous_syntax_never_pass(self) -> None:
        original = self.template(TEMPLATES[1])
        for replacement in (
            "PULSEDAG_P2P_ENABLED =false",
            "PULSEDAG_P2P_ENABLED= false",
            "PULSEDAG_P2P_ENABLED=\"false\"",
            "export PULSEDAG_P2P_ENABLED=false",
            "PULSEDAG_P2P_ENABLED=false # active",
            "PULSEDAG_P2P_ENABLED=false; PULSEDAG_P2P_ENABLED=true",
            "PULSEDAG_P2P_ENABLED=$(echo false)",
            "PULSEDAG_P2P_ENABLED=false\\",
            "PULSEDAG_P2P_ENABLED=FALSE",
            "PULSEDAG_P2P_ENABLED=0",
        ):
            with self.subTest(replacement=replacement):
                self.reject(original.replace("PULSEDAG_P2P_ENABLED=false", replacement, 1))

    def test_forbidden_effective_rpc_consensus_and_identity_replacements(self) -> None:
        original = self.template(TEMPLATES[2])
        for key, alternative in (
            ("PULSEDAG_RPC_BIND", "0.0.0.0:8080"),
            ("PULSEDAG_API_PROFILE", "admin"),
            ("PULSEDAG_PROTOCOL_CONSENSUS_MODE", "monetary_v3"),
            ("PULSEDAG_CONSENSUS_MODE", "monetary_v3"),
            ("PULSEDAG_NETWORK_PROFILE", "public"),
            ("PULSEDAG_CHAIN_ID", "pulsedag-mainnet-v3"),
            ("PULSEDAG_P2P_BOOTSTRAP", "/dns4/public.example.org/tcp/30333"),
            ("PULSEDAG_PUBLIC_P2P_MULTIADDR", "/ip4/1.2.3.4/tcp/30333"),
        ):
            with self.subTest(key=key):
                self.reject(original.replace(
                    f"{key}={PUBLIC_TEMPLATE_REQUIRED[key]}",
                    f"{key}={alternative}", 1
                ))

    def test_missing_active_assignment_rejected_even_if_note_mentions_it(self) -> None:
        original = self.template(TEMPLATES[0])
        key = "PULSEDAG_CONTRACTS_ENABLED"
        self.reject(original.replace(f"{key}=false", f"# disabled: {key}=false", 1))

    def test_comment_lines_do_not_change_active_guards(self) -> None:
        original = self.template(TEMPLATES[0])
        body = (
            "# Documentation-only PULSEDAG_P2P_ENABLED=true never activates anything\n"
            + original
        )
        env = validate_public_template_assignments(body, "SYNTHETIC.env")
        self.assertEqual(env["PULSEDAG_P2P_ENABLED"], "false")

    def test_reject_non_pulsedag_assignment_and_oversized_payload(self) -> None:
        original = self.template(TEMPLATES[1])
        self.reject(original + "\nOTHER_UNREVIEWED_CONFIG=1\n")
        for injected in (
            "PULSEDAG_CONTRACT_EXECUTION_ENABLED=true",
            "PULSEDAG_GO_PUBLIC_TESTNET=true",
            "PULSEDAG_P2P_ENABLED_SHADOW=false",
        ):
            with self.subTest(injected=injected):
                self.reject(original + "\n" + injected + "\n")
        self.reject(original + ("# " + "x" * (64 * 1024)))

    def test_safety_sensitive_allowed_keys_cannot_weaken_pre_go_defaults(self) -> None:
        original = self.template(TEMPLATES[1])
        for key, unsafe in (
            ("PULSEDAG_RPC_RATE_LIMIT_PER_IP", "false"),
            ("PULSEDAG_RPC_RATE_LIMIT_REQUESTS_PER_MINUTE", "0"),
            ("PULSEDAG_RPC_REQUEST_BODY_LIMIT_BYTES", "9999999"),
            ("PULSEDAG_PRUNE_REQUIRE_SNAPSHOT", "false"),
            ("PULSEDAG_PERSIST_SNAPSHOT_ON_START", "false"),
            ("PULSEDAG_P2P_MDNS", "true"),
            ("PULSEDAG_P2P_IDENTITY_KEY", "/tmp/identity.key"),
            ("PULSEDAG_ROCKSDB_PATH", "/tmp/shared-mainnet"),
            ("PULSEDAG_RPC_CORS_ALLOWLIST", "example.org"),
        ):
            with self.subTest(key=key):
                line = next(
                    item for item in original.splitlines()
                    if item.startswith(f"{key}=")
                )
                self.reject(original.replace(line, f"{key}={unsafe}", 1))



    def test_real_unrendered_miner_cli_has_exact_pre_go_tokens(self) -> None:
        path = ROOT / "configs" / "public-testnet" / "miner.args.template"
        original = path.read_text(encoding="utf-8")
        self.assertEqual(
            validate_public_miner_args_template(original, str(path)),
            PUBLIC_MINER_ARGS_REQUIRED,
        )
        # Comments must not turn a disabled flag into effective arguments.
        annotated = "# --backend gpu is only a documentation warning\n" + original
        self.assertEqual(
            validate_public_miner_args_template(annotated, "SYNTHETIC.args"),
            PUBLIC_MINER_ARGS_REQUIRED,
        )

    def test_miner_cli_template_rejects_comment_alias_duplicate_and_unsafe_value(self) -> None:
        original = (
            ROOT / "configs" / "public-testnet" / "miner.args.template"
        ).read_text(encoding="utf-8")
        mutations = (
            original.replace("--backend\ncpu", "# --backend\n# cpu\n--backend\ngpu", 1),
            original.replace("--backend\ncpu", "--backend\ngpu", 1),
            original.replace("--node\n__TASK31_FREEZE_REQUIRED__", "--node\nmainnet", 1),
            original.replace("--miner-address", "# --miner-address", 1),
            original.replace("--loop\n", "", 1),
            original + "--backend\ngpu\n",
            original + "--contracts-enabled\n",
            original + "$(echo --backend)\n",
            original.replace("--threads\n4", "--threads 4", 1),
            original.replace("--backend\ncpu", "--backend\n'cpu'", 1),
            original.replace("--backend\ncpu", " --backend\ncpu", 1),
            original + "# " + "x" * (8 * 1024),
        )
        for index, mutant in enumerate(mutations):
            with self.subTest(index=index), self.assertRaises(SystemExit):
                validate_public_miner_args_template(mutant, "SYNTHETIC.args")


if __name__ == "__main__":
    unittest.main()
