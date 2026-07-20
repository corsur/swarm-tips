import Lean

/-!
# The certification gate — mechanical genuineness

Replaces the hand-maintained `NOT_GENUINE` blacklist with checks computed from the **elaborated
environment** (the same objects the Lean kernel certified). A problem namespace `LC.PXXXX` passes
iff:

1. **`sol` exists** — the file designates one constant as the modelled solution.
2. **`cls` is about `sol`** — the *statement* (type) of `cls` mentions the constant `sol`, so the
   scheme-membership theorem classifies the solution itself, not a disconnected helper.
3. **`corr` is about `sol`** — same requirement for the problem-specific property.
4. **A concrete test vector exists** — at least one theorem named `vec*` whose statement mentions
   `sol`, is *closed* (no hypotheses, no quantifiers: a ground fact such as
   `sol [2,3,1,2,4,3] 7 = 2`), anchoring the model to the problem's published example. An abstract
   or placeholder `sol` cannot produce one.
5. **Standard axioms only** — the transitive axiom closure of `cls`, `corr`, and every `vec*` is
   contained in `{propext, Quot.sound, Classical.choice}`. This mechanically rejects `sorry`
   (`sorryAx`) and `native_decide` (`Lean.ofReduceBool`/`Lean.ofReduceNat`).

Output: `gate.csv` (one row per problem namespace) consumed by `build_manifest.py`. No allowlist,
no blocklist — every verdict is recomputed from the environment on each run.
-/

open Lean

/-- Transitive axiom closure of a declaration (iterative worklist; no recursion). -/
def axiomsOf (env : Environment) (roots : Array Name) : NameSet := Id.run do
  let mut visited : NameSet := {}
  let mut ax : NameSet := {}
  let mut stack : Array Name := roots
  while stack.size > 0 do
    let n := stack.back!
    stack := stack.pop
    if visited.contains n then
      continue
    visited := visited.insert n
    match env.find? n with
    | none => pure ()
    | some ci =>
      if ci matches .axiomInfo _ then
        ax := ax.insert n
      for d in ci.getUsedConstantsAsSet do
        if !visited.contains d then
          stack := stack.push d
  return ax

def standardAxioms : List Name := [``propext, ``Quot.sound, ``Classical.choice]

/-- A `vec` theorem must be a closed ground fact: no ∀-binders / hypotheses in its statement. -/
def isClosedStatement (t : Expr) : Bool := !(t.consumeMData.isForall || t.consumeMData.isArrow)

structure Verdict where
  num : String
  ns : Name
  pass : Bool
  reasons : List String

/-- `LC.P0209` → `209`. -/
def numOfProblemSeg (seg : String) : Option String := do
  guard (seg.startsWith "P")
  let digits := (seg.drop 1).toString
  guard (!digits.isEmpty && digits.all Char.isDigit)
  return toString digits.toNat!

def checkProblem (env : Environment) (ns : Name) (num : String)
    (decls : Array Name) : Verdict := Id.run do
  let mut reasons : List String := []
  let solName := ns ++ `sol
  let mentions (n : Name) : Bool :=
    match env.find? n with
    | some ci => ci.type.getUsedConstants.contains solName
    | none => false
  if env.find? solName |>.isNone then
    reasons := reasons ++ ["no sol"]
  for thm in [`cls, `corr] do
    match env.find? (ns ++ thm) with
    | none => reasons := reasons ++ [s!"no {thm}"]
    | some _ =>
      if !mentions (ns ++ thm) then
        reasons := reasons ++ [s!"{thm} statement does not reference sol"]
  let vecs := decls.filter fun d =>
    (d.componentsRev.head?.map fun c => c.toString.startsWith "vec").getD false
  let goodVecs := vecs.filter fun v =>
    match env.find? v with
    | some ci => ci.type.getUsedConstants.contains solName && isClosedStatement ci.type
    | none => false
  if goodVecs.isEmpty then
    reasons := reasons ++ ["no closed test vector about sol (vec*)"]
  let roots := #[ns ++ `cls, ns ++ `corr] ++ goodVecs
  let ax := axiomsOf env (roots.filter fun r => (env.find? r).isSome)
  let bad := ax.toList.filter fun a => !standardAxioms.contains a
  if !bad.isEmpty then
    reasons := reasons ++ [s!"nonstandard axioms: {bad}"]
  return { num, ns, pass := reasons.isEmpty, reasons }

def main : IO Unit := do
  initSearchPath (← findSysroot)
  let env ← importModules #[{ module := `Lproofs }] {}
  -- Group the user-facing declarations of each LC.PXXXX namespace.
  let mut byNs : Std.HashMap Name (Array Name) := {}
  for (n, _) in env.constants.toList do
    if n.isInternal then continue
    match n.components with
    | .str .anonymous "LC" :: seg :: _ =>
      match seg with
      | .str .anonymous s =>
        if (numOfProblemSeg s).isSome then
          let ns := (Name.mkSimple "LC") ++ Name.mkSimple s
          byNs := byNs.insert ns ((byNs.getD ns #[]).push n)
      | _ => pure ()
    | _ => pure ()
  let mut rows : Array Verdict := #[]
  for (ns, decls) in byNs.toList do
    let some seg := ns.componentsRev.head? | continue
    let some num := numOfProblemSeg seg.toString | continue
    rows := rows.push (checkProblem env ns num decls)
  let sorted := rows.qsort fun a b => a.num.toNat! < b.num.toNat!
  let mut out := "num,namespace,pass,reasons\n"
  for r in sorted do
    let reason := String.intercalate "; " r.reasons
    out := out ++ s!"{r.num},{r.ns},{if r.pass then "True" else "False"},\"{reason}\"\n"
  IO.FS.writeFile "gate.csv" out
  let passed := sorted.filter (·.pass) |>.size
  IO.println s!"gate: {passed}/{sorted.size} problem namespaces pass; wrote gate.csv"
