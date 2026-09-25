# Explorer DAG view v1

Status: **FAIL-CLOSED MATCHER IN TREE** (not activated)

Date: 2026-09-25 UTC
Parent issues: #1178, #794
Source thesis: `ROADMAP_V3_0_0.md` Task P7
Depends on: `PULSECLOCK_V1.md`, `GHOSTDAG` selected-parent / blue-red metadata

Planning only. Does not ship a hosted explorer, change Task31 RPC identity, or claim a custody wallet.

## Purpose

A 3.0 explorer must render the DAG, not only a linear block list: parents, blue/red, selected parent, and PulseClock when metadata exists.

The in-tree matcher (`crates/pulsedag-core/src/explorer_dag_v1.rs`) rejects a linear-only view. Default `ExplorerDagAdmissionV1::INACTIVE` keeps this out of the official surface.

## Required fields per block

| Field | Rule |
|---|---|
| `hash` | non-empty |
| `parents` | genesis may be empty; every other block MUST list parents |
| `selected_parent` | required except genesis |
| `merge_color` | `blue`, `red`, or `selected` |
| `blue_score` | GHOSTDAG score |
| `pulse` | optional; if present it must be the observational PulseClock tuple |

A view that only exposes height + hash is `LinearOnly`. Missing PulseClock metadata is allowed; the explorer must not invent host time.

## Authorization

Planning only. Not an official hosted explorer.
