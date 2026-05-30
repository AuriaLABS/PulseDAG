# Codex task: header and inventory sync

Priority: P3 after parent fetch recovery.

Goal: reduce orphan storms by announcing hashes and headers before full block download.

Scope:

- add bounded block inventory announcements;
- add header request and response flow;
- add full block request and response flow if missing;
- add dependency-aware fetch scheduler;
- fetch parents before children;
- suppress duplicate inflight requests;
- expose inventory and fetch scheduler metrics.

Acceptance:

- existing block propagation remains compatible;
- 3N/1M PASS;
- 5N/1M PASS;
- 5N/2M PASS;
- 5N/4M stress improves convergence and orphan backlog.

Guardrails:

- no consensus changes;
- no public readiness claim;
- no smart contracts;
- do not copy external code verbatim.
