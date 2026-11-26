#!/usr/bin/env python3
"""Independent Python-stdlib GD-08 ORDER/DIGEST oracle: DORMANT, TEST ONLY.

Uses separately frozen independently generated authentic native-v3 PoW header
vectors. The caller's K=0/K=1 selected-parent and blue/red classifications
remain UNAPPROVED candidate metadata. This is not an oracle for PoW, reward,
signed-UTXO state, monetary eligibility, checkpoint trust, or production GO.
"""
import argparse
import hashlib
import itertools
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "docs/fixtures/native_v3_ghostdag_fork_crosslang.json"
FROZEN = ROOT / "docs/fixtures/native_v3_gd08_order_crosslang.json"
DOMAIN = b"PulseDAG:UNAPPROVED:GD08:authenticated-order-kernel:v1"
ROLE = {"MergeBlue": 0, "MergeRed": 1, "Selected": 2}
SCHEMA = "pulsedag.dormant-v3-gd08-crosslang-order-digest-v1"


def u64(value):
    if not 0 <= value < (1 << 64):
        raise ValueError("non-u64 digest field")
    return value.to_bytes(8, "little")


def ancestors(start, headers):
    seen = set()
    todo = [start]
    while todo:
        key = todo.pop()
        if key in seen:
            continue
        if key not in headers:
            raise ValueError("missing DAG ancestry")
        seen.add(key)
        todo.extend(headers[key]["parents"])
    return seen


def preview(chain, spine, headers, roles):
    """Standalone topology/K-role-bound hashing (not derived policy authority)."""
    if not chain or len(spine) < 2 or len(set(spine)) != len(spine):
        raise ValueError("bad chain or selected spine")
    closure = ancestors(spine[-1], headers)
    if closure != set(roles):
        raise ValueError("missing, extra or duplicate role")
    if headers[spine[0]]["parents"]:
        raise ValueError("non-genesis first spine block")
    for i, block in enumerate(spine):
        if roles[block] != (i, "Selected"):
            raise ValueError("selected-spine assignment mismatch")
        if i and spine[i - 1] not in headers[block]["parents"]:
            raise ValueError("selected parent is not a direct parent")
    for block, (anchor, role) in roles.items():
        if block in spine:
            continue
        if role == "Selected" or anchor < 1 or anchor >= len(spine):
            raise ValueError("invalid side role or anchor")
        first = next(
            (i for i, selected in enumerate(spine) if block in ancestors(selected, headers)),
            None,
        )
        if anchor != first:
            raise ValueError("not the earliest authentic merge anchor")

    remaining = set(closure)
    emitted = set()
    order = []
    while remaining:
        ready = [
            block for block in remaining
            if all(parent in emitted for parent in headers[block]["parents"])
        ]
        if not ready:
            raise ValueError("cycle or missing ancestor")
        chosen = min(
            ready,
            key=lambda block: (roles[block][0], ROLE[roles[block][1]], block),
        )
        order.append(chosen)
        remaining.remove(chosen)
        emitted.add(chosen)
    if order[0] != spine[0] or order[-1] != spine[-1]:
        raise ValueError("genesis/tip order mismatch")

    digest = hashlib.sha256()
    digest.update(DOMAIN)
    encoded = chain.encode()
    digest.update(u64(len(encoded)))
    digest.update(encoded)
    digest.update(u64(len(spine)))
    for block in order:
        anchor, role = roles[block]
        b = block.encode()
        digest.update(u64(len(b)))
        digest.update(b)
        digest.update(u64(anchor))
        digest.update(bytes([ROLE[role]]))
    return {
        "ordered": order,
        "digest": digest.hexdigest(),
        "blue_count": sum(role == "MergeBlue" for _, role in roles.values()),
        "red_count": sum(role == "MergeRed" for _, role in roles.values()),
    }


def build(source):
    if source["status"] != "DORMANT_TEST_ONLY_UNAPPROVED":
        raise ValueError("refuse non-trial input")
    genesis = source["blocks"]["G"]["id"]
    selected = source["chosen_parent"]
    other = source["other_parent"]
    assert selected != other and selected < other
    assert source["blocks"]["A"]["id"] in (selected, other)
    assert source["blocks"]["B"]["id"] in (selected, other)
    assert source["blocks"]["G"]["header"]["parents"] == []
    chain = source["chain_id"]
    cases = {}
    for case, tip_name in [("K0", "X0"), ("K1", "X1")]:
        expected = source["expected"][case]
        tip = source["blocks"][tip_name]["id"]
        assert expected["selected_parent"] == selected
        assert sorted(source["blocks"][tip_name]["header"]["parents"]) == [selected, other]
        for name in ("A", "B", tip_name):
            assert source["blocks"][name]["accepted"], "real PoW corpus required"
        assigned = ("MergeBlue" if other in expected["merge_blues"] else "MergeRed")
        assert expected["merge_blues"] + expected["merge_reds"] == [other]
        spine = [genesis, selected, tip]
        roles = {
            genesis: (0, "Selected"),
            selected: (1, "Selected"),
            other: (2, assigned),
            tip: (2, "Selected"),
        }
        original_headers = {
            block["id"]: block["header"]
            for name, block in source["blocks"].items()
            if name in ("G", "A", "B", tip_name)
        }
        first = preview(chain, spine, original_headers, roles)
        assert first["ordered"] == [genesis, selected, other, tip]
        assert first["blue_count"] == (1 if case == "K1" else 0)
        assert first["red_count"] == (0 if case == "K1" else 1)
        # The same authenticated headers and tip MUST produce a different
        # cryptographic digest if the unapproved blue/red role changes. This
        # proves the digest commits to role metadata; it does not approve K.
        flipped_roles = dict(roles)
        flipped_roles[other] = (
            2,
            "MergeRed" if assigned == "MergeBlue" else "MergeBlue",
        )
        flipped = preview(chain, spine, original_headers, flipped_roles)
        assert flipped["ordered"] == first["ordered"]
        assert flipped["digest"] != first["digest"]
        assert flipped["blue_count"] != first["blue_count"]
        assert flipped["red_count"] != first["red_count"]
        # A merge-side block must not masquerade as selected spine authority.
        fake_selected_roles = dict(roles)
        fake_selected_roles[other] = (2, "Selected")
        try:
            preview(chain, spine, original_headers, fake_selected_roles)
            raise AssertionError("side header falsely assigned Selected was accepted")
        except ValueError:
            pass
        # All 24 map insertion permutations must be byte-identical.
        for keys in itertools.permutations(original_headers):
            permuted = {key: original_headers[key] for key in keys}
            assert preview(chain, spine, permuted, roles) == first
        bad_roles = dict(roles)
        bad_roles[other] = (1, assigned)
        try:
            preview(chain, spine, original_headers, bad_roles)
            raise AssertionError("earlier incorrect merge anchor was accepted")
        except ValueError:
            pass
        try:
            preview(chain, spine, {genesis: original_headers[genesis]}, roles)
            raise AssertionError("missing DAG history was accepted")
        except ValueError:
            pass
        cases[case] = {"tip": tip, **first}
    assert cases["K0"]["digest"] != cases["K1"]["digest"]
    return {
        "schema": SCHEMA,
        "status": "DORMANT_TEST_ONLY_UNAPPROVED",
        "source_fixture": SOURCE.name,
        "chain_id": chain,
        "genesis_ns": source["genesis_ns"],
        "cases": cases,
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--emit", action="store_true")
    parser.add_argument("--verify", action="store_true")
    args = parser.parse_args()
    oracle = build(json.loads(SOURCE.read_text(encoding="utf-8")))
    if args.verify:
        frozen = json.loads(FROZEN.read_text(encoding="utf-8"))
        if oracle != frozen:
            raise AssertionError("independent Python order/digest differs from frozen golden")
        print("PASS: GD-08 dormant Python order/digest frozen parity; 24 permutations x K")
    elif args.emit:
        print(json.dumps(oracle, sort_keys=True, separators=(",", ":")))
    else:
        parser.error("choose --emit or --verify")


if __name__ == "__main__":
    main()
