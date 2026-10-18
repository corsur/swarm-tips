theorem solution : statementProp := by
  intro a b c n d hpyth harea
  dsimp only
  calc
    (c * (a ^ 2 - b ^ 2)) ^ 2
        = c ^ 2 * ((a ^ 2 + b ^ 2) ^ 2 - 4 * (a * b) ^ 2) := by ring
    _ = c ^ 2 * ((c ^ 2) ^ 2 - 4 * (2 * n * d ^ 2) ^ 2) := by rw [hpyth, harea]
    _ = (c ^ 2) ^ 3 - (4 * n * d ^ 2) ^ 2 * (c ^ 2) := by ring
