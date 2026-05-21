#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_ID="${RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}"
ARTIFACT_ROOT="${ARTIFACT_ROOT:-$ROOT_DIR/artifacts/v2.2.18/miner-telemetry-rehearsal}"
RUN_DIR="$ARTIFACT_ROOT/$RUN_ID"
SUMMARY_JSON="$RUN_DIR/summary.json"
SUMMARY_MD="$RUN_DIR/summary.md"
SCENARIO="${SCENARIO:-one-miner-node-a}"
MINER_LOG_GLOB="${MINER_LOG_GLOB:-$RUN_DIR/logs/*.log}"
GPU_REQUESTED="${GPU_REQUESTED:-auto}"
NODE_TARGET_URL="${NODE_TARGET_URL:-http://127.0.0.1:18080}"

info(){ echo "[info] $*"; }
warn(){ echo "[warn] $*" >&2; }
fatal(){ echo "[error] $*" >&2; exit 1; }

require_tools() {
  command -v python3 >/dev/null || fatal "python3 is required"
}

write_manifest() {
  cat > "$RUN_DIR/manifest.env" <<MANIFEST
RUN_ID=$RUN_ID
SCENARIO=$SCENARIO
NODE_TARGET_URL=$NODE_TARGET_URL
GPU_REQUESTED=$GPU_REQUESTED
MINER_LOG_GLOB=$MINER_LOG_GLOB
MANIFEST
}

write_summary() {
  python3 - "$RUN_DIR" "$SCENARIO" "$NODE_TARGET_URL" "$GPU_REQUESTED" "$MINER_LOG_GLOB" "$SUMMARY_JSON" "$SUMMARY_MD" <<'PY'
import glob
import json
import os
import re
import sys
from collections import Counter

run_dir, scenario, node_target_url, gpu_requested, miner_log_glob, summary_json, summary_md = sys.argv[1:]
paths = sorted(glob.glob(miner_log_glob))
lines = []
for p in paths:
    try:
        with open(p, "r", encoding="utf-8", errors="replace") as fh:
            for ln in fh:
                lines.append(ln.rstrip("\n"))
    except OSError:
        pass

def count(rx):
    prog = re.compile(rx, re.IGNORECASE)
    return sum(1 for ln in lines if prog.search(ln))

def extract_last_int(rx):
    prog = re.compile(rx, re.IGNORECASE)
    for ln in reversed(lines):
        m = prog.search(ln)
        if m:
            try:
                return int(m.group(1))
            except ValueError:
                return None
    return None

def extract_last_float(rx):
    prog = re.compile(rx, re.IGNORECASE)
    for ln in reversed(lines):
        m = prog.search(ln)
        if m:
            try:
                return float(m.group(1))
            except ValueError:
                return None
    return None

rejects = Counter()
rx_reject = re.compile(r"reject(?:ed)?(?:_code|\s+code)?[=: ]+([A-Za-z0-9_\-]+)", re.IGNORECASE)
for ln in lines:
    m = rx_reject.search(ln)
    if m:
        rejects[m.group(1)] += 1

backend_cpu = count(r"backend[=: ]+cpu") > 0 or count(r"\bcpu\b") > 0
backend_gpu = count(r"backend[=: ]+gpu") > 0 or count(r"\bgpu\b") > 0

gpu_status = "SKIP"
if gpu_requested.lower() == "false":
    gpu_status = "SKIP"
elif backend_gpu:
    gpu_status = "OBSERVED"
elif gpu_requested.lower() in ("true", "auto"):
    gpu_status = "SKIP"

payload = {
    "run_id": os.path.basename(run_dir),
    "scenario": scenario,
    "node_target_url": node_target_url,
    "log_files": paths,
    "templates_received": count(r"template(s)?\s+(received|fetched)|template_received|/mining/template"),
    "stale_templates_skipped": count(r"stale\s+template\s+skip|stale_templates_skipped|skip.*stale"),
    "submits_total": count(r"submit"),
    "submits_accepted": count(r"accepted=true|submit.*accepted"),
    "submits_rejected": count(r"accepted=false|submit.*rejected"),
    "reject_codes": dict(rejects),
    "backend": {"cpu": "OBSERVED" if backend_cpu else "UNKNOWN", "gpu": gpu_status},
    "hashes_per_sec": extract_last_float(r"hashes(?:_per_sec|/sec|\s+per\s+sec)[=: ]+([0-9]+(?:\.[0-9]+)?)"),
    "last_accepted_height": extract_last_int(r"accepted(?:\s+at)?\s+height[=: ]+([0-9]+)"),
    "miner_restarts": count(r"restart|starting\s+miner|miner\s+boot"),
    "notes": [
        "GPU is optional; SKIP is valid when unavailable or not implemented.",
        "CPU/core verification must remain authoritative.",
        "No pool shares, payouts, or mining protocol changes are introduced by this rehearsal tooling."
    ]
}

with open(summary_json, "w", encoding="utf-8") as fh:
    json.dump(payload, fh, indent=2)

md = []
md.append("# Miner Telemetry Rehearsal v2.2.18")
md.append("")
md.append(f"- Run ID: `{payload['run_id']}`")
md.append(f"- Scenario: `{scenario}`")
md.append(f"- Node target URL: `{node_target_url}`")
md.append(f"- Log files parsed: `{len(paths)}`")
md.append("")
md.append("## Metrics")
for key in [
    "templates_received",
    "stale_templates_skipped",
    "submits_total",
    "submits_accepted",
    "submits_rejected",
    "hashes_per_sec",
    "last_accepted_height",
    "miner_restarts",
]:
    md.append(f"- {key}: `{payload[key]}`")
md.append(f"- backend.cpu: `{payload['backend']['cpu']}`")
md.append(f"- backend.gpu: `{payload['backend']['gpu']}`")
md.append(f"- reject_codes: `{json.dumps(payload['reject_codes'], sort_keys=True)}`")
md.append("")
md.append("## Guardrails")
for n in payload["notes"]:
    md.append(f"- {n}")

with open(summary_md, "w", encoding="utf-8") as fh:
    fh.write("\n".join(md) + "\n")
PY
}

main(){
  require_tools
  mkdir -p "$RUN_DIR" "$RUN_DIR/logs"
  write_manifest
  write_summary
  info "wrote telemetry summary: $SUMMARY_JSON"
  info "wrote telemetry report: $SUMMARY_MD"
}

main "$@"
