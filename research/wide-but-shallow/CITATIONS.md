# External classifications — verified citations and confirmed problem statements

Web pass (2026-06-22). For the few sampled in-scheme problems whose *full algorithmic correctness*
is a standalone formalization effort, we classify the scheme in Lean (at the corrected bar:
problem-specific, not optimality) and cite an existing formal proof for the heavy algorithmic theorem
rather than re-derive it. We also record the exact statements we confirmed for two recent problems so
the Lean models are faithful to a solution LeetCode actually accepts (not a strawman).

## Verified AFP citations (cite for the algorithmic-correctness fact, pair with a Lean scheme cert)

- **LC 1489 — Find Critical and Pseudo-Critical Edges in Minimum Spanning Tree** (scheme: relaxation).
  The accepted solution recomputes the minimum-spanning-tree weight (Kruskal + union-find). The MST
  optimality it relies on is formally verified:
  > Maximilian P. L. Haslbeck, Peter Lammich, Julian Biendarra.
  > "Kruskal's Algorithm for Minimum Spanning Forest." *Archive of Formal Proofs*, 14 Feb 2019.
  > https://www.isa-afp.org/entries/Kruskal.html
  > (Greedy-on-weighted-matroid correctness, instantiated to forests, refined to union-find code.)
  Lean cert plan: component = reachable set under the concrete edge relation (like FloodFill P0733).

- **LC 1192 — Critical Connections in a Network (bridges)** (scheme: relaxation).
  The accepted solution finds bridges via a DFS low-link traversal. The DFS-algorithm framework is
  formally verified:
  > Peter Lammich, René Neumann.
  > "A Framework for Verifying Depth-First Search Algorithms." *Archive of Formal Proofs*, 2016
  > (presented at CPP 2015, pp. 137–146). https://www.isa-afp.org/entries/DFS_Framework.html
  Lean cert plan: an edge is a bridge iff removing it makes its endpoints mutually unreachable —
  a reachability characterization in the concrete graph minus that edge.

## Confirmed problem statements (so the Lean models are faithful)

- **LC 3466 — Maximum Coin Collection** (scheme: dp). Source: https://leetcode.com/problems/maximum-coin-collection/
  Mario drives a multi-lane freeway, starting in lane 1, and may make **at most 2 lane switches**;
  maximize coins collected. Accepted DP: `dfs(i, j, k)` = max coins from position `i`, lane `j`, with
  `k` switches remaining. Lean cert plan: lane-DP achievability (the DP value is realized by a concrete
  driving strategy within the switch budget) — like the grid achievability in Triangle (P0120).

- **LC 3742 — Maximum Path Score in a Grid** (scheme: dp). Source: https://leetcode.com/problems/maximum-path-score-in-a-grid/
  m×n grid of values 0/1/2; from (0,0) to (m−1,n−1) moving only right or down; each cell adds its value
  to the score and costs 0 (value 0) or 1 (values 1,2); **maximize score with total cost ≤ k**, else
  −1. Lean cert plan: budget-constrained grid-DP achievability (the DP value is realized by a concrete
  right/down path whose cost stays within `k`).

## Note on counting

Lean certs (the eight scheme classifications) count in the manifest as usual. The two AFP citations
back the algorithmic-correctness facts we deliberately do not re-prove; they are recorded here and in
the paper, not in `certs.csv`. The headline rests on per-problem proofs (+ these two citations), not on
the family→scheme label rule.
