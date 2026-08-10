/-
Proof artifact for the Shillbot LeanProof bounty "Example: a + b = b + a"
(platform 10, campaign 327d9611-2c7d-4a5e-ab8d-bc9063f7f802).

CONTAINS THE THEOREM ONLY. The runner PREPENDS the campaign's statement before
compiling, so redeclaring `statementProp` here fails elaboration with
"`statementProp` has already been declared" — which scores 0 and refunds the
client, exactly as a wrong proof should. That is what the first version of this
file did.

Verified against the live runner before shipping:
  POST /check {mode:"self_contained", statement_def:"statementProp",
               entry_theorem:"proof"}
  -> {"axioms":[],"detail":"proof checked; axioms: []","verdict":"pass"}

Commutativity is not definitional (`a + b` recurses on `b`), so `rfl` cannot
close it; Nat.add_comm is core, not Mathlib, so no import is needed and the
axiom set comes back empty.
-/

theorem proof : statementProp := fun a b => Nat.add_comm a b
