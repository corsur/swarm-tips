/-
Proof artifact for the Shillbot LeanProof bounty "Example: n + 0 = n"
(platform 10, campaign f1f8c32f-b589-4dd8-9b2e-3c105677b945).

The attester fetches this file over HTTPS and hands it to the zero-credential
lean-runner under policy v1: self-contained (NO imports), toolchain
leanprover/lean4:v4.31.0, `statement_def = statementProp`,
`entry_theorem = proof`, axioms limited to propext / Classical.choice /
Quot.sound.

`statementProp` must appear verbatim — the checker asserts the submitted source
contains the campaign's statement before it will build. `n + 0` reduces to `n`
definitionally (Nat.add recurses on its second argument), so `rfl` closes it and
no axiom beyond the allowlist is used.
-/

def statementProp : Prop := ∀ n : Nat, n + 0 = n

theorem proof : statementProp := fun _ => rfl
