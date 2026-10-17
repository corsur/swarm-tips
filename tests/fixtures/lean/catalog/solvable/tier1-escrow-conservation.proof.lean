theorem solution : statementProp := by
  unfold statementProp
  intro E s hs
  dsimp
  have h1 : E * s ≤ E * 10000 := Nat.mul_le_mul_left E hs
  have h2 : (E * s) / 10000 ≤ E := Nat.div_le_of_le_mul (by omega)
  omega
