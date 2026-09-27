# SwarmProofs

`SwarmProofs` is the public, content-addressed Lean library produced by
successful Shillbot LeanProof tasks. A module being present means its exact
formal theorem passed the recorded Lean and axiom checks. It does **not** mean
the informal interpretation has received mathematical peer review.

Generated modules live at `SwarmProofs.Generated.H<sha256>` and are never
imported by the package root. Import the exact module recorded in a task's
dependency bundle. Registry history is immutable; deprecated and revoked
modules remain auditable but cannot be selected for new campaigns.

The first environment pins Zeta23 and its exact Lean/Mathlib toolchain. See
`environment-lock.json`, `NOTICE`, the SPDX `SBOM.spdx.json`, and the
machine-readable `registry.json`.

Administrative deprecation, revocation, and replacement are reviewed registry
commits. They never delete historical source or finalized attestations.
Revoked modules are rejected for unfinished verification; deprecated modules
remain reproducible but are hidden and cannot be selected by new campaigns.
