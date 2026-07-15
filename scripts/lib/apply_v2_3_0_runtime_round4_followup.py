#!/usr/bin/env python3
from __future__ import annotations

import re
import textwrap
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


def replace_once(path: Path, old: str, new: str, label: str) -> None:
    text = path.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one {label} match, found {count}")
    path.write_text(text.replace(old, new, 1))


def regex_replace_once(path: Path, pattern: str, replacement, label: str) -> None:
    text = path.read_text()
    updated, count = re.subn(pattern, replacement, text, count=1, flags=re.MULTILINE)
    if count != 1:
        raise SystemExit(f"{path}: expected one {label} regex match, found {count}")
    path.write_text(updated)


main = ROOT / "apps/pulsedagd/src/main.rs"
regex_replace_once(
    main,
    r"^(?P<indent>\s*)let canonical_gap = remote_height\.saturating_sub\(local_height\);\n"
    r"(?P=indent)rt\.selected_segment_gap_blocks = canonical_gap;\n"
    r"(?P=indent)rt\.network_selected_height_gap =\n"
    r"(?P=indent)    rt\.network_selected_height_gap\.max\(canonical_gap\);\n",
    lambda match: (
        f"{match.group('indent')}rt.selected_segment_gap_blocks =\n"
        f"{match.group('indent')}    remote_height.saturating_sub(local_height);\n"
    ),
    "invalid runtime canonical-gap field",
)

metrics = ROOT / "crates/pulsedag-rpc/src/handlers/metrics.rs"
replace_once(
    metrics,
    "    handlers::canonical_sync::build_canonical_sync_state,\n",
    textwrap.dedent(
        """\
            handlers::canonical_sync::{
                build_canonical_sync_state_with_remote_evidence,
                remote_sync_evidence_from_p2p_status,
            },
        """
    ),
    "canonical sync imports",
)
replace_once(
    metrics,
    textwrap.dedent(
        """\
            let canonical_sync = build_canonical_sync_state(
                chain,
                runtime,
                chain.dag.blocks.len(),
                now_unix,
                p2p_status
                    .as_ref()
                    .and_then(|snapshot| snapshot.status.selected_sync_peer.clone()),
            );
        """
    ),
    textwrap.dedent(
        """\
            let remote_sync_evidence = remote_sync_evidence_from_p2p_status(
                p2p_status.as_ref().map(|snapshot| &snapshot.status),
                now_unix,
            );
            let canonical_sync = build_canonical_sync_state_with_remote_evidence(
                chain,
                runtime,
                chain.dag.blocks.len(),
                now_unix,
                p2p_status
                    .as_ref()
                    .and_then(|snapshot| snapshot.status.selected_sync_peer.clone()),
                &remote_sync_evidence,
            );
        """
    ),
    "metrics canonical sync evidence wiring",
)

contract = ROOT / "scripts/tests/test_v2_3_0_lag_runtime_driver.sh"
replace_once(
    contract,
    'NODE_MAIN="apps/pulsedagd/src/main.rs"\n',
    'NODE_MAIN="apps/pulsedagd/src/main.rs"\n'
    'METRICS="crates/pulsedag-rpc/src/handlers/metrics.rs"\n',
    "metrics source path",
)
replace_once(
    contract,
    textwrap.dedent(
        """\
        grep -Fq 'let mut selected_segment_completed = false;' "$NODE_MAIN"
        grep -Fq 'selected_segment_session = None;' "$NODE_MAIN"
        grep -Fq 'rt.active_session_remaining_blocks = 0;' "$NODE_MAIN"
        grep -Fq 'rt.peer_addressed_getblock_sent_total = rt' "$NODE_MAIN"
        grep -Fq 'rt.network_selected_height_gap.max(canonical_gap)' "$NODE_MAIN"
        """
    ),
    'python3 scripts/tests/test_v2_3_0_selected_segment_source_semantics.py '
    '"$NODE_MAIN" "$METRICS"\n',
    "selected-segment source assertions",
)
replace_once(
    contract,
    'grep -Fq \'ss -K state established\' "$tmp/patched-harness.sh"\n',
    "",
    "obsolete literal ss invocation assertion",
)

source_test = ROOT / "scripts/tests/test_v2_3_0_selected_segment_source_semantics.py"
source_test.write_text(
    textwrap.dedent(
        r'''\
        #!/usr/bin/env python3
        from __future__ import annotations

        import re
        import sys
        from pathlib import Path

        if len(sys.argv) != 3:
            raise SystemExit(
                f"usage: {sys.argv[0]} apps/pulsedagd/src/main.rs "
                "crates/pulsedag-rpc/src/handlers/metrics.rs"
            )

        node_source = Path(sys.argv[1]).read_text()
        metrics_source = Path(sys.argv[2]).read_text()
        node_checks = {
            "completion flag": r"let\s+mut\s+selected_segment_completed\s*=\s*false",
            "session cleared": r"selected_segment_session\s*=\s*None",
            "remaining blocks cleared": r"active_session_remaining_blocks\s*=\s*0",
            "peer-addressed request counted": r"peer_addressed_getblock_sent_total\s*=\s*rt\s*\.peer_addressed_getblock_sent_total\s*\.saturating_add\(1\)",
            "selected segment gap retained": r"selected_segment_gap_blocks\s*=\s*remote_height\s*\.saturating_sub\(local_height\)",
        }
        metrics_checks = {
            "remote evidence extraction": r"remote_sync_evidence_from_p2p_status\s*\(",
            "canonical evidence-aware builder": r"build_canonical_sync_state_with_remote_evidence\s*\(",
            "remote evidence supplied": r"&remote_sync_evidence",
        }
        missing = [
            f"node: {label}"
            for label, pattern in node_checks.items()
            if not re.search(pattern, node_source)
        ]
        missing.extend(
            f"metrics: {label}"
            for label, pattern in metrics_checks.items()
            if not re.search(pattern, metrics_source)
        )
        if missing:
            raise SystemExit(
                "missing selected-segment source safeguards: " + ", ".join(missing)
            )
        '''
    )
)

print("runtime round-4 follow-up patch applied")
