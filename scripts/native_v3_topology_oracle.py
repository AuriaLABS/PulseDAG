#!/usr/bin/env python3
"""Independent *symbolic* DAG topology oracle for dormant native-v3 diagnostics.

NO header hashes, compact PoW/KHeavyHash, selected-parent authority, blue/red
classification, K, cumulative blue-work, transactions or consensus activation.
Inputs are directed parent graphs using human labels; exact native-v3 security
still requires separate canonical authenticated header-DAG verification.
"""
from __future__ import annotations

import argparse
import json
from itertools import product
from pathlib import Path

MAX_HEADERS = 128
MAX_EDGES = 2048
MAX_ROOTS = 64


class GraphError(ValueError):
    """A symbolic graph is invalid/incomplete or a query is outside its closure."""


def verified_closure(parents: dict[str, list[str]], roots: list[str]) -> set[str]:
    """Return only the complete ancestor closure of roots, NOT all map entries."""
    if not isinstance(parents, dict) or parents.get("G") != []:
        raise GraphError("independent symbolic genesis G must have no parents")
    if not isinstance(roots, list) or not roots or len(roots) > MAX_ROOTS:
        raise GraphError("roots must be a nonempty bounded list")
    if len(roots) != len(set(roots)):
        raise GraphError("duplicate roots")
    visited: set[str] = set()
    visiting: set[str] = set()
    edges = 0

    def visit(node: str) -> None:
        nonlocal edges
        if not isinstance(node, str) or not node:
            raise GraphError("invalid node label")
        if node in visiting:
            raise GraphError("cycle")
        if node in visited:
            return
        if node not in parents:
            raise GraphError("missing reachable ancestor")
        links = parents[node]
        if not isinstance(links, list) or any(
            not isinstance(p, str) or not p for p in links
        ):
            raise GraphError("invalid parent list")
        if node != "G" and not links:
            raise GraphError("non-genesis node missing ancestry")
        if len(links) != len(set(links)):
            raise GraphError("duplicate parents")
        if len(visited) + len(visiting) >= MAX_HEADERS:
            raise GraphError("bounded header closure exceeded")
        visiting.add(node)
        edges += len(links)
        if edges > MAX_EDGES:
            raise GraphError("bounded edge closure exceeded")
        for parent in links:
            visit(parent)
        visiting.remove(node)
        visited.add(node)

    for root in roots:
        visit(root)
    if "G" not in visited:
        raise GraphError("unreachable genesis")
    return visited


def inclusive_past(parents: dict[str, list[str]], node: str) -> set[str]:
    """Graph-theoretic inclusive past; call only after closure validation."""
    past = {node}
    stack = [node]
    while stack:
        for parent in parents[stack.pop()]:
            if parent not in past:
                past.add(parent)
                stack.append(parent)
    return past


def pair_relation(
    parents: dict[str, list[str]], closure: set[str], first: str, second: str
) -> str:
    if first not in closure or second not in closure:
        raise GraphError("queried block outside verified reachable closure")
    if first == second:
        return "SameBlock"
    if first in inclusive_past(parents, second):
        return "FirstIsAncestor"
    if second in inclusive_past(parents, first):
        return "SecondIsAncestor"
    return "Anticone"


def parent_relative_merge_set(
    parents: dict[str, list[str]], roots: list[str], selected: str
) -> list[str]:
    """Pure set algebra for caller-supplied comparison parent, NOT GHOSTDAG."""
    closure = verified_closure(parents, roots)
    if selected not in roots:
        raise GraphError("comparison parent is not a supplied root")
    other_past: set[str] = set()
    for parent in roots:
        if parent != selected:
            other_past.update(inclusive_past(parents, parent))
    return sorted((other_past - inclusive_past(parents, selected)) & closure)


def check_fixture(document: dict) -> int:
    if document.get("schema") != "pulsedag.dormant-v3-symbolic-topology.v1":
        raise GraphError("unknown symbolic fixture schema")
    cases = document.get("cases")
    if not isinstance(cases, list) or not cases:
        raise GraphError("empty fixture set")
    for case in cases:
        graph = case["parents"]
        roots = case["frontier"]
        closure = verified_closure(graph, roots)
        # A graph's map may contain UNQUERIED extra headers outside the closure.
        for pair in case["relations"]:
            got = pair_relation(graph, closure, pair["first"], pair["second"])
            if got != pair["expected"]:
                raise AssertionError(
                    f'{case["name"]} pair {pair["first"]}/{pair["second"]}: '
                    f'{got} != {pair["expected"]}'
                )
        for entry in case["merge_sets"]:
            got = parent_relative_merge_set(graph, roots, entry["relative_to"])
            if got != entry["expected_sorted_set"]:
                raise AssertionError(
                    f'{case["name"]} merge {entry["relative_to"]}: '
                    f'{got} != {entry["expected_sorted_set"]}'
                )
        # Independent-of-map-insertion-order / supplied-root-permutation replay.
        reordered = dict(reversed(list(graph.items())))
        reversed_roots = list(reversed(roots))
        if verified_closure(reordered, reversed_roots) != closure:
            raise AssertionError(f'{case["name"]}: non-deterministic closure')
        for entry in case["merge_sets"]:
            got = parent_relative_merge_set(
                reordered, reversed_roots, entry["relative_to"]
            )
            if got != entry["expected_sorted_set"]:
                raise AssertionError(f'{case["name"]}: non-deterministic merge set')
    return len(cases)


def exhaustive_small_dag_properties() -> tuple[int, int, int, int]:
    """Compare every five-vertex DAG and frontier with an independent bitset DP.

    The expected ancestor bitsets do NOT call our DFS or inclusive-past code.
    This is K-neutral graph algebra, not authenticated consensus selection.
    """
    names = ("G", "A", "B", "C", "D")
    graphs = frontiers = merges = relations = 0
    # Each vertex selects a nonempty subset of previous vertices:
    # 1 * 3 * 7 * 15 = 315 complete labeled DAG constructions.
    for choices in product(*(range(1, 1 << i) for i in range(1, 5))):
        graph = {"G": []}
        reference = {"G": 1}
        for i, choice in enumerate(choices, start=1):
            parents = [names[j] for j in range(i) if choice & (1 << j)]
            graph[names[i]] = parents
            ancestors = 1 << i
            for parent in parents:
                ancestors |= reference[parent]
            reference[names[i]] = ancestors
        graphs += 1
        reordered = dict(reversed(list(graph.items())))

        # Exhaust all 15 nonempty frontiers, including redundant parent-child
        # roots and extra valid map entries outside the verified closure.
        for roots_bits in range(1, 1 << 4):
            roots = [names[i + 1] for i in range(4) if roots_bits & (1 << i)]
            closure_bits = 0
            for root in roots:
                closure_bits |= reference[root]
            expected_closure = {
                name for i, name in enumerate(names) if closure_bits & (1 << i)
            }
            closure = verified_closure(graph, roots)
            if closure != expected_closure:
                raise AssertionError(f"bitset closure mismatch {choices} {roots}")
            if verified_closure(reordered, list(reversed(roots))) != expected_closure:
                raise AssertionError(f"permutation closure mismatch {choices} {roots}")
            frontiers += 1

            for first in sorted(expected_closure):
                for second in sorted(expected_closure):
                    if first == second:
                        expected_relation = "SameBlock"
                    elif reference[second] & (1 << names.index(first)):
                        expected_relation = "FirstIsAncestor"
                    elif reference[first] & (1 << names.index(second)):
                        expected_relation = "SecondIsAncestor"
                    else:
                        expected_relation = "Anticone"
                    actual = pair_relation(graph, closure, first, second)
                    if actual != expected_relation:
                        raise AssertionError(
                            f"bitset relation mismatch {choices} {roots} "
                            f"{first}/{second}: {actual} != {expected_relation}"
                        )
                    relations += 1

            for selected in roots:
                other_bits = 0
                for root in roots:
                    if root != selected:
                        other_bits |= reference[root]
                expected_bits = (other_bits & ~reference[selected]) & closure_bits
                expected = sorted(
                    name for i, name in enumerate(names) if expected_bits & (1 << i)
                )
                actual = parent_relative_merge_set(graph, roots, selected)
                inverted = parent_relative_merge_set(
                    reordered, list(reversed(roots)), selected
                )
                if actual != expected or inverted != expected:
                    raise AssertionError(
                        f"bitset merge mismatch {choices} {roots} {selected}: "
                        f"{actual} vs {expected} (reordered {inverted})"
                    )
                merges += 1
    return graphs, frontiers, merges, relations


def self_test() -> None:
    graph = {
        "G": [], "A": ["G"], "B": ["G"], "M": ["A", "B"],
        "A2": ["A"], "B2": ["B"], "extra": ["G"],
    }
    closure = verified_closure(graph, ["M", "A2", "B2"])
    assert "extra" not in closure and len(closure) == 6
    assert pair_relation(graph, closure, "A", "B") == "Anticone"
    assert pair_relation(graph, closure, "A", "M") == "FirstIsAncestor"
    assert pair_relation(graph, closure, "M", "A") == "SecondIsAncestor"
    assert pair_relation(graph, closure, "A", "A") == "SameBlock"
    assert parent_relative_merge_set(graph, ["M", "A2", "B2"], "M") == [
        "A2", "B2"
    ]
    def rejects(call) -> None:
        try:
            call()
        except GraphError:
            return
        raise AssertionError("expected fail-closed GraphError")

    rejects(lambda: pair_relation(graph, closure, "extra", "A"))
    rejects(lambda: verified_closure({"G": [], "A": ["absent"]}, ["A"]))
    rejects(lambda: verified_closure({"G": [], "A": ["B"], "B": ["A"]}, ["A"]))
    rejects(lambda: verified_closure({"G": [], "A": ["G", "G"]}, ["A"]))
    rejects(lambda: verified_closure({"G": ["A"], "A": ["G"]}, ["A"]))
    rejects(lambda: verified_closure({"G": [], "A": []}, ["A"]))
    rejects(lambda: parent_relative_merge_set(graph, ["M"], "A"))
    chain = {"G": []}
    for index in range(MAX_HEADERS):
        chain[f"H{index}"] = ["G" if index == 0 else f"H{index - 1}"]
    rejects(lambda: verified_closure(chain, [f"H{MAX_HEADERS - 1}"]))
    # Separate integer-bitset DP against graph DFS and inclusive-past logic.
    assert exhaustive_small_dag_properties() == (315, 4_725, 10_080, 77_965)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("fixture", nargs="?", type=Path)
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("PASS: symbolic DAG oracle, 315 graphs, 4725 frontiers, 10080 merge checks, 77965 relation checks")
    if args.fixture:
        count = check_fixture(json.loads(args.fixture.read_text(encoding="utf-8")))
        print(f"PASS: {count} deterministic, K-neutral symbolic DAG fixtures")
    if not args.self_test and not args.fixture:
        parser.error("specify --self-test and/or a fixture path")


if __name__ == "__main__":
    main()
