/- SPDX-License-Identifier: Apache-2.0 -/
import Lean

open Lean

/-- Inspect parsed syntax, never substrings in comments or string literals. -/
partial def auditSyntax (stx : Syntax) : IO Unit := do
  let forbiddenKinds := #[
    ``Parser.Command.axiom, ``Parser.Term.attributes,
    ``Parser.Command.deriving]
  if forbiddenKinds.contains stx.getKind then
    throw <| IO.userError s!"forbidden syntax: {stx.getKind}"
  if let .atom _ token := stx then
    if #["unsafe", "partial", "meta", "sorry", "admit", "run_tac", "run_elab",
      "by_elab", "native_decide", "extern", "implemented_by", "deriving"].contains token.trimAscii.toString then
      throw <| IO.userError s!"forbidden syntax token: {token}"
  for child in stx.getArgs do auditSyntax child

unsafe def main (args : List String) : IO Unit := do
  let path :: expectedNamespace :: allowedImports := args | throw <| IO.userError "expected source path, generated namespace and exact allowed imports"
  initSearchPath (← findSysroot)
  -- Explicitly approved trusted-library initialization. This fixed import list
  -- must never include candidate paths or their declared dependency imports.
  enableInitializersExecution
  let env ← importModules #[{ module := `Mathlib }, { module := `Zeta23 }] {} (loadExts := true)
  let input := Parser.mkInputContext (← IO.FS.readFile path) path
  let (header, initial, messages) ← Parser.parseHeader input
  if messages.hasErrors then throw <| IO.userError "invalid import header"
  if let `(Parser.Module.header| $[module%$moduleTk?]? $[prelude%$preludeTk?]? $imports*) := header then
    if moduleTk?.isSome || preludeTk?.isSome then throw <| IO.userError "module/prelude overrides forbidden"
    for item in imports do
      if let `(Parser.Module.import| import $name:ident) := item then
        unless allowedImports.contains name.getId.toString do
          throw <| IO.userError s!"undeclared import: {name.getId}"
      else throw <| IO.userError "noncanonical import"
  else throw <| IO.userError "unrecognized header"
  let mut state := initial
  let mut inNamespace := false
  let mut closed := false
  let mut namespaceName := Name.anonymous
  repeat
    let (command, next, messages) := Parser.parseCommand input { env, options := {} } state {}
    if messages.hasErrors then
      messages.forM fun message => message.toString >>= IO.eprintln
      throw <| IO.userError "source parsing failed"
    state := next
    if command.isOfKind ``Parser.Command.eoi then break
    auditSyntax command
    if let `(command| namespace $name:ident) := command then
      if inNamespace || closed || name.getId.toString != expectedNamespace || !( (`SwarmProofs.Generated).isPrefixOf name.getId) then
        throw <| IO.userError "invalid generated namespace"
      namespaceName := name.getId
      inNamespace := true
    else if let `(command| end $name:ident) := command then
      unless inNamespace && name.getId == namespaceName do
        throw <| IO.userError "namespace escape"
      inNamespace := false
      closed := true
    else
      unless inNamespace && !closed do throw <| IO.userError "declaration outside generated namespace"
      unless #[``Parser.Command.declaration, ``Parser.Command.open,
        ``Parser.Command.variable, ``Parser.Command.universe].contains command.getKind do
        throw <| IO.userError s!"forbidden command: {command.getKind}"
  unless closed && !inNamespace do throw <| IO.userError "missing generated namespace boundary"
  IO.println "parsed promotion source policy passed"
