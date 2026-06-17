import Lproofs.Generators
import Lproofs.Classify

/-!
# The four recursion-scheme patterns (the merge)

The twelve interview techniques merge into four recursion schemes covering ~80% of
frequency-weighted load. This file gives, for each, a defining scheme-predicate and
machine-checked instances.

**Pattern 1 (streaming fold, ~48%)** is the punchline: techniques that are memorized as
*unrelated* — prefix-sum, XOR (bit tricks), linked-list reversal, running statistics,
hashing's seen-set — are all `List.foldl`, i.e. one pass with a carried accumulator.

(Patterns 2–4: recursive decomposition / DP, graph relaxation, bisection — defined here and in
`Classify.lean`/`Generators.lean`.)
-/

namespace Interview.Patterns
open Interview

/-! ## Pattern 1 — streaming fold / single pass with carried state -/

/-- A function is a **streaming fold** if it equals a left fold over its input: one pass with
    some step and initial accumulator (the accumulator's *type* is what varies across
    techniques — scalar, group element, list, statistic, finite set). -/
def IsFold {α β : Type*} (f : List α → β) : Prop :=
  ∃ (step : β → α → β) (init : β), ∀ xs, f xs = xs.foldl step init

/-- A function is a **right fold** (the canonical list catamorphism) if it equals `List.foldr` for
    some combiner and base — one structural pass consuming the list head-first, each step combining
    the element with the result of the rest. The grouping/run-length family (Summary Ranges, String
    Compression, Count and Say) inhabits this scheme: each step inspects only the head run of the
    accumulated result. -/
def IsRightFold {α β : Type*} (f : List α → β) : Prop :=
  ∃ (step : α → β → β) (init : β), ∀ xs, f xs = xs.foldr step init

-- prefix-sum / running aggregate (accumulator: a scalar)
theorem fold_prefixSum {α : Type*} [AddMonoid α] :
    IsFold (fun xs : List α => xs.foldl (· + ·) 0) := ⟨(· + ·), 0, fun _ => rfl⟩

-- bit technique: XOR reduce, e.g. Single Number (accumulator: a group element)
theorem fold_xor : IsFold (fun xs : List ℕ => xs.foldl (· ^^^ ·) 0) :=
  ⟨(· ^^^ ·), 0, fun _ => rfl⟩

-- running statistic, e.g. max-so-far (accumulator: a scalar)
theorem fold_runningMax : IsFold (fun xs : List ℕ => xs.foldl max 0) := ⟨max, 0, fun _ => rfl⟩

-- hashing's seen-set is a fold building a Finset (accumulator: a finite set)
theorem fold_seenSet :
    IsFold (fun xs : List ℕ => xs.foldl (fun s x => insert x s) (∅ : Finset ℕ)) :=
  ⟨fun s x => insert x s, ∅, fun _ => rfl⟩

-- linked-list reversal is a fold (flip cons) — the non-trivial instance: `reverse` is defined
-- separately, yet *equals* a left fold.
theorem foldl_cons_eq {α : Type*} (xs acc : List α) :
    xs.foldl (fun a x => x :: a) acc = xs.reverse ++ acc := by
  induction xs generalizing acc with
  | nil => simp
  | cons y ys ih => rw [List.foldl_cons, ih, List.reverse_cons, List.append_assoc]; rfl

theorem fold_reverse {α : Type*} : IsFold (List.reverse : List α → List α) :=
  ⟨fun a x => x :: a, [], fun xs => by rw [foldl_cons_eq xs [], List.append_nil]⟩

/-- The hashing accumulator computes the underlying finite set (correctness of the seen-set). -/
theorem seenSet_eq_toFinset (xs : List ℕ) :
    xs.foldl (fun s x => insert x s) (∅ : Finset ℕ) = xs.toFinset := by
  have h : ∀ (init : Finset ℕ), xs.foldl (fun s x => insert x s) init = init ∪ xs.toFinset := by
    intro init
    induction xs generalizing init with
    | nil => simp
    | cons y ys ih =>
      rw [List.foldl_cons, ih, List.toFinset_cons]
      ext a; simp only [Finset.mem_union, Finset.mem_insert]; tauto
  simpa using h ∅

/-! ## Pattern 4 — bisection / divide-and-conquer (monotone-predicate search)

When feasibility is monotone, the feasible set is a *threshold* (an up-set), so it is decided
by comparison to one boundary point — which binary search finds. Instances: Koko Eating
Bananas, Capacity to Ship, Split Array Largest Sum, Sqrt. -/

/-- The bisection answer (least feasible value) is exactly `Nat.find`, and is the least element
    of the feasible set. (Scheme + uniform correctness; holds for any decidable feasible set.) -/
theorem bisection_isLeast (p : ℕ → Prop) [DecidablePred p] (h : ∃ n, p n) :
    IsLeast {n | p n} (Nat.find h) :=
  ⟨Nat.find_spec h, fun _ hm => Nat.find_min' h hm⟩

/-- **The defining property of bisection:** a *monotone* feasible predicate is a threshold —
    `p n ↔ (least feasible) ≤ n`. This up-set structure is exactly what makes binary search
    (halving on the decision boundary) correct. -/
theorem bisection_threshold (p : ℕ → Prop) [DecidablePred p]
    (mono : ∀ a b, a ≤ b → p a → p b) (h : ∃ n, p n) (n : ℕ) :
    p n ↔ Nat.find h ≤ n :=
  ⟨fun hn => Nat.find_min' h hn, fun hle => mono _ _ hle (Nat.find_spec h)⟩

-- A concrete monotone (hence bisectable) feasibility: integer sqrt, "is `k` big enough that
-- `k*k ≥ x`". Monotone in `k`, so `Nat.find` is the ceil-sqrt and the predicate is a threshold.
def sqrtFeasible (x k : ℕ) : Prop := x ≤ k * k
instance (x : ℕ) : DecidablePred (sqrtFeasible x) := fun k => inferInstanceAs (Decidable (x ≤ k * k))

theorem sqrtFeasible_mono (x : ℕ) : ∀ a b, a ≤ b → sqrtFeasible x a → sqrtFeasible x b :=
  fun _ _ hab ha => le_trans ha (Nat.mul_le_mul hab hab)

/-! ## Pattern 2 — recursive decomposition / DP (catamorphism), more instances -/

/-- A binary tree (the structure folds happen over). -/
inductive Tree (α : Type*) where
  | leaf : Tree α
  | node : Tree α → α → Tree α → Tree α

/-- The defining fold over a tree (its catamorphism). -/
def Tree.fold {α β : Type*} (lf : β) (nd : β → α → β → β) : Tree α → β
  | .leaf => lf
  | .node l v r => nd (l.fold lf nd) v (r.fold lf nd)

/-- Maximum depth (LC 104). -/
def Tree.depth {α : Type*} : Tree α → ℕ
  | .leaf => 0
  | .node l _ r => 1 + max l.depth r.depth

/-- CLASSIFICATION: Max Depth *is* a tree catamorphism (a fold over the tree). -/
theorem depth_isCata {α : Type*} :
    (Tree.depth : Tree α → ℕ) = Tree.fold 0 (fun dl _ dr => 1 + max dl dr) := by
  funext t; induction t with
  | leaf => rfl
  | node l v r ihl ihr => simp [Tree.depth, Tree.fold, ihl, ihr]

/-- Subsets (LC 78): the backtracking enumeration as a recursive fold. -/
def subsets {α : Type*} : List α → List (List α)
  | [] => [[]]
  | x :: xs => let s := subsets xs; s ++ s.map (x :: ·)

/-- CLASSIFICATION: the enumeration produces exactly `2^n` subsets — the powerset count, the
    hallmark of the backtracking/recursive-decomposition scheme. -/
theorem subsets_card {α : Type*} (xs : List α) : (subsets xs).length = 2 ^ xs.length := by
  induction xs with
  | nil => rfl
  | cons x xs ih =>
    simp only [subsets, List.length_append, List.length_map, ih, List.length_cons, pow_succ]
    ring

/-! ## Pattern 1 — more fold archetypes (one per fold family, for the ~82% coverage)

Each family the coverage analysis counts under the streaming-fold scheme gets a representative
`IsFold` certificate here. Together with `fold_prefixSum`, `fold_seenSet` (hashing),
`fold_reverse` (linked-list) and `fold_xor` (bit) above, this covers the fold families:
two-pointers, sliding-window, stack, intervals, string-other. -/

/-! ### stack — Valid Parentheses (LC 20): a bracket scan with a stack accumulator. -/

/-- The closing bracket expected for an opening bracket (`none` if `c` is not an opener). -/
def closerOf (c : Char) : Option Char :=
  if c = '(' then some ')' else if c = '[' then some ']'
  else if c = '{' then some '}' else none

/-- One bracket-scan step: push the expected closer on an opener; on any other char pop and
    check it matches; `none` is the stuck/invalid state. -/
def parenStep : Option (List Char) → Char → Option (List Char)
  | none, _ => none
  | some stack, c =>
    match closerOf c with
    | some cl => some (cl :: stack)
    | none =>
      match stack with
      | top :: rest => if c = top then some rest else none
      | [] => none

/-- Valid Parentheses: balanced iff the scan ends with an empty stack. -/
def parenMatch (s : List Char) : Bool :=
  match s.foldl parenStep (some []) with
  | some [] => true
  | _ => false

/-- CLASSIFICATION (stack family): the bracket scan is a streaming fold; the accumulator is a
    stack of expected closers. -/
theorem fold_parenScan : IsFold (fun s : List Char => s.foldl parenStep (some [])) :=
  ⟨parenStep, some [], fun _ => rfl⟩

example : parenMatch ['(', ')', '[', ']'] = true := by decide
example : parenMatch ['(', '[', ']', ')'] = true := by decide
example : parenMatch ['(', ']'] = false := by decide
example : parenMatch ['(', '('] = false := by decide

/-! ### two-pointers — Valid Palindrome (LC 125): the converging two-pointer test decides
`s = s.reverse`, and `reverse` is a streaming fold (`fold_reverse`). -/

/-- The two-pointer palindrome test (decide whether the sequence reads the same reversed). -/
def isPalindrome {α : Type*} [DecidableEq α] (s : List α) : Bool := decide (s = s.reverse)

/-- CLASSIFICATION (two-pointers family): the palindrome test is exactly equality with the
    fold-computed reverse — a streaming fold downstream. -/
theorem palindrome_via_fold {α : Type*} [DecidableEq α] (s : List α) :
    isPalindrome s = true ↔ s = s.reverse := by simp [isPalindrome]

example : isPalindrome ['a', 'b', 'a'] = true := by decide
example : isPalindrome ['a', 'b'] = false := by decide

/-! ### sliding-window — a fixed window of size `k` is maintained by a single left pass, the
window (last ≤ `k` elements) carried as the accumulator. -/

/-- One step: append the new element and drop the oldest if the window now exceeds `k`. -/
def windowStep (k : ℕ) (win : List ℕ) (x : ℕ) : List ℕ :=
  let w := win ++ [x]
  if w.length ≤ k then w else w.tail

/-- The sliding window after consuming `xs` (the last ≤ `k` elements). -/
def slidingWindow (k : ℕ) (xs : List ℕ) : List ℕ := xs.foldl (windowStep k) []

/-- CLASSIFICATION (sliding-window family): maintaining the window is a streaming fold. -/
theorem fold_slidingWindow (k : ℕ) : IsFold (slidingWindow k) :=
  ⟨windowStep k, [], fun _ => rfl⟩

example : slidingWindow 2 [1, 2, 3] = [2, 3] := by decide

/-! ### intervals — Merge Intervals (LC 56): on intervals pre-sorted by start, the merge is a
left fold accumulating merged intervals (the sort is the divide-and-conquer precondition). -/

/-- One merge step: extend the most recent interval if the next overlaps it, else start a new one
    (accumulated head-first). -/
def mergeStep (acc : List (ℕ × ℕ)) (iv : ℕ × ℕ) : List (ℕ × ℕ) :=
  match acc with
  | [] => [iv]
  | (lo, hi) :: rest => if iv.1 ≤ hi then (lo, max hi iv.2) :: rest else iv :: (lo, hi) :: rest

/-- Merge overlapping intervals (input assumed sorted by start). -/
def mergeIntervals (ivs : List (ℕ × ℕ)) : List (ℕ × ℕ) := (ivs.foldl mergeStep []).reverse

/-- CLASSIFICATION (intervals family): the post-sort merge sweep is a streaming fold. -/
theorem fold_mergeIntervals :
    IsFold (fun ivs : List (ℕ × ℕ) => ivs.foldl mergeStep []) :=
  ⟨mergeStep, [], fun _ => rfl⟩

example : mergeIntervals [(1, 3), (2, 6), (8, 10)] = [(1, 6), (8, 10)] := by decide

/-! ### string-other — Valid Anagram (LC 242): the character-frequency count is a streaming fold
building a `Multiset`; two strings are anagrams iff these folds (their multisets) are equal. -/

/-- Character-frequency fold (accumulator: a multiset of characters). -/
def charCount {α : Type*} (s : List α) : Multiset α := s.foldl (fun m c => insert c m) 0

/-- CLASSIFICATION (string-other family): the character count is a streaming fold. -/
theorem fold_charCount {α : Type*} :
    IsFold (fun s : List α => s.foldl (fun m c => insert c m) (0 : Multiset α)) :=
  ⟨fun m c => insert c m, 0, fun _ => rfl⟩

/-- The count fold builds exactly the multiset of the list (order-independent correctness). -/
theorem charCount_eq {α : Type*} (s : List α) : charCount s = (s : Multiset α) := by
  have h : ∀ init : Multiset α, s.foldl (fun m c => insert c m) init = ↑s + init := by
    intro init
    induction s generalizing init with
    | nil => simp
    | cons x xs ih =>
      rw [List.foldl_cons, ih, Multiset.insert_eq_cons, Multiset.add_cons, ← Multiset.cons_coe,
        Multiset.cons_add]
  simpa using h 0

/-- Anagram ⟺ equal character multisets (⟺ permutation), via the count fold. -/
theorem anagram_via_fold {α : Type*} (s t : List α) :
    charCount s = charCount t ↔ (s : Multiset α) = ↑t := by
  rw [charCount_eq, charCount_eq]

end Interview.Patterns
