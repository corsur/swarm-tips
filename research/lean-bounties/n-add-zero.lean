/-
Proof artifact for the Shillbot LeanProof bounty "Example: n + 0 = n"
(platform 10, campaign f1f8c32f-b589-4dd8-9b2e-3c105677b945).

CONTAINS THE THEOREM ONLY. The runner PREPENDS the campaign's statement before
compiling, so an artifact that also defines `statementProp` fails elaboration:

  error: `statementProp` has already been declared

That scores 0 and refunds the client — indistinguishable, from every signal
except the payment, from a proof that was simply wrong.

This file previously redeclared the statement AND asserted the opposite rule in
this very comment ("`statementProp` must appear verbatim — the checker asserts
the submitted source contains the campaign's statement"). That is not what the
runner does, and the wrong note propagated: add-comm.lean was written the same
way from it and cost a bounty cycle. Both artifacts are now theorem-only and
both were verified against the LIVE runner before commit rather than reasoned
about:

  POST /check {mode:"self_contained", statement_def:"statementProp",
               entry_theorem:"proof", max_build_secs:300, max_build_mem_mb:4096}
  -> {"axioms":[],"detail":"proof checked; axioms: []","verdict":"pass"}

`n + 0` reduces to `n` definitionally (Nat.add recurses on its second argument),
so `rfl` closes it under policy v1 (self-contained, no imports) and the axiom
set comes back empty.
-/

theorem proof : statementProp := fun _ => rfl
