# Editorial verification — do the modeled solutions match accepted approaches?

Web pass (2026-06-22). For every in-scheme problem in the pre-registered sample (seed 20260619), we
checked the published editorial / standard accepted approach (LeetCode editorial, algo.monster,
neetcode, leetcode.ca, doocs/leetcode) and asked: is the canonical accepted solution actually the
recursion scheme we assigned? This does not run the judge — it confirms the scheme assignment against
documented accepted approaches, which is the load-bearing fact for the classification.

**Result: 69 clean matches, 3 flagged, 0 unclear. Two flags are re-labelings *within* the four; the
third (258 Add Digits) is reclassified to the TAIL (its optimal solution is O(1) arithmetic), giving 71
in-scheme. None of the remaining 71 falls outside the four.**

## The 3 flagged problems

- **270 Closest Binary Search Tree Value** — assigned `dp`, editorial-canonical is the **binary
  descent** down the BST (compare, go left/right): `bisection`. Both a recursive traversal (dp) and
  the guided descent (bisection) appear in editorials; either is one of the four. Re-labeled to
  bisection.
- **736 Parse Lisp Expression** — assigned `fold`, editorial is **recursive evaluation with scoped
  environments**: a catamorphism over the parse tree = `dp`. Our Lean model already evaluates the
  expression tree recursively, so the proof is unchanged; only the scheme tag moves fold → dp.
- **258 Add Digits** — assigned `fold`, **RECLASSIFIED TO TAIL (2026-06-24)**. The *optimal* accepted
  solution is the O(1) digital-root formula `(n-1)%9+1`, a math identity outside the four; the repeated
  digit-sum fold passes too, but since the canonical/optimal solution is not one of the four we move it
  to the tail (the strict reading). This lowers the count 72 -> 71. See `build_manifest.py` tail_override.

## Effect on the per-scheme split

In-scheme total is 71 (after moving 258 to tail). After re-labeling: fold 27, dp 25, relaxation 11, bisection 8.

## Full verdict table (72 in-scheme problems)

| # | name | assigned | verdict | canonical approach |
|---|------|----------|---------|--------------------|
| 7 | Reverse Integer | fold | MATCH | one-pass digit math |
| 20 | Valid Parentheses | fold | MATCH | single scan with stack |
| 71 | Simplify Path | fold | MATCH | scan components into stack |
| 74 | Search a 2D Matrix | bisection | MATCH | binary search flattened matrix |
| 88 | Merge Sorted Array | fold | MATCH | two-pointer merge from end |
| 94 | Binary Tree Inorder | dp | MATCH | recursive tree traversal |
| 101 | Symmetric Tree | dp | MATCH | recursive mirror compare |
| 104 | Maximum Depth | dp | MATCH | recursive tree depth |
| 109 | Sorted List to BST | dp | MATCH | divide-and-conquer build |
| 111 | Minimum Depth | dp | MATCH | recursive tree min-depth |
| 113 | Path Sum II | dp | MATCH | backtracking root-to-leaf DFS |
| 120 | Triangle | dp | MATCH | bottom-up DP recurrence |
| 139 | Word Break | dp | MATCH | DP / memoized recursion |
| 142 | Linked List Cycle II | fold | MATCH | Floyd two-pointer scan |
| 186 | Reverse Words II | fold | MATCH | in-place reverse scan |
| 188 | Best Time Buy/Sell IV | dp | MATCH | DP over transaction states |
| 208 | Implement Trie | dp | MATCH | trie construction/traversal |
| 209 | Min Size Subarray Sum | fold | MATCH | sliding window one-pass |
| 210 | Course Schedule II | relaxation | MATCH | topological sort |
| 226 | Invert Binary Tree | dp | MATCH | tree recursion |
| 229 | Majority Element II | fold | MATCH | Boyer-Moore voting |
| 258 | Add Digits | fold→TAIL | RECLASS | digit-sum fold; optimal is O(1) math |
| 269 | Alien Dictionary | relaxation | MATCH | topological sort (Kahn) |
| 270 | Closest BST Value | dp→bisection | RELABEL | binary descent of BST |
| 278 | First Bad Version | bisection | MATCH | binary search on versions |
| 314 | Vertical Order Traversal | relaxation | MATCH | BFS with column buckets |
| 329 | Longest Increasing Path | dp | MATCH | DFS on grid with memo |
| 378 | Kth Smallest in Matrix | bisection | MATCH | binary search on value+count |
| 505 | The Maze II | relaxation | MATCH | Dijkstra / BFS shortest path |
| 516 | Longest Palindromic Subseq | dp | MATCH | interval DP |
| 536 | Construct BT from String | dp | MATCH | recursive parse subtrees |
| 541 | Reverse String II | fold | MATCH | single string scan |
| 543 | Diameter of Binary Tree | dp | MATCH | tree recursion (post-order) |
| 611 | Valid Triangle Number | fold | MATCH | sort + two-pointer count |
| 647 | Palindromic Substrings | fold | MATCH | expand-around-center scan |
| 719 | K-th Smallest Pair Distance | bisection | MATCH | binary search on distance |
| 736 | Parse Lisp Expression | fold→dp | RELABEL | recursive eval with scopes |
| 759 | Employee Free Time | fold | MATCH | sort intervals, scan gaps |
| 815 | Bus Routes | relaxation | MATCH | BFS over route graph |
| 939 | Minimum Area Rectangle | fold | MATCH | hashset + pair scan |
| 986 | Interval List Intersections | fold | MATCH | two pointers over intervals |
| 996 | Number of Squareful Arrays | dp | MATCH | backtracking permutations |
| 1166 | Design File System | dp | MATCH | trie/hashmap path insert |
| 1192 | Critical Connections | relaxation | MATCH | Tarjan bridge DFS |
| 1239 | Max Length Concat Unique | dp | MATCH | bitmask backtracking/DP |
| 1268 | Search Suggestions System | dp | MATCH | trie (or sort+binsearch) |
| 1385 | Distance Value Between Arrays | bisection | MATCH | sort + binary search |
| 1387 | Sort by Power Value | fold | MATCH | per-int power walk to 1 |
| 1438 | Longest Subarray Abs Diff | fold | MATCH | sliding window + deques |
| 1489 | MST Critical Edges | relaxation | MATCH | Kruskal MST + union-find |
| 1492 | The kth Factor of n | fold | MATCH | O(√n) divisor scan |
| 1614 | Max Nesting Depth Parens | fold | MATCH | one-pass paren counter |
| 1657 | Two Strings Are Close | fold | MATCH | frequency-count + compare |
| 1674 | Min Moves Complementary | fold | MATCH | difference-array sweep |
| 1823 | Find the Winner (Josephus) | fold | MATCH | Josephus recurrence fold |
| 1868 | Product of Two RLE Arrays | fold | MATCH | two-pointer RLE merge |
| 1915 | Number of Wonderful Substrings | fold | MATCH | prefix-XOR bitmask + hash |
| 2149 | Rearrange Array by Sign | fold | MATCH | two-pointer placement |
| 2423 | Remove Letter Equalize Freq | fold | MATCH | char-frequency counting |
| 2493 | Divide Nodes Max Groups | relaxation | MATCH | BFS bipartite coloring |
| 2791 | Palindrome Paths in Tree | dp | MATCH | tree DFS XOR-parity mask |
| 2858 | Min Edge Reversals | relaxation | MATCH | re-rooting tree DP / DFS |
| 2861 | Maximum Number of Alloys | bisection | MATCH | binary search on count |
| 2912 | Ways to Reach Destination | dp | MATCH | combinatorial DP |
| 2948 | Lex Smallest by Swapping | relaxation | MATCH | connected components (union-find) |
| 2965 | Find Missing and Repeated | fold | MATCH | one-pass sum/frequency scan |
| 3043 | Longest Common Prefix | dp | MATCH | trie / hashed-prefix set |
| 3371 | Largest Outlier | fold | MATCH | total-sum + frequency scan |
| 3453 | Separate Squares I | bisection | MATCH | binary search on threshold |
| 3466 | Maximum Coin Collection | dp | MATCH | memoized DFS lane/switches |
| 3629 | Min Jumps Prime Teleport | relaxation | MATCH | BFS shortest path |
| 3742 | Maximum Path Score in Grid | dp | MATCH | 3D grid DP over budget |

**Tally: 69 MATCH · 2 RELABEL (within the four) · 1 RECLASSIFIED-TO-TAIL (258) · 0 UNCLEAR.**
71 sampled in-scheme problems have a documented accepted solution that is one of the four ideas; 258 is
moved to the tail because its optimal accepted solution is not.
