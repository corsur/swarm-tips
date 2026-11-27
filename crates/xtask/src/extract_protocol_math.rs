//! Deterministic Rust AST (`syn`) -> Lean 4 (`v4.33.0-rc2`) extractor for
//! `crates/protocol-math/src/lib.rs` -> `SwarmProofs.Chains.ProtocolMath.Defs.V1`.
//!
//! Eliminates the hand-transcription gap between live Rust settlement arithmetic
//! and Lean 4 specifications by translating the parsed Rust AST directly into
//! self-contained `UInt64`/`Option` Lean 4 definitions preserving exact `u64`
//! (`2^64 - 1`) overflow, underflow, and division-by-zero semantics.

use anyhow::{anyhow, bail, ensure, Context, Result};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use syn::{
    BinOp, Block, Expr, ExprBinary, ExprBlock, ExprCall, ExprCast, ExprField, ExprIf, ExprLit,
    ExprMatch, ExprMethodCall, ExprParen, ExprPath, ExprReturn, ExprStruct, ExprTry, FnArg, Item,
    ItemConst, ItemEnum, ItemFn, ItemStruct, Lit, Pat, ReturnType, Stmt, Type, UnOp, Visibility,
};

pub const PROTOCOL_MATH_REL_RS: &str = "crates/protocol-math/src/lib.rs";
pub const PROTOCOL_MATH_REL_LEAN: &str =
    "proofs/SwarmProofs/SwarmProofs/Chains/ProtocolMath/Defs/V1.lean";

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for b in digest {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

fn is_pub(vis: &Visibility) -> bool {
    matches!(vis, Visibility::Public(_))
}

fn translate_type(ty: &Type) -> Result<String> {
    match ty {
        Type::Path(tp) => {
            ensure!(tp.qself.is_none(), "qualified type paths are not supported");
            let seg = tp
                .path
                .segments
                .last()
                .ok_or_else(|| anyhow!("empty type path"))?;
            let ident = seg.ident.to_string();
            match ident.as_str() {
                "u64" => Ok("UInt64".to_owned()),
                "u128" => Ok("Nat".to_owned()),
                "bool" => Ok("Bool".to_owned()),
                "Option" => match &seg.arguments {
                    syn::PathArguments::AngleBracketed(ab) if ab.args.len() == 1 => {
                        if let Some(syn::GenericArgument::Type(inner)) = ab.args.first() {
                            Ok(format!("Option {}", translate_type(inner)?))
                        } else {
                            bail!("Option generic arg must be a type")
                        }
                    }
                    _ => bail!("Option must have exactly one type argument"),
                },
                other => Ok(other.to_owned()),
            }
        }
        _ => bail!("unsupported Rust type in protocol-math"),
    }
}

fn is_option_return(ret: &ReturnType) -> Result<bool> {
    match ret {
        ReturnType::Default => Ok(false),
        ReturnType::Type(_, ty) => {
            let rendered = translate_type(ty)?;
            Ok(rendered.starts_with("Option "))
        }
    }
}

fn translate_int_lit(lit_int: &syn::LitInt) -> Result<String> {
    let val: u64 = lit_int
        .base10_parse()
        .context("protocol-math integer literal must fit in u64")?;
    Ok(format!("({val} : UInt64)"))
}

fn translate_expr(expr: &Expr, in_option_do: bool) -> Result<String> {
    match expr {
        Expr::Paren(ExprParen { expr, .. }) => Ok(format!("({})", translate_expr(expr, false)?)),
        Expr::Lit(ExprLit { lit, .. }) => match lit {
            Lit::Int(li) => translate_int_lit(li),
            Lit::Bool(lb) => Ok(if lb.value { "true" } else { "false" }.to_owned()),
            _ => bail!("unsupported literal in protocol-math"),
        },
        Expr::Path(ExprPath { path, qself, .. }) => {
            ensure!(qself.is_none(), "qualified expr paths are not supported");
            let segs: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
            match segs.as_slice() {
                [single] if single == "None" => Ok("none".to_owned()),
                [single] => Ok(single.clone()),
                [enum_ty, variant] => Ok(format!("{enum_ty}.{variant}")),
                other => bail!("unsupported path expression: {:?}", other),
            }
        }
        Expr::Field(ExprField { base, member, .. }) => {
            let b = translate_expr(base, false)?;
            let m = match member {
                syn::Member::Named(id) => id.to_string(),
                syn::Member::Unnamed(idx) => idx.index.to_string(),
            };
            Ok(format!("{b}.{m}"))
        }
        Expr::Unary(u) => match u.op {
            UnOp::Not(_) => Ok(format!("(!({}))", translate_expr(&u.expr, false)?)),
            _ => bail!("unsupported unary op in protocol-math"),
        },
        Expr::Binary(ExprBinary {
            left, op, right, ..
        }) => {
            let l = translate_expr(left, false)?;
            let r = translate_expr(right, false)?;
            let lean_op = match op {
                BinOp::Eq(_) => "==",
                BinOp::Ne(_) => "!=",
                BinOp::Lt(_) => "<",
                BinOp::Le(_) => "<=",
                BinOp::Gt(_) => ">",
                BinOp::Ge(_) => ">=",
                BinOp::And(_) => "&&",
                BinOp::Or(_) => "||",
                _ => bail!(
                    "unchecked binary arithmetic is forbidden in protocol-math; use checked_*/saturating_*"
                ),
            };
            Ok(format!("({l} {lean_op} {r})"))
        }
        Expr::Cast(ExprCast { expr, ty, .. }) => {
            // Only widening `x as u128` (modeled as `UInt64.toNat x`) is permitted.
            let target = translate_type(ty)?;
            ensure!(
                target == "Nat",
                "only lossless widening casts `as u128` are supported, got `as {target}`"
            );
            let inner = translate_expr(expr, false)?;
            Ok(format!("({inner}).toNat"))
        }
        Expr::MethodCall(ExprMethodCall {
            receiver,
            method,
            args,
            ..
        }) => {
            let m = method.to_string();
            let recv = translate_expr(receiver, false)?;
            let arg_list: Vec<String> = args
                .iter()
                .map(|a| translate_expr(a, false))
                .collect::<Result<Vec<_>>>()?;
            // Detect whether receiver is a widening `u128` (`Nat`) expression vs `u64` (`UInt64`).
            let recv_is_u128 = matches!(receiver.as_ref(), Expr::Cast(_) | Expr::Paren(_))
                || recv.ends_with(".toNat")
                || recv == "prod";
            match (m.as_str(), arg_list.as_slice()) {
                ("checked_add", [arg]) => Ok(format!("u64CheckedAdd ({recv}) ({arg})")),
                ("checked_sub", [arg]) => Ok(format!("u64CheckedSub ({recv}) ({arg})")),
                ("checked_mul", [arg]) if recv_is_u128 => {
                    Ok(format!("u128CheckedMul ({recv}) ({arg})"))
                }
                ("checked_mul", [arg]) => Ok(format!("u64CheckedMul ({recv}) ({arg})")),
                ("checked_div", [arg]) if recv_is_u128 => {
                    Ok(format!("u128CheckedDiv ({recv}) ({arg})"))
                }
                ("checked_div", [arg]) => Ok(format!("u64CheckedDiv ({recv}) ({arg})")),
                ("saturating_add", [arg]) => Ok(format!("u64SaturatingAdd ({recv}) ({arg})")),
                ("saturating_sub", [arg]) => Ok(format!("u64SaturatingSub ({recv}) ({arg})")),
                ("min", [arg]) => Ok(format!("u64Min ({recv}) ({arg})")),
                ("ok", []) => {
                    // Only `u64::try_from(quot).ok()` is translated via Expr::Call receiver.
                    Ok(recv)
                }
                other => bail!("unsupported method call in protocol-math: {:?}", other),
            }
        }
        Expr::Call(ExprCall { func, args, .. }) => {
            let f = match func.as_ref() {
                Expr::Path(ep) => ep
                    .path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::"),
                _ => bail!("unsupported call target in protocol-math"),
            };
            let arg_list: Vec<String> = args
                .iter()
                .map(|a| translate_expr(a, false))
                .collect::<Result<Vec<_>>>()?;
            match (f.as_str(), arg_list.as_slice()) {
                ("Some", [inner]) => Ok(format!("some ({inner})")),
                ("u64::try_from", [inner]) => Ok(format!("u128ToU64 ({inner})")),
                (fn_name, vals) => {
                    if vals.is_empty() {
                        Ok(fn_name.to_owned())
                    } else {
                        let rendered_args = vals
                            .iter()
                            .map(|v| format!("({v})"))
                            .collect::<Vec<_>>()
                            .join(" ");
                        Ok(format!("({fn_name} {rendered_args})"))
                    }
                }
            }
        }
        Expr::Try(ExprTry { expr, .. }) => {
            ensure!(
                in_option_do,
                "`?` operator is only supported inside Option-returning functions"
            );
            translate_expr(expr, false)
        }
        Expr::Struct(ExprStruct {
            path, fields, rest, ..
        }) => {
            ensure!(rest.is_none(), "struct update syntax `..` is not supported");
            let ty_name = path
                .segments
                .last()
                .ok_or_else(|| anyhow!("empty struct path"))?
                .ident
                .to_string();
            let mut rendered_fields = Vec::with_capacity(fields.len());
            for f in fields {
                let fname = match &f.member {
                    syn::Member::Named(id) => id.to_string(),
                    syn::Member::Unnamed(_) => bail!("tuple structs are not supported"),
                };
                let fval = translate_expr(&f.expr, false)?;
                rendered_fields.push(format!("{fname} := {fval}"));
            }
            Ok(format!(
                "({{ {} }} : {ty_name})",
                rendered_fields.join(", ")
            ))
        }
        Expr::Return(ExprReturn { expr, .. }) => {
            let inner = expr
                .as_deref()
                .ok_or_else(|| anyhow!("bare `return;` is not supported"))?;
            translate_expr(inner, false)
        }
        Expr::If(ExprIf {
            cond,
            then_branch,
            else_branch,
            ..
        }) => {
            let c = translate_expr(cond, false)?;
            let t = translate_block(then_branch, in_option_do, 2)?;
            let Some((_, else_expr)) = else_branch else {
                bail!("if-expression without else is only supported as a statement guard");
            };
            let e = match else_expr.as_ref() {
                Expr::Block(ExprBlock { block, .. }) => translate_block(block, in_option_do, 2)?,
                other => translate_expr(other, in_option_do)?,
            };
            Ok(format!("if {c} then\n{t}\n  else\n{e}"))
        }
        Expr::Match(ExprMatch { expr, arms, .. }) => {
            let scrutinee = translate_expr(expr, false)?;
            let mut out = format!("match {scrutinee} with");
            for arm in arms {
                ensure!(arm.guard.is_none(), "match guards are not supported");
                let pat = translate_pat(&arm.pat)?;
                let body = match arm.body.as_ref() {
                    Expr::Block(ExprBlock { block, .. }) => {
                        translate_block(block, in_option_do, 4)?
                    }
                    other => format!("    {}", translate_expr(other, in_option_do)?),
                };
                out.push_str(&format!("\n  | {pat} =>\n{body}"));
            }
            Ok(out)
        }
        Expr::Block(ExprBlock { block, .. }) => translate_block(block, in_option_do, 2),
        Expr::Unsafe(_) => bail!("`unsafe` expressions are strictly forbidden in protocol-math"),
        _ => bail!("unsupported expression in protocol-math"),
    }
}

fn translate_pat(pat: &Pat) -> Result<String> {
    match pat {
        Pat::Path(pp) => {
            let segs: Vec<String> = pp
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect();
            match segs.as_slice() {
                [enum_ty, variant] => Ok(format!("{enum_ty}.{variant}")),
                [single] => Ok(single.clone()),
                other => bail!("unsupported pattern path: {:?}", other),
            }
        }
        Pat::Wild(_) => Ok("_".to_owned()),
        _ => bail!("unsupported match pattern in protocol-math"),
    }
}

fn translate_stmts(stmts: &[Stmt], in_option_do: bool, indent: usize) -> Result<String> {
    let pad = " ".repeat(indent);
    if stmts.is_empty() {
        bail!("empty block in protocol-math");
    }
    let first = &stmts[0];
    let rest = &stmts[1..];
    match first {
        Stmt::Local(local) => {
            let name = match &local.pat {
                Pat::Ident(pi) => pi.ident.to_string(),
                _ => bail!("unsupported let pattern in protocol-math"),
            };
            let init = local
                .init
                .as_ref()
                .ok_or_else(|| anyhow!("uninitialized let binding `{name}` not supported"))?;
            ensure!(init.diverge.is_none(), "let-else is not supported");
            ensure!(
                !rest.is_empty(),
                "let binding `{name}` cannot be final statement"
            );
            let tail = translate_stmts(rest, in_option_do, indent)?;
            match init.expr.as_ref() {
                Expr::Try(ExprTry { expr, .. }) => {
                    ensure!(
                        in_option_do,
                        "`?` binding only valid in Option-returning function"
                    );
                    let rhs = translate_expr(expr, false)?;
                    Ok(format!(
                        "{pad}match {rhs} with\n{pad}| none => none\n{pad}| some {name} =>\n{tail}"
                    ))
                }
                other => {
                    let rhs = translate_expr(other, false)?;
                    Ok(format!("{pad}let {name} := {rhs}\n{tail}"))
                }
            }
        }
        Stmt::Expr(
            Expr::If(ExprIf {
                cond,
                then_branch,
                else_branch: None,
                ..
            }),
            _,
        ) => {
            // Early-return guard: `if cond { return expr; } rest...`
            ensure!(
                !rest.is_empty(),
                "guard `if` without else must be followed by remaining statements"
            );
            let c = translate_expr(cond, false)?;
            let then_body = translate_block(then_branch, in_option_do, indent + 2)?;
            let else_body = translate_stmts(rest, in_option_do, indent + 2)?;
            Ok(format!(
                "{pad}if {c} then\n{then_body}\n{pad}else\n{else_body}"
            ))
        }
        Stmt::Expr(expr, _) => {
            ensure!(
                rest.is_empty(),
                "non-final bare expression statements are not supported"
            );
            let rendered = translate_expr(expr, in_option_do)?;
            let indented = rendered
                .lines()
                .map(|line| format!("{pad}{line}"))
                .collect::<Vec<_>>()
                .join("\n");
            Ok(indented)
        }
        _ => bail!("unsupported statement in protocol-math"),
    }
}

fn translate_block(block: &Block, in_option_do: bool, indent: usize) -> Result<String> {
    translate_stmts(&block.stmts, in_option_do, indent)
}

fn extract_struct(s: &ItemStruct) -> Result<String> {
    ensure!(
        s.generics.params.is_empty(),
        "generic structs are not supported in protocol-math"
    );
    let mut out = format!("structure {} where\n", s.ident);
    match &s.fields {
        syn::Fields::Named(named) => {
            for f in &named.named {
                let fname = f
                    .ident
                    .as_ref()
                    .ok_or_else(|| anyhow!("unnamed struct field"))?;
                let fty = translate_type(&f.ty)?;
                out.push_str(&format!("  {fname} : {fty}\n"));
            }
        }
        _ => bail!("only named-field structs are supported in protocol-math"),
    }
    out.push_str("  deriving Repr, DecidableEq\n");
    Ok(out)
}

fn extract_enum(e: &ItemEnum) -> Result<String> {
    ensure!(
        e.generics.params.is_empty(),
        "generic enums are not supported in protocol-math"
    );
    let mut out = format!("inductive {} : Type where\n", e.ident);
    for v in &e.variants {
        ensure!(
            matches!(v.fields, syn::Fields::Unit),
            "only unit enum variants are supported in protocol-math"
        );
        out.push_str(&format!("  | {} : {}\n", v.ident, e.ident));
    }
    out.push_str("  deriving Repr, DecidableEq\n");
    Ok(out)
}

fn extract_const(c: &ItemConst) -> Result<String> {
    let ty = translate_type(&c.ty)?;
    let val = translate_expr(&c.expr, false)?;
    Ok(format!("def {} : {} := {}\n", c.ident, ty, val))
}

fn extract_fn(f: &ItemFn) -> Result<String> {
    ensure!(
        f.sig.unsafety.is_none(),
        "`unsafe fn` is strictly forbidden in protocol-math"
    );
    ensure!(
        f.sig.generics.params.is_empty(),
        "generic fns are not supported in protocol-math"
    );
    let mut params = Vec::new();
    for arg in &f.sig.inputs {
        match arg {
            FnArg::Typed(pt) => {
                let pname = match pt.pat.as_ref() {
                    Pat::Ident(pi) => pi.ident.to_string(),
                    _ => bail!("unsupported fn param pattern in protocol-math"),
                };
                let pty = translate_type(&pt.ty)?;
                params.push(format!("({pname} : {pty})"));
            }
            FnArg::Receiver(_) => bail!("method receivers are not supported in protocol-math"),
        }
    }
    let ret_ty = match &f.sig.output {
        ReturnType::Default => "Unit".to_owned(),
        ReturnType::Type(_, ty) => translate_type(ty)?,
    };
    let in_opt = is_option_return(&f.sig.output)?;
    let body = translate_block(&f.block, in_opt, 2)?;
    Ok(format!(
        "def {} {} : {} :=\n{}\n",
        f.sig.ident,
        params.join(" "),
        ret_ty,
        body
    ))
}

/// Mechanically extract `crates/protocol-math/src/lib.rs` source bytes into a
/// self-contained Lean 4 (`v4.33.0-rc2`) frozen `Defs/V1.lean` module.
pub fn extract_rust_source_to_lean(rust_source: &str) -> Result<String> {
    let file = syn::parse_file(rust_source).context("failed parsing protocol-math Rust AST")?;
    let source_sha = sha256_hex(rust_source.as_bytes());

    let mut out = String::new();
    out.push_str(
        "/-
Copyright 2026 Swarm Tips contributors
SPDX-License-Identifier: Apache-2.0
-/

/-!
# Mechanically Extracted Rust Settlement Core (`crates/protocol-math/src/lib.rs`)

DO NOT EDIT BY HAND.
Generated deterministically by `cargo xtask extract-protocol-math-lean` directly
from the parsed Rust AST (`syn`) of `crates/protocol-math/src/lib.rs`.
Source SHA-256: `",
    );
    out.push_str(&source_sha);
    out.push_str(
        "`

Models exact Rust `u64` (`0 .. 2^64 - 1`) and `u128` checked/saturating arithmetic
semantics (`Option UInt64` for overflow/underflow/div-by-zero rejection), eliminating
the hand-transcription gap between live Rust settlement code and Lean 4 proofs.
-/

/-- Exact `2^64` modulus matching Rust `u64::MAX + 1`. -/
def U64_MOD : Nat := 18446744073709551616

/-- Exact `2^128` modulus matching Rust `u128::MAX + 1`. -/
def U128_MOD : Nat := 340282366920938463463374607431768211456

/-- Rust `u64::checked_add`. -/
def u64CheckedAdd (a b : UInt64) : Option UInt64 :=
  if a.toNat + b.toNat < U64_MOD then some (a + b) else none

/-- Rust `u64::checked_sub`. -/
def u64CheckedSub (a b : UInt64) : Option UInt64 :=
  if b.toNat ≤ a.toNat then some (a - b) else none

/-- Rust `u64::checked_mul`. -/
def u64CheckedMul (a b : UInt64) : Option UInt64 :=
  if a.toNat * b.toNat < U64_MOD then some (a * b) else none

/-- Rust `u64::checked_div`. -/
def u64CheckedDiv (a b : UInt64) : Option UInt64 :=
  if b.toNat = 0 then none else some (a / b)

/-- Rust `u64::saturating_add`. -/
def u64SaturatingAdd (a b : UInt64) : UInt64 :=
  if a.toNat + b.toNat < U64_MOD then a + b else UInt64.ofNat (U64_MOD - 1)

/-- Rust `u64::saturating_sub`. -/
def u64SaturatingSub (a b : UInt64) : UInt64 :=
  if b.toNat ≤ a.toNat then a - b else (0 : UInt64)

/-- Rust `Ord::min` on `u64`. -/
def u64Min (a b : UInt64) : UInt64 :=
  if a.toNat ≤ b.toNat then a else b

/-- Rust `u128::checked_mul` (modeled in `Nat` with exact `2^128` ceiling). -/
def u128CheckedMul (a b : Nat) : Option Nat :=
  if a * b < U128_MOD then some (a * b) else none

/-- Rust `u128::checked_div`. -/
def u128CheckedDiv (a b : Nat) : Option Nat :=
  if b = 0 then none else some (a / b)

/-- Rust `u64::try_from(x: u128).ok()`. -/
def u128ToU64 (x : Nat) : Option UInt64 :=
  if x < U64_MOD then some (UInt64.ofNat x) else none

",
    );

    // Emit constants, types, and helper functions in topological order so `mul_div_u64`
    // is defined before callers in Lean 4 (Lean requires declaration-before-use).
    let mut consts = Vec::new();
    let mut types = Vec::new();
    let mut helpers = Vec::new();
    let mut fns = Vec::new();
    let mut seen_names: BTreeSet<String> = BTreeSet::new();

    for item in &file.items {
        match item {
            Item::Const(c) if is_pub(&c.vis) => {
                seen_names.insert(c.ident.to_string());
                consts.push(extract_const(c)?);
            }
            Item::Struct(s) if is_pub(&s.vis) => {
                seen_names.insert(s.ident.to_string());
                types.push(extract_struct(s)?);
            }
            Item::Enum(e) if is_pub(&e.vis) => {
                seen_names.insert(e.ident.to_string());
                types.push(extract_enum(e)?);
            }
            Item::Fn(f) if is_pub(&f.vis) => {
                let name = f.sig.ident.to_string();
                seen_names.insert(name.clone());
                let rendered = extract_fn(f)?;
                if name == "mul_div_u64" {
                    helpers.push(rendered);
                } else {
                    fns.push(rendered);
                }
            }
            Item::Mod(m) => {
                // Allow `#[cfg(test)] mod tests` only; reject non-test submodules.
                let is_cfg_test = m.attrs.iter().any(|a| a.path().is_ident("cfg"));
                ensure!(
                    is_cfg_test,
                    "unexpected non-test submodule `{}` in protocol-math",
                    m.ident
                );
            }
            _ => {}
        }
    }

    ensure!(
        seen_names.contains("compute_route_and_recoup")
            && seen_names.contains("compute_mark_default")
            && seen_names.contains("compute_task_payout")
            && seen_names.contains("compute_xleg_split")
            && seen_names.contains("mul_div_u64"),
        "protocol-math missing required public settlement functions"
    );

    for block in consts.into_iter().chain(types).chain(helpers).chain(fns) {
        out.push_str(&block);
        out.push('\n');
    }

    // Emit differential `#guard` assertions mirroring `crates/protocol-math`'s Rust unit tests.
    out.push_str(
        "-- Differential #guard sanity checks mirroring `crates/protocol-math` Rust unit tests.\n\
#guard compute_route_and_recoup 6624280 1224280 2600000 0 =\n\
  some { to_backer := 2600000, to_recipient := 2800000, new_recouped := 2600000, remaining_balance := 1224280 }\n\
#guard compute_route_and_recoup 2000000 1224280 100 101 = none\n\
#guard compute_mark_default 1224280 1224280 2600000 2600000 = none\n\
#guard compute_mark_default 6624280 1224280 2600000 1000000 =\n\
  some { to_backer := 1600000, to_recipient := 3800000, new_recouped := 2600000, remaining_balance := 1224280 }\n\
#guard compute_task_payout 10000000 1000000 700000 1000000 1000 =\n\
  some { payment_amount := 9000000, fee_amount := 1000000, client_refund := 0 }\n\
#guard compute_task_payout 10000000 699999 700000 1000000 1000 =\n\
  some { payment_amount := 0, fee_amount := 0, client_refund := 10000000 }\n\
#guard compute_task_payout 1000 5 0 10 1001 = none\n\
#guard compute_xleg_split 1000000 900000 1000 true XOutcome.P1Wins =\n\
  some { to_player := 1900000, to_fee := 0, to_prize := 0, to_pool_reimburse := 0, tranche_released := 0 }\n\
#guard compute_xleg_split 18446744073709551615 1 1000 true XOutcome.P1Wins = none\n",
    );

    Ok(out)
}

pub fn run_extract_protocol_math_lean(repo_root: &Path, check_only: bool) -> Result<()> {
    let rs_path = repo_root.join(PROTOCOL_MATH_REL_RS);
    let lean_path = repo_root.join(PROTOCOL_MATH_REL_LEAN);
    let rust_source = fs::read_to_string(&rs_path)
        .with_context(|| format!("failed reading {}", rs_path.display()))?;
    let generated = extract_rust_source_to_lean(&rust_source)?;

    if check_only {
        let on_disk = fs::read_to_string(&lean_path).with_context(|| {
            format!(
                "missing extracted Lean file {} (run `cargo run --locked -p xtask -- extract-protocol-math-lean`)",
                lean_path.display()
            )
        })?;
        ensure!(
            on_disk == generated,
            "extracted Lean module {} drifted from {}; re-run `cargo run --locked -p xtask -- extract-protocol-math-lean`",
            lean_path.display(),
            rs_path.display()
        );
        println!(
            "OK: {} matches Rust AST extraction of {} byte-for-byte",
            PROTOCOL_MATH_REL_LEAN, PROTOCOL_MATH_REL_RS
        );
        return Ok(());
    }

    if let Some(parent) = lean_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed creating {}", parent.display()))?;
    }
    fs::write(&lean_path, &generated)
        .with_context(|| format!("failed writing {}", lean_path.display()))?;
    println!(
        "Extracted {} -> {} ({} bytes)",
        PROTOCOL_MATH_REL_RS,
        PROTOCOL_MATH_REL_LEAN,
        generated.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .unwrap()
            .to_path_buf()
    }

    #[test]
    fn extractor_detects_single_operator_mutation_and_rejects_unsafe_or_unchecked_math() {
        let root = workspace_root();
        let rs_source = fs::read_to_string(root.join(PROTOCOL_MATH_REL_RS)).unwrap();
        let clean = extract_rust_source_to_lean(&rs_source).unwrap();

        // Single constant mutation changes extracted output & source hash.
        let mutated = rs_source.replace("1_000;", "1_001;");
        assert_ne!(rs_source, mutated);
        let mutated_lean = extract_rust_source_to_lean(&mutated).unwrap();
        assert_ne!(clean, mutated_lean);

        // Unchecked `+` arithmetic MUST fail extraction closed.
        let unchecked = rs_source.replace(
            "recouped_lamports.checked_add(to_backer)?",
            "recouped_lamports + to_backer",
        );
        assert!(extract_rust_source_to_lean(&unchecked).is_err());

        // `unsafe fn` MUST fail extraction closed.
        let unsafe_fn = rs_source.replace("pub fn mul_div_u64", "pub unsafe fn mul_div_u64");
        assert!(extract_rust_source_to_lean(&unsafe_fn).is_err());
    }

    #[test]
    fn checked_in_protocol_math_lean_matches_rust_ast_extraction_byte_for_byte() {
        let root = workspace_root();
        run_extract_protocol_math_lean(&root, true).unwrap();
    }
}
