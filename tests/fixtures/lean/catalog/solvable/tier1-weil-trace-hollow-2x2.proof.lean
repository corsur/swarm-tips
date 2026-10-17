theorem solution : statementProp := by
  unfold statementProp
  intro a
  exact ⟨by ring, add_nonneg (mul_self_nonneg a) (mul_self_nonneg a)⟩
