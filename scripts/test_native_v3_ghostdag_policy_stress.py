"""Additional K-boundary adversarial tests for UNAPPROVED symbolic GHOSTDAG v3.

Symbolic labels and assigned work are NOT authenticated native headers, KHeavyHash,
network propagation measurements or accepted monetary consensus. No production
constant or activation mode imports this file.
"""
from __future__ import annotations

import unittest

from native_v3_ghostdag_policy_candidate import select_and_color


def eight_branch_trial(depth: int = 5) -> dict:
    """Eight independent sibling chains; all parents of X are incomparable."""
    blocks = {"G": ((), 0)}
    tips = []
    for prefix in "ABCDEFGH":
        prev = "G"
        for n in range(1, depth + 1):
            name = f"{prefix}{n}"
            blocks[name] = ((prev,), 2)
            prev = name
        tips.append(prev)
    blocks["X"] = (tuple(tips), 2)
    return blocks


def past_for(blocks: dict, name: str) -> set[str]:
    """Independent recursive ancestry oracle for verifying proposed K-clusters."""
    stack = [name]
    visited = set()
    while stack:
        node = stack.pop()
        if node in visited:
            continue
        visited.add(node)
        stack.extend(blocks[node][0])
    return visited


class DormantKBoundaryAdversarialTests(unittest.TestCase):
    def test_eight_parent_deep_fork_pressures_trial_k_32(self) -> None:
        graph = eight_branch_trial()
        self.assertEqual(len(graph), 42)
        expected = {
            # selected parent, score (inclusive), exact illustrative work,
            # blue merge count, red merge count.
            0: ("A5", 6, 12, 0, 35),
            6: ("A5", 11, 22, 5, 30),
            31: ("A5", 36, 72, 30, 5),
            32: ("A5", 36, 72, 30, 5),
            35: ("A5", 41, 82, 35, 0),
        }
        for k, (parent, score, work, nblues, nreds) in expected.items():
            with self.subTest(k=k):
                selected, actual_score, actual_work, blues, reds = select_and_color(graph, k)["X"]
                self.assertEqual((selected, actual_score, actual_work), (parent, score, work))
                self.assertEqual((len(blues), len(reds)), (nblues, nreds))
                self.assertEqual(set(blues) | set(reds), {
                    name for name in graph if name[0] in "BCDEFGH" and name != "G"
                })
                self.assertFalse(set(blues) & set(reds))

    def test_proposed_k_cluster_applies_to_every_existing_blue(self) -> None:
        graph = eight_branch_trial()
        ancestral = {name: past_for(graph, name) for name in graph}
        for k in (0, 6, 31, 32, 35):
            selected, _, _, blues, _ = select_and_color(graph, k)["X"]
            # Parent A5's blue past is G + A1..A5 in this simple chain.
            blue_past = {selected, "G"} | {f"A{j}" for j in range(1, 5)} | set(blues)
            for a in blue_past:
                incomparable = [
                    b for b in blue_past if b != a
                    and a not in ancestral[b] and b not in ancestral[a]
                ]
                self.assertLessEqual(len(incomparable), k, (k, a, incomparable))

    def test_map_insertion_order_and_competing_parent_order_independent(self) -> None:
        graph = eight_branch_trial()
        original = {k: select_and_color(graph, k) for k in (0, 6, 32, 35)}
        for order in (
            list(graph),
            list(reversed(graph)),
            sorted(graph, reverse=True),
            sorted(graph, key=lambda label: (len(label), label)),
        ):
            reordered = {name: graph[name] for name in order}
            reordered["X"] = (tuple(reversed(graph["X"][0])), 2)
            for k, expected in original.items():
                self.assertEqual(select_and_color(reordered, k), expected)

    def test_bad_ancestry_and_non_integer_k_fail_closed(self) -> None:
        graph = eight_branch_trial()
        for wrong_k in (True, False, -1, 1.0, "32"):
            with self.subTest(k=wrong_k), self.assertRaises(ValueError):
                select_and_color(graph, wrong_k)
        redundant = dict(graph)
        redundant["X"] = (("A4", "A5"), 2)
        with self.assertRaises(ValueError):
            select_and_color(redundant, 32)
        missing = dict(graph)
        missing["B2"] = (("NOT_IN_DAG",), 2)
        with self.assertRaises(ValueError):
            select_and_color(missing, 32)
        cyclic = dict(graph)
        cyclic["A1"] = (("A5",), 2)
        with self.assertRaises(ValueError):
            select_and_color(cyclic, 32)


if __name__ == "__main__":
    unittest.main()
