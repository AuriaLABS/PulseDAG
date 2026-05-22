import json
import subprocess
import tempfile
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / "v2_2_19_snapshot_restore_drill.sh"


def run(*args):
    return subprocess.run(["bash", str(SCRIPT), *args], capture_output=True, text=True)


def write_json(tmp: Path, name: str, payload: dict) -> Path:
    p = tmp / name
    p.write_text(json.dumps(payload), encoding="utf-8")
    return p


def test_snapshot_metadata_validation_passes():
    with tempfile.TemporaryDirectory() as d:
        tmp = Path(d)
        meta = write_json(tmp, "meta.json", {
            "chain_id": "testnet",
            "schema_version": 3,
            "best_height": 10,
            "selected_tip": "abc",
        })
        proc = run("--validate-snapshot-metadata", str(meta))
        assert proc.returncode == 0, proc.stderr


def test_chain_id_mismatch_detection():
    with tempfile.TemporaryDirectory() as d:
        tmp = Path(d)
        a = write_json(tmp, "a.json", {"chain_id": "a", "schema_version": 1, "best_height": 1, "selected_tip": "x", "block_count": 2})
        b = write_json(tmp, "b.json", {"chain_id": "b", "schema_version": 1, "best_height": 1, "selected_tip": "x", "block_count": 2})
        proc = run("--compare-summaries", str(a), str(b))
        assert proc.returncode == 0, proc.stderr
        out = json.loads(proc.stdout)
        assert out["chain_id_match"] is False


def test_incomplete_snapshot_detection():
    with tempfile.TemporaryDirectory() as d:
        tmp = Path(d)
        meta = write_json(tmp, "meta_bad.json", {"chain_id": "testnet", "best_height": 10})
        proc = run("--validate-snapshot-metadata", str(meta))
        assert proc.returncode != 0


def test_restore_summary_parser_shape():
    with tempfile.TemporaryDirectory() as d:
        tmp = Path(d)
        a = write_json(tmp, "a.json", {"chain_id": "t", "schema_version": 1, "best_height": 1, "selected_tip": "x", "block_count": 2})
        b = write_json(tmp, "b.json", {"chain_id": "t", "schema_version": 1, "best_height": 1, "selected_tip": "x", "block_count": 2})
        proc = run("--compare-summaries", str(a), str(b))
        out = json.loads(proc.stdout)
        expected = {"chain_id_match", "schema_version_match", "best_height_match", "selected_tip_match", "block_count_match"}
        assert expected.issubset(set(out.keys()))
        assert all(out[k] is True for k in expected)
