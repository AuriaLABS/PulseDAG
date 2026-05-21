# Windows/WSL Local Rehearsal (v2.2.18)

This runbook adds a local Windows/WSL rehearsal path for v2.2.18 with conservative defaults:

- RPC bound to localhost (`127.0.0.1`)
- no admin exposure
- CPU miner by default
- GPU optional and skipped unless explicitly requested

## Scenario

1. Build workspace
2. Start 3 local nodes
3. Start 1 external CPU miner
4. Collect endpoint evidence:
   - `/health`
   - `/status`
   - `/release`
   - `/readiness`
   - `/p2p/status`
   - `/sync/status`
   - `/pow/metrics` (if available)
5. Collect logs
6. Create evidence directory
7. Create `summary.md`

## Out of scope / do not require

- GPU required path
- public network exposure
- real funds
- protocol changes for node/miner

## Prerequisites

From repository root (`/workspace/PulseDAG`):

- Rust + Cargo installed
- PowerShell available (`pwsh` in WSL or Windows PowerShell)
- Build targets available via workspace build

## Rehearsal commands

### 1) Start local rehearsal

```powershell
pwsh -File scripts/windows/v2_2_18_start_local_rehearsal.ps1
```

Optional GPU mode (explicit opt-in only):

```powershell
pwsh -File scripts/windows/v2_2_18_start_local_rehearsal.ps1 -UseGpuMiner
```

The script builds with:

```powershell
cargo build --workspace
```

Then starts:

- `target/debug/pulsedagd.exe` (3 instances)
- `target/debug/pulsedag-miner.exe` (1 external miner)

An evidence directory is created automatically under `evidence/` and includes `processes.json`.

### 2) Collect local evidence

```powershell
pwsh -File scripts/windows/v2_2_18_collect_local_evidence.ps1 -EvidenceRoot <EVIDENCE_DIR>
```

Outputs:

- `<EVIDENCE_DIR>/api/*.json` for endpoint captures
- `<EVIDENCE_DIR>/logs/*.log` for node/miner logs
- `<EVIDENCE_DIR>/summary.md`

### 3) Stop local rehearsal

```powershell
pwsh -File scripts/windows/v2_2_18_stop_local_rehearsal.ps1 -EvidenceRoot <EVIDENCE_DIR>
```

Stops all tracked node/miner processes from `processes.json`.

## Expected evidence structure

```text
evidence/windows_wsl_rehearsal_v2_2_18_<timestamp>/
  processes.json
  WORKSPACE_ROOT.txt
  logs/
    node-a.log
    node-b.log
    node-c.log
    miner.log
  api/
    node-a_health.json
    ...
  summary.md
```
