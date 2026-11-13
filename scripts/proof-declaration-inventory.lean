/- SPDX-License-Identifier: Apache-2.0 -/
import Lean

open Lean

/-- Audit recursive logical dependencies, including opaque theorem bodies. -/
def auditDependencies (env : Environment) (roots : Array Name) : IO Unit := do
  let mut pending := roots.toList
  let mut visited : NameHashSet := {}
  while !pending.isEmpty do
    let name := pending.head!
    pending := pending.tail!
    if visited.contains name then continue
    visited := visited.insert name
    let some info := env.find? name | throw <| IO.userError s!"missing dependency: {name}"
    if info.isUnsafe || info.isPartial then
      throw <| IO.userError s!"unsafe/partial dependency: {name}"
    if let .axiomInfo _ := info then
      unless #[`propext, `Classical.choice, `Quot.sound].contains name do
        throw <| IO.userError s!"unapproved axiom: {name}"
    pending := info.type.getUsedConstants.toList ++ pending
    if let some value := info.value? (allowOpaque := true) then
      pending := value.getUsedConstants.toList ++ pending

structure ModuleExpectation where
  namespaceName : Name
  entryTheorem : Option Name := none
  isDisproof : Bool := false

def hasSubstr (s pat : String) : Bool :=
  (s.splitOn pat).length > 1

def isNegatedTargetType (ty : Expr) (targetProp : Name) : Bool :=
  let targetExpr := Expr.const targetProp []
  let notTarget := mkApp (Expr.const ``Not []) targetExpr
  ty.getUsedConstants.contains targetProp &&
    (ty == notTarget ||
      match ty with
      | .forallE _ d b .default => d == targetExpr && b == .const ``False []
      | _ => false)

/-- Inventory by defining module, not public name or declared entry theorem.
This deliberately includes private constants and generated helpers. -/
def main (args : List String) : IO Unit := do
  let moduleText :: extra := args | throw <| IO.userError "expected audit module and optional fixture namespace"
  if extra.length > 1 then throw <| IO.userError "too many audit arguments"
  let registry ← match Json.parse (← IO.FS.readFile "registry.json") with
    | .ok value => pure value
    | .error error => throw <| IO.userError error
  let modules ← match registry.getObjValAs? (Array Json) "modules" with
    | .ok value => pure value
    | .error error => throw <| IO.userError error
  let mut expectations : NameMap ModuleExpectation := {}
  for entry in modules do
    let .ok owner := entry.getObjValAs? String "module_name" | throw <| IO.userError "missing module name"
    let .ok theoremName := entry.getObjValAs? String "theorem_name" | throw <| IO.userError "missing theorem name"
    let signature := (entry.getObjValAs? String "theorem_signature").toOption.getD ""
    let outcome := (entry.getObjValAs? String "outcome").toOption.getD ""
    let thmName := theoremName.toName
    let isDisproof :=
      outcome == "disproved" ||
      thmName.getString! == "disproof" ||
      hasSubstr signature "¬" ||
      hasSubstr signature "Not " ||
      hasSubstr signature "→ False" ||
      hasSubstr signature "-> False"
    expectations := expectations.insert owner.toName {
      namespaceName := thmName.getPrefix
      entryTheorem := some thmName
      isDisproof := isDisproof
    }
  if let [fixtureNamespace] := extra then
    expectations := expectations.insert moduleText.toName {
      namespaceName := fixtureNamespace.toName
    }
  initSearchPath (← findSysroot)
  let moduleName := moduleText.toName
  let env ← importModules #[{ module := moduleName }] {}
  let mut declarations : Array Json := #[]
  let mut roots : Array Name := #[]
  let mut seenEntryTheorems : NameHashSet := {}
  for (name, info) in env.constants.toList do
    let some index := env.getModuleIdxFor? name | continue
    let owner := env.header.moduleNames[index.toNat]!
    unless owner == moduleName || (`SwarmProofs.Generated).isPrefixOf owner do continue
    if info.isUnsafe || info.isPartial then throw <| IO.userError s!"unsafe/partial declaration: {name}"
    if let .axiomInfo _ := info then throw <| IO.userError s!"new axiom: {name}"
    let some exp := expectations.find? owner
      | throw <| IO.userError s!"declaration belongs to unregistered audit module: {owner}"
    let expectedNamespace := exp.namespaceName
    unless (`SwarmProofs.Generated).isPrefixOf expectedNamespace && expectedNamespace.isPrefixOf (privateToUserName name) do
      throw <| IO.userError s!"declaration escaped generated namespace: {name}"
    if exp.entryTheorem == some name then
      seenEntryTheorems := seenEntryTheorems.insert name
    if name == expectedNamespace ++ `proof || name == expectedNamespace ++ `disproof || exp.entryTheorem == some name then
      unless info.levelParams.isEmpty do
        throw <| IO.userError s!"entry theorem {name} must not have universe parameters"
      let targetProp := expectedNamespace ++ `statementProp
      if exp.entryTheorem.isSome && !env.contains targetProp then
        throw <| IO.userError s!"missing target proposition: {targetProp}"
      if env.contains targetProp then
        let expectNegated :=
          (name == expectedNamespace ++ `disproof) ||
          (exp.entryTheorem == some name && exp.isDisproof)
        if expectNegated then
          unless isNegatedTargetType info.type targetProp do
            throw <| IO.userError s!"disproof theorem type {info.type} does not directly negate {targetProp} (expected ¬ {targetProp} or {targetProp} → False without extra binders)"
        else
          unless info.type == .const targetProp [] do
            throw <| IO.userError s!"proof theorem type {info.type} does not match bare {targetProp} (unexpected binders or type mismatch)"
    roots := roots.push name
    declarations := declarations.push <| Json.mkObj [
      ("name", toJson name.toString),
      ("module", toJson owner.toString),
      ("private", toJson (isPrivateName name)),
      ("type", toJson (toString info.type))]
  for m in env.header.moduleNames.push moduleName do
    if (`SwarmProofs.Generated).isPrefixOf m then
      unless expectations.contains m do
        throw <| IO.userError s!"declaration belongs to unregistered audit module: {m}"
    if m == moduleName || (`SwarmProofs.Generated).isPrefixOf m then
      if let some exp := expectations.find? m then
        if let some expectedThm := exp.entryTheorem then
          unless seenEntryTheorems.contains expectedThm do
            throw <| IO.userError s!"missing expected entry theorem: {expectedThm}"
  auditDependencies env roots
  declarations := declarations.qsort fun a b =>
    (a.getObjValAs? String "name").toOption.getD "" <
      (b.getObjValAs? String "name").toOption.getD ""
  IO.println <| (Json.mkObj [
    ("schema", toJson "swarm.lean-declaration-inventory/v1"),
    ("module", toJson moduleText),
    ("declarations", toJson declarations)]).compress
