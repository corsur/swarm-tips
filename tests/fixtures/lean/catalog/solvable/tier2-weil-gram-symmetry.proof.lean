theorem solution : statementProp := by
  unfold statementProp
  intro c hc i j N log_m
  have h : (i - j) * N * log_m = -((j - i) * N * log_m) := by ring
  rw [h, hc]
