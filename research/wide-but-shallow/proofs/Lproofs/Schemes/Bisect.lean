import Lproofs.Patterns

/-!
# Scheme 4 — bisection: verified combinators

A monotone feasibility predicate is a **threshold** (an up-set), so it is decided by comparison
to one boundary point — which binary search finds. The boundary is `Nat.find`. The two reusable
facts (proven in `Interview.Patterns`) are re-exported: `bisection_threshold` (membership ⟺ the
answer is ≤ n — the scheme witness) and `bisection_isLeast` (the answer is the least feasible
value — correctness).
-/

namespace Interview.Schemes

export Interview.Patterns (bisection_threshold bisection_isLeast)

end Interview.Schemes
