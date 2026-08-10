/-
Proof artifact for the Shillbot LeanProof bounty "Example: a + b = b + a"
(platform 10, campaign 327d9611-2c7d-4a5e-ab8d-bc9063f7f802).

Policy v1: self-contained (NO imports), leanprover/lean4:v4.31.0,
`statement_def = statementProp`, `entry_theorem = proof`, axioms limited to
propext / Classical.choice / Quot.sound.

Unlike `n + 0 = n`, commutativity is NOT definitional — `a + b` recurses on `b`,
so `rfl` cannot close it. Nat.add_comm is in core (not Mathlib), which keeps this
inside the self-contained policy: no import is required to name it.
-/

def statementProp : Prop := ∀ a b : Nat, a + b = b + a

theorem proof : statementProp := fun a b => Nat.add_comm a b
