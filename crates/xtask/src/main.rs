#![forbid(unsafe_code)]
#![deny(warnings)]
#![deny(clippy::all)]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

mod firestore_ops;
mod http;
mod inbox_eval;
mod mirror_proof_registry;
mod proof_audit;
#[cfg(test)]
mod python_guardrail_tests;

use anyhow::{bail, ensure, Result};
use firestore_ops::{AuditArgs, ReconcileArgs};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "\
usage: cargo xtask <subcommand> [args...]

Subcommands:
  prepare-proof-promotion-audit [--package <dir>]
  check-proof-sources [--package <dir>]
  export-proof-declarations <package-root> <exporter-bin> <module> <inventory.json> <export.txt>
  check-proof-export-coverage <inventory.json> <export.txt> <coverage.json>
  write-proof-verification-receipt <report-root> <lean4export-rev> <nanoda-rev>
  check-proof-axiom-audit <audit-output.txt> [registry.json]
  mirror-proof-registry --project <gcp-project> --package <package-root>
  audit-stuck-shillbot [--project <gcp-project>] [--rpc <solana-rpc-url>]
  reconcile-firestore-state <task-id> <new-state> [--project <gcp-project>] [--execute|--apply]
  inbox-eval <state-dir> <snapshot.json> <case> <request-json>";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match dispatch(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn workspace_root() -> Result<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| anyhow::anyhow!("cannot resolve workspace root from CARGO_MANIFEST_DIR"))?;
    ensure!(
        root.is_dir(),
        "workspace root does not exist: {}",
        root.display()
    );
    Ok(root.to_path_buf())
}

fn dispatch(args: &[String]) -> Result<()> {
    let Some(cmd) = args.first().map(String::as_str) else {
        bail!("{USAGE}");
    };
    let rest = &args[1..];
    match cmd {
        "--help" | "-h" | "help" => {
            println!("{USAGE}");
            Ok(())
        }
        "prepare-proof-promotion-audit" => {
            let pkg = parse_optional_package(rest)?;
            proof_audit::prepare_proof_promotion_audit(&pkg)
        }
        "check-proof-sources" => {
            let root = workspace_root()?;
            let pkg = parse_optional_package(rest)?;
            proof_audit::check_proof_sources(&root, &pkg)
        }
        "export-proof-declarations" => {
            ensure!(rest.len() == 5, "{USAGE}");
            proof_audit::export_proof_declarations(
                Path::new(&rest[0]),
                Path::new(&rest[1]),
                &rest[2],
                Path::new(&rest[3]),
                Path::new(&rest[4]),
            )
        }
        "check-proof-export-coverage" => {
            ensure!(rest.len() == 3, "{USAGE}");
            proof_audit::check_proof_export_coverage_files(
                Path::new(&rest[0]),
                Path::new(&rest[1]),
                Path::new(&rest[2]),
            )
        }
        "write-proof-verification-receipt" => {
            ensure!(rest.len() == 3, "{USAGE}");
            proof_audit::write_proof_verification_receipt(Path::new(&rest[0]), &rest[1], &rest[2])
        }
        "check-proof-axiom-audit" => {
            ensure!((1..=2).contains(&rest.len()), "{USAGE}");
            let reg = rest.get(1).map(Path::new);
            proof_audit::check_proof_axiom_audit(Path::new(&rest[0]), reg).map(|_| ())
        }
        "mirror-proof-registry" => {
            let (project, package) = parse_mirror_args(rest)?;
            mirror_proof_registry::mirror_proof_registry(&project, &package)
        }
        "audit-stuck-shillbot" => {
            let audit_args = parse_audit_args(rest)?;
            firestore_ops::run_audit_stuck_shillbot(&audit_args)
        }
        "reconcile-firestore-state" => {
            let rec_args = parse_reconcile_args(rest)?;
            firestore_ops::run_reconcile_firestore_state(&rec_args)
        }
        "inbox-eval" => {
            ensure!(rest.len() == 4, "{USAGE}");
            let out = inbox_eval::run_inbox_eval(
                Path::new(&rest[0]),
                Path::new(&rest[1]),
                &rest[2],
                &rest[3],
            )?;
            println!("{out}");
            Ok(())
        }
        other => bail!("unknown xtask subcommand `{other}`\n\n{USAGE}"),
    }
}

fn parse_optional_package(args: &[String]) -> Result<PathBuf> {
    match args {
        [] => Ok(workspace_root()?.join("proofs").join("SwarmProofs")),
        [single] if !single.starts_with('-') => Ok(PathBuf::from(single)),
        [flag, path] if flag == "--package" => Ok(PathBuf::from(path)),
        [combined] if combined.starts_with("--package=") => {
            let p = combined.trim_start_matches("--package=");
            ensure!(!p.is_empty(), "--package requires a path");
            Ok(PathBuf::from(p))
        }
        _ => bail!("{USAGE}"),
    }
}

fn parse_mirror_args(args: &[String]) -> Result<(String, PathBuf)> {
    let mut project: Option<String> = None;
    let mut package: Option<PathBuf> = None;
    let mut i = 0usize;
    while i < args.len() {
        let arg = args[i].as_str();
        if arg == "--project" {
            let val = args
                .get(i + 1)
                .ok_or_else(|| anyhow::anyhow!("missing value for --project"))?;
            project = Some(val.clone());
            i += 2;
        } else if let Some(val) = arg.strip_prefix("--project=") {
            project = Some(val.to_string());
            i += 1;
        } else if arg == "--package" {
            let val = args
                .get(i + 1)
                .ok_or_else(|| anyhow::anyhow!("missing value for --package"))?;
            package = Some(PathBuf::from(val));
            i += 2;
        } else if let Some(val) = arg.strip_prefix("--package=") {
            package = Some(PathBuf::from(val));
            i += 1;
        } else {
            bail!("unexpected argument `{arg}`\n\n{USAGE}");
        }
    }
    let proj = project.ok_or_else(|| anyhow::anyhow!("--project is required\n\n{USAGE}"))?;
    let pkg = package.ok_or_else(|| anyhow::anyhow!("--package is required\n\n{USAGE}"))?;
    Ok((proj, pkg))
}

fn parse_audit_args(args: &[String]) -> Result<AuditArgs> {
    let mut parsed = AuditArgs::default();
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--project" => {
                let val = args
                    .get(i + 1)
                    .ok_or_else(|| anyhow::anyhow!("missing --project value"))?;
                parsed.project = val.clone();
                i += 2;
            }
            "--rpc" => {
                let val = args
                    .get(i + 1)
                    .ok_or_else(|| anyhow::anyhow!("missing --rpc value"))?;
                parsed.rpc_url = val.clone();
                i += 2;
            }
            "--dry-run" => i += 1,
            other => bail!("unexpected argument `{other}`\n\n{USAGE}"),
        }
    }
    ensure!(
        !parsed.project.is_empty() && !parsed.rpc_url.is_empty(),
        "{USAGE}"
    );
    Ok(parsed)
}

fn parse_reconcile_args(args: &[String]) -> Result<ReconcileArgs> {
    let mut positional: Vec<String> = Vec::new();
    let mut project = String::from("coordination-game-prod");
    let mut execute = false;
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--execute" | "--apply" => {
                execute = true;
                i += 1;
            }
            "--dry-run" => {
                execute = false;
                i += 1;
            }
            "--project" => {
                let val = args
                    .get(i + 1)
                    .ok_or_else(|| anyhow::anyhow!("missing --project value"))?;
                project = val.clone();
                i += 2;
            }
            other if other.starts_with('-') => bail!("unexpected flag `{other}`\n\n{USAGE}"),
            pos => {
                positional.push(pos.to_string());
                i += 1;
            }
        }
    }
    ensure!(positional.len() == 2, "usage: cargo xtask reconcile-firestore-state <task_id> <new_state> [--project <gcp-project>] [--execute|--apply]");
    Ok(ReconcileArgs {
        task_id: positional[0].clone(),
        new_state: positional[1].clone(),
        project,
        execute,
    })
}
