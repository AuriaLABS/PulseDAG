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


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("fixture", nargs="?", type=Path)
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("PASS: symbolic DAG oracle negative and permutation tests")
    if args.fixture:
        count = check_fixture(json.loads(args.fixture.read_text(encoding="utf-8")))
        print(f"PASS: {count} deterministic, K-neutral symbolic DAG fixtures")
    if not args.self_test and not args.fixture:
        parser.error("specify --self-test and/or a fixture path")


if __name__ == "__main__":
    main()
