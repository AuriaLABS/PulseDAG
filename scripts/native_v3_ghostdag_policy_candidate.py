#!/usr/bin/env python3
"""UNAPPROVED symbolic GHOSTDAG-v3 candidate; NO crypto, monetary or runtime admission.

Uses manually assigned integer work and symbolic block IDs to exercise the exact
selected-parent/merge coloring recurrence proposed in #1341. It is not an
independent PoW-valid Rust vector, a policy freeze, or activation evidence.
Does not choose a production K, parent cap, bounds or canonical hash-byte ordering.
"""
from itertools import permutations

MAX_U256 = (1 << 256) - 1
MAX_U512 = (1 << 512) - 1


def select_and_color(blocks, k):
    """blocks: id -> (parent tuple, positive independent PoW work), with G genesis."""
    if type(k) is not int or k < 0:
        raise ValueError("invalid K")
    if "G" not in blocks or blocks["G"] != ((), 0):
        raise ValueError("genesis must be unique and work-free")
    for node, (parents, work) in blocks.items():
        if not isinstance(node, str) or not node:
            raise ValueError("invalid node id")
        if type(work) is not int or (node != "G" and not 1 <= work <= MAX_U256):
            raise ValueError("invalid work contribution")
        if node != "G" and not parents:
            raise ValueError("missing parents")
        if len(set(parents)) != len(parents) or node in parents:
            raise ValueError("duplicate or self parent")
        if any(parent not in blocks for parent in parents):
            raise ValueError("missing ancestry")
    pending = set(blocks)
    past, chosen, blue_past, score, cumulative, height, merge_blues, merge_reds = (
        {}, {}, {}, {}, {}, {}, {}, {}
    )
    while pending:
        ready = sorted(
            n for n in pending if all(parent in past for parent in blocks[n][0])
        )
        if not ready:
            raise ValueError("cycle or unanchored ancestry")
        n = ready[0]
        pending.remove(n)
        parents, own_work = blocks[n]
        past[n] = {n} | set().union(*(past[p] for p in parents))
        if n == "G":
            chosen[n], blue_past[n], score[n], cumulative[n], height[n] = (
                None, {"G"}, 0, 0, 0
            )
            merge_blues[n], merge_reds[n] = (), ()
            continue
        if "G" not in past[n]:
            raise ValueError("unanchored block")
        # Proposed parent cap/frontier rule: immediate parents form an antichain.
        if any(a in past[b] for a in parents for b in parents if a != b):
            raise ValueError("redundant frontier")
        parent = sorted(
            parents, key=lambda p: (-cumulative[p], -score[p], -height[p], p)
        )[0]
        chosen[n] = parent
        height[n] = max(height[p] for p in parents) + 1
        merge = set().union(*(past[p] for p in parents)) - past[parent]
        accepted, rejected, blue_set = [], [], set(blue_past[parent])
        while merge:
            ready_merge = [
                x for x in merge
                if not (set(blocks[x][0]) & merge)
            ]
            if not ready_merge:
                raise ValueError("merge-set topological cycle")
            x = sorted(
                ready_merge,
                key=lambda p: (-cumulative[p], -score[p], -height[p], p)
            )[0]
            merge.remove(x)

            def incomparable(a, b):
                return a not in past[b] and b not in past[a]

            x_anticone = sum(incomparable(x, b) for b in blue_set)
            stays_k_cluster = all(
                sum(incomparable(b, other) for other in blue_set) +
                int(incomparable(b, x)) <= k
                for b in blue_set
            )
            if x_anticone <= k and stays_k_cluster:
                accepted.append(x)
                blue_set.add(x)
            else:
                rejected.append(x)
        new_score = score[parent] + 1 + len(accepted)
        new_work = cumulative[parent] + own_work + sum(
            blocks[b][1] for b in accepted
        )
        if new_score > (1 << 64) - 1 or new_work > MAX_U512:
            raise OverflowError("checked consensus accumulator overflow")
        score[n], cumulative[n] = new_score, new_work
        blue_past[n] = blue_set | {n}
        merge_blues[n], merge_reds[n] = tuple(accepted), tuple(rejected)
    return {
        n: (chosen[n], score[n], cumulative[n], merge_blues[n], merge_reds[n])
        for n in sorted(blocks)
    }


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def self_test():
    fork = {
        "G": ((), 0),
        "A": (("G",), 2),
        "B": (("G",), 512),
        "C": (("A", "B"), 4),
    }
    k0 = select_and_color(fork, 0)
    k1 = select_and_color(fork, 1)
    require(k0["C"] == ("B", 2, 516, (), ("A",)), "K=0 fork mismatch")
    require(k1["C"] == ("B", 3, 518, ("A",), ()), "K=1 fork mismatch")

    triple = {
        "G": ((), 0),
        "A": (("G",), 2),
        "B": (("G",), 512),
        "D": (("G",), 32),
        "X": (("A", "B", "D"), 4),
    }
    k1 = select_and_color(triple, 1)
    k2 = select_and_color(triple, 2)
    require(k1["X"] == ("B", 3, 548, ("D",), ("A",)), "K=1 triple mismatch")
    require(k2["X"] == ("B", 4, 550, ("D", "A"), ()), "K=2 triple mismatch")

    # A longer easy-work selected chain (3 x work=2) is overtaken by
    # a shorter branch with one hard target (512) and an easy child (2).
    # The proposed K-cluster admits 0/2/3 merge-side ancestors as K rises.
    # This is metadata policy only, never live UTXO reorganization authority.
    deep = {
        "G": ((), 0),
        "A": (("G",), 2),
        "A2": (("A",), 2),
        "A3": (("A2",), 2),
        "B": (("G",), 512),
        "B2": (("B",), 2),
        "X": (("A3", "B2"), 2),
    }
    for k, score, work, blues, reds in [
        (0, 3, 516, (), ("A", "A2", "A3")),
        (1, 3, 516, (), ("A", "A2", "A3")),
        (2, 5, 520, ("A", "A2"), ("A3",)),
        (3, 6, 522, ("A", "A2", "A3"), ()),
    ]:
        out = select_and_color(deep, k)
        require(out["A3"][2] == 6, "weak multi-height work")
        require(out["B2"][2] == 514, "hard branch cumulative work")
        require(out["X"] == ("B2", score, work, blues, reds),
                f"multi-height K={k} fork/join classification mismatch")
        for order in (
            ("G", "A", "A2", "A3", "B", "B2", "X"),
            ("X", "B2", "B", "A3", "A2", "A", "G"),
            ("B", "B2", "G", "X", "A3", "A2", "A"),
        ):
            require(select_and_color({n: deep[n] for n in order}, k) == out,
                    f"arrival-order dependent multi-height K={k}")

    # All 120 map arrival orders must produce byte-for-byte identical output.
    for ids in permutations(triple):
        candidate = {n: triple[n] for n in ids}
        require(select_and_color(candidate, 1) == k1, "K=1 permutation mismatch")
        require(select_and_color(candidate, 2) == k2, "K=2 permutation mismatch")

    high = {"G": ((), 0), "A": (("G",), 1 << 255),
            "B": (("A",), 1 << 255)}
    require(select_and_color(high, 1)["B"][2] == 1 << 256, "wide work lost precision")

    bad = [
        {"G": ((), 0), "A": (("MISSING",), 2)},
        {"G": ((), 0), "A": (("B",), 2), "B": (("A",), 4)},
        {"G": ((), 0), "A": (("G",), 2),
         "B": (("A",), 4), "X": (("A", "B"), 8)},
        {"G": ((), 0), "A": (("G", "G"), 2)},
    ]
    for candidate in bad:
        try:
            select_and_color(candidate, 1)
        except ValueError:
            pass
        else:
            raise AssertionError("invalid symbolic ancestry accepted")
    print("PASS: symbolic trial-policy parent/work/K and 120 arrival permutations")


if __name__ == "__main__":
    self_test()
