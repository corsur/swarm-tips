# Devnet Gram-matrix research ladder

Status: **unfunded templates, not published campaigns**. No campaign or module IDs
exist for these entries. Publication is gated on the production-devnet
two-generation reuse acceptance test.

## Pinned-library audit

Reviewed Zeta23 commit `3635e74826a4c1fcece7d1cd2b6fa75e43a00510`.

- `Zeta23.Defs`: `Params`, `Params.Valid`, `Params.lam`, `Params.fk`,
  `ZeroConfig.Gsummand`, `ZeroConfig.Gz`, `Params.Gentry`, and `Params.Gp`
  already specify the taper family and finite index set `Fin (P.d T)`.
- `Params.tilde` is division by `L(T)`. `Params.hat` is division by
  `a(T)L(T)^2`. These are different normalizations and must remain distinct.
- `Zeta23.Hypotheses.ExplicitFormulaPaper` explicitly supplies summability,
  integrability, support, differentiability, and the conjugation convention.
  An adapter must retain these hypotheses rather than relying on Lean's
  default value for a divergent sum or integral.
- `Zeta23.ZeroSide.hatP_posSemidef` and `hatQ_isHermitian` concern pieces of a
  decomposition; neither establishes unconditional positivity of the whole
  Weil form. `ZeroSide.Final` already exposes usable structural wrappers.
- Mathlib's `Matrix.trace_mul_cycle` and `trace_mul_cycle'` already provide
  cyclic permutation identities. The requested entrywise cubic/quartic sums
  are separate API targets.
- `Zeta23.ZeroSide.RankTraceMult` already develops spectral transfer and
  conditional rank estimates. Reuse these before creating replacement tasks.
- `Zeta23.XiPrime.QuarticWindow` and `XiPrime.PrimeSide.Moments` concern other
  specific objects. Their names alone do not identify them with powers of the
  selected Weil matrix.

## Dependency templates

| Label | Formal obligation | Prerequisites |
| --- | --- | --- |
| weil-lambda-one-adapter | Define raw/hat matrices from the pinned `Gz`/`Gp` family, require `P.lam = 1`, and prove the exact normalization and explicit-formula bridge | Pinned environment; adapter target review |
| weil-structural | Indexing, Hermitian symmetry and scaling under explicit hypotheses; positivity only with sufficient stated assumptions | Promoted adapter |
| matrix-cubic | Generic entrywise trace-of-cube identity, then a separate specialization to the adapter | Generic target below has no module dependencies; specialization requires adapter |
| matrix-quartic | Generic entrywise trace-of-fourth-power identity, independently of the cubic proof | Generic target below has no module dependencies; specialization requires adapter |
| weil-cubic-arithmetic | Expand selected cubic formula into weighted prime-power sums with all additional terms | Adapter, structural facts, cubic identity |
| weil-quartic-arithmetic | Expand selected quartic formula into weighted prime-power sums with all additional terms | Adapter, structural facts, quartic identity |
| correlation-interfaces | Quantifiers, support, constants, normalization, diagonal/error terms and limiting hypotheses | Arithmetic expansions |
| conditional-consequences | Trace/spectral/rank/zero-multiplicity consequences of named correlation hypotheses | Promoted interfaces |
| open-bound / counterexample | Separate exact proposition and its formal negation | Fixed interfaces and consequences |

The generic challenge files define immutable candidate statements but are not
published registry modules. Their final campaign commitments will bind their
exact bytes, environment and dependency bundle. Budget assignment, campaign IDs,
and acceptance examples remain to be recorded after the rollout gates pass.

Each result is **kernel checked, not mathematical peer review**. No entry claims
that an open higher-correlation estimate has been established.
