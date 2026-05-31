# Codex task: parent fetch and orphan recovery

Priority: P2 after #545 and #546.

Goal: make nodes recover missing parents and drain orphan blocks under multi-miner pressure.

Scope:

- request missing parent blocks from peers when orphan is queued;
- rate-limit duplicate parent requests;
- prefer source peer, then fallback peers;
- track pending/inflight requests;
- reprocess dependent orphans when parents arrive;
- expose metrics in p2p/sync/readiness evidence;
- add tests for child-before-parent recovery.

Acceptance:

- 3N/1M PASS;
- 5N/1M PASS;
- 5N/2M should improve or PASS;
- 5N/4M stress must produce lower missing-parent backlog or clear evidence.

Guardrails:

- no consensus rule changes;
- no public readiness claim;
- no smart contracts;
- do not copy Kaspa code verbatim.
