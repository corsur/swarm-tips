import Lproofs.Schemes.Fold

/-! @lc 43 | name:Multiply Strings | scheme:fold | family:string-other | complexity:O(mn) |
    source:https://leetcode.com/problems/multiply-strings/

    Multiply two non-negative integers given as digit strings. The grade-school algorithm is a
    digit convolution: each digit of one factor scales the other and is shifted. Correctness: the
    numeric value of the convolution is exactly the product of the two values. (Carry normalization
    to 0–9 digits is a value-preserving presentation step, layered on top of this.) -/

namespace LC.P0043
open Interview.Patterns

/-- Little-endian digit value: `Σ dᵢ · 10ⁱ`. -/
def toNat : List ℕ → ℕ
  | [] => 0
  | d :: ds => d + 10 * toNat ds

/-- Digit-wise addition (no carry). -/
def addLists : List ℕ → List ℕ → List ℕ
  | [], b => b
  | a, [] => a
  | x :: a, y :: b => (x + y) :: addLists a b

/-- Grade-school multiplication as a digit convolution (no carry). -/
def conv : List ℕ → List ℕ → List ℕ
  | [], _ => []
  | x :: a, b => addLists (b.map (x * ·)) (0 :: conv a b)

def sol (a b : List ℕ) : List ℕ := conv a b

/-- SCHEME (fold): for a fixed second factor `b`, the convolution `conv · b` is a genuine right
    fold over the first factor's digits — each digit contributes one shifted partial product,
    accumulated. This is exactly `sol`'s computation. -/
theorem cls (b : List ℕ) : IsRightFold (fun a => conv a b) :=
  ⟨fun x acc => addLists (b.map (x * ·)) (0 :: acc), [], by
    intro a
    induction a with
    | nil => rfl
    | cons x a ih => rw [List.foldr_cons, ← ih]; rfl⟩

theorem toNat_addLists : ∀ (a b : List ℕ), toNat (addLists a b) = toNat a + toNat b := by
  intro a
  induction a with
  | nil => intro b; simp [addLists, toNat]
  | cons x a ih =>
    intro b
    cases b with
    | nil => simp [addLists, toNat]
    | cons y b => simp only [addLists, toNat, ih b]; ring

theorem toNat_smul (x : ℕ) : ∀ (b : List ℕ), toNat (b.map (x * ·)) = x * toNat b := by
  intro b
  induction b with
  | nil => simp [toNat]
  | cons y b ih => simp only [List.map_cons, toNat, ih]; ring

/-- CORRECT: the convolution's value is the product of the two factors' values. -/
theorem corr : ∀ (a b : List ℕ), toNat (conv a b) = toNat a * toNat b := by
  intro a
  induction a with
  | nil => intro b; simp [conv, toNat]
  | cons x a ih =>
    intro b
    rw [conv, toNat_addLists, toNat_smul, toNat, ih b, toNat]
    ring

end LC.P0043
