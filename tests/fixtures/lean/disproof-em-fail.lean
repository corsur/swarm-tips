theorem proof : statementProp ∨ ¬ statementProp :=
  Classical.em statementProp
