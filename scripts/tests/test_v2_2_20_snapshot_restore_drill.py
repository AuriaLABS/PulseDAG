import json
import subprocess
import tempfile
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / "v2_2_20_snapshot_restore_drill.sh"


def run(*args):
    return subprocess.run(["bash", str(SCRIPT), *args], capture_output=True, text=True)


def write_json(tmp: Path, name: str, payload: dict) -> Path:
    p = tmp / name
    p.write_text(json.dumps(payload), encoding="utf-8")
    return p


def valid_metadata() -> dict:
    return {
        "chain_id": "pulsedag-restore-drill-v2-2-20",
        "schema_version": 1,
        "best_height": 3,
        "selected_tip": "abc123",
        "state_root": "state-root",
        "created_at": 1_770_000_000,
    }



def test_ci_mode_lowers_default_height_threshold_without_node_launch():
    proc = subprocess.run(
        ["bash", "-n", str(SCRIPT)],
        capture_output=True,
        text=True,
    )
    assert proc.returncode == 0, proc.stderr
    script = SCRIPT.read_text(encoding="utf-8")
    assert 'HEIGHT_THRESHOLD_WAS_SET="${HEIGHT_THRESHOLD+x}"' in script
    assert "HEIGHT_THRESHOLD=2" in script

def test_snapshot_metadata_validation_requires_restore_identity_fields():
    with tempfile.TemporaryDirectory() as d:
        meta = write_json(Path(d), "meta.json", valid_metadata())
        proc = run("--validate-snapshot-metadata", str(meta))
        assert proc.returncode == 0, proc.stderr


def test_snapshot_metadata_validation_rejects_missing_state_root():
    with tempfile.TemporaryDirectory() as d:
        payload = valid_metadata()
        payload.pop("state_root")
        meta = write_json(Path(d), "meta_bad.json", payload)
        proc = run("--validate-snapshot-metadata", str(meta))
        assert proc.returncode != 0


def test_snapshot_metadata_validation_rejects_empty_selected_tip():
    with tempfile.TemporaryDirectory() as d:
        payload = valid_metadata()
        payload["selected_tip"] = ""
        meta = write_json(Path(d), "meta_bad.json", payload)
        proc = run("--validate-snapshot-metadata", str(meta))
        assert proc.returncode != 0


def test_restore_summary_comparison_includes_snapshot_checksum_gate():
    with tempfile.TemporaryDirectory() as d:
        tmp = Path(d)
        base = {
            "chain_id": "pulsedag-restore-drill-v2-2-20",
            "best_height": 3,
            "selected_tip": "abc123",
            "block_count": 4,
            "snapshot_height": 3,
            "snapshot_sha256": "deadbeef",
        }
        a = write_json(tmp, "a.json", base)
        b = write_json(tmp, "b.json", dict(base))
        proc = run("--compare-summaries", str(a), str(b))
        assert proc.returncode == 0, proc.stderr
        out = json.loads(proc.stdout)
        assert out["chain_id_match"] is True
        assert out["best_height_match"] is True
        assert out["selected_tip_match"] is True
        assert out["block_count_match"] is True
        assert out["snapshot_height_match"] is True
        assert out["snapshot_checksum_present"] is True


def test_restore_summary_comparison_detects_tip_mismatch():
    with tempfile.TemporaryDirectory() as d:
        tmp = Path(d)
        a = write_json(tmp, "a.json", {
            "chain_id": "c",
            "best_height": 3,
            "selected_tip": "tip-a",
            "block_count": 4,
            "snapshot_height": 3,
            "snapshot_sha256": "deadbeef",
        })
        b = write_json(tmp, "b.json", {
            "chain_id": "c",
            "best_height": 3,
            "selected_tip": "tip-b",
            "block_count": 4,
            "snapshot_height": 3,
            "snapshot_sha256": "deadbeef",
        })
        proc = run("--compare-summaries", str(a), str(b))
        assert proc.returncode == 0, proc.stderr
        out = json.loads(proc.stdout)
        assert out["selected_tip_match"] is False
