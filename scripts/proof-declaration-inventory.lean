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
  let mut namespaces : NameMap Name := {}
  for entry in modules do
    let .ok owner := entry.getObjValAs? String "module_name" | throw <| IO.userError "missing module name"
    let .ok theoremName := entry.getObjValAs? String "theorem_name" | throw <| IO.userError "missing theorem name"
    namespaces := namespaces.insert owner.toName theoremName.toName.getPrefix
  if let [fixtureNamespace] := extra then
    namespaces := namespaces.insert moduleText.toName fixtureNamespace.toName
  initSearchPath (← findSysroot)
  let moduleName := moduleText.toName
  let env ← importModules #[{ module := moduleName }] {}
  let mut declarations : Array Json := #[]
  let mut roots : Array Name := #[]
  for (name, info) in env.constants.toList do
    let some index := env.getModuleIdxFor? name | continue
    let owner := env.header.moduleNames[index.toNat]!
    unless owner == moduleName || (`SwarmProofs.Generated).isPrefixOf owner do continue
    if info.isUnsafe || info.isPartial then throw <| IO.userError s!"unsafe/partial declaration: {name}"
    if let .axiomInfo _ := info then throw <| IO.userError s!"new axiom: {name}"
    let some expectedNamespace := namespaces.find? owner
      | throw <| IO.userError s!"declaration belongs to unregistered audit module: {owner}"
    unless (`SwarmProofs.Generated).isPrefixOf expectedNamespace && expectedNamespace.isPrefixOf (privateToUserName name) do
      throw <| IO.userError s!"declaration escaped generated namespace: {name}"
    roots := roots.push name
    declarations := declarations.push <| Json.mkObj [
      ("name", toJson name.toString),
      ("module", toJson owner.toString),
      ("private", toJson (isPrivateName name)),
      ("type", toJson (toString info.type))]
  auditDependencies env roots
  declarations := declarations.qsort fun a b =>
    (a.getObjValAs? String "name").toOption.getD "" <
      (b.getObjValAs? String "name").toOption.getD ""
  IO.println <| (Json.mkObj [
    ("schema", toJson "swarm.lean-declaration-inventory/v1"),
    ("module", toJson moduleText),
    ("declarations", toJson declarations)]).compress
