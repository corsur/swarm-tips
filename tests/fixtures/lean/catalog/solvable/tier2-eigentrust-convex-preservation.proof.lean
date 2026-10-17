theorem solution : statementProp := by
  unfold statementProp
  intro α x1 x2 x3 p1 p2 p3 hx hp
  calc
    ((1 - α) * x1 + α * p1) + ((1 - α) * x2 + α * p2) + ((1 - α) * x3 + α * p3)
      = (1 - α) * (x1 + x2 + x3) + α * (p1 + p2 + p3) := by ring
    _ = (1 - α) * 1 + α * 1 := by rw [hx, hp]
    _ = 1 := by ring
