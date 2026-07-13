#!/usr/bin/env python3
from pathlib import Path

SOURCE_PATH = Path("crates/pulsedag-p2p/src/lib.rs")
MARKER_PATH = Path("crates/pulsedag-p2p/CI_DEADLOCK_FIX_PENDING.md")
ONE_SHOT_WORKFLOW = Path(".github/workflows/apply_p2p_redial_deadlock_fix.yml")
SCRIPT_PATH = Path("scripts/ci/apply_p2p_redial_deadlock_fix.py")
LINT_PATH = Path(".github/workflows/lint.yml")

OLD = '''                    } else if let Ok(mut guard) = inner.lock() {
                        pending_bootnode_dials.insert(*peer_id);
                        bootnode_redial_backoff_secs.insert(*peer_id, 1);
                        bootnode_next_redial_at.insert(*peer_id, now.saturating_add(1));
                        guard.pending_bootnode_dials.insert(peer_id.to_string());
                        guard.pending_bootnode_dial_started_at.insert(peer_id.to_string(), now);
                        guard.bootnode_redial_backoff_secs.insert(peer_id.to_string(), 1);
                        guard.bootnode_next_redial_at.insert(peer_id.to_string(), now.saturating_add(1));
                        guard.bootnode_redial_successes = guard.bootnode_redial_successes.saturating_add(1);
                        note_swarm_event(&inner, format!("dial-success:redial:{peer_id}"));
                    }
'''

NEW = '''                    } else {
                        if let Ok(mut guard) = inner.lock() {
                            pending_bootnode_dials.insert(*peer_id);
                            bootnode_redial_backoff_secs.insert(*peer_id, 1);
                            bootnode_next_redial_at.insert(*peer_id, now.saturating_add(1));
                            guard.pending_bootnode_dials.insert(peer_id.to_string());
                            guard.pending_bootnode_dial_started_at.insert(peer_id.to_string(), now);
                            guard.bootnode_redial_backoff_secs.insert(peer_id.to_string(), 1);
                            guard.bootnode_next_redial_at.insert(peer_id.to_string(), now.saturating_add(1));
                            guard.bootnode_redial_successes =
                                guard.bootnode_redial_successes.saturating_add(1);
                        }
                        note_swarm_event(&inner, format!("dial-success:redial:{peer_id}"));
                    }
'''

ORIGINAL_LINT = '''name: Lint

on:
  pull_request:

jobs:
  lint:
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt

      - name: Cargo.lock hygiene check
        run: |
          set -euo pipefail
          if ! cargo metadata --locked --format-version 1 >/dev/null; then
            echo "::error::Cargo.lock is out of sync. Run 'cargo generate-lockfile' (or intentional 'cargo update') and commit Cargo.lock."
            exit 1
          fi

      - name: Format check
        run: cargo fmt --all -- --check

      - name: Clippy
        run: cargo clippy --all-targets --all-features -- -D warnings
'''


def main() -> None:
    if not MARKER_PATH.exists():
        raise SystemExit("temporary repair marker is absent")

    source = SOURCE_PATH.read_text()
    count = source.count(OLD)
    if count != 1:
        raise SystemExit(f"expected exactly one confirmed recursive-lock block, found {count}")

    SOURCE_PATH.write_text(source.replace(OLD, NEW, 1))
    LINT_PATH.write_text(ORIGINAL_LINT)

    MARKER_PATH.unlink(missing_ok=True)
    ONE_SHOT_WORKFLOW.unlink(missing_ok=True)
    SCRIPT_PATH.unlink(missing_ok=True)


if __name__ == "__main__":
    main()
