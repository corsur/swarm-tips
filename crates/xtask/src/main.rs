#![forbid(unsafe_code)]
#![deny(warnings)]
#![deny(clippy::all)]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

mod fetch_promotion_candidates;
mod firestore_ops;
mod http;
mod inbox_eval;
mod mirror_proof_registry;
mod promote_pipeline;
mod proof_audit;
#[cfg(test)]
mod python_guardrail_tests;

use anyhow::{bail, ensure, Result};
use fetch_promotion_candidates::FetchCandidatesArgs;
use firestore_ops::{AuditArgs, ReconcileArgs};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "\
usage: cargo xtask <subcommand> [args...]

Subcommands:
  prepare-proof-promotion-audit [--package <dir>]
  check-proof-sources [--package <dir>]
  check-frozen-defs-append-only [--base-registry <path>] [--package <dir>]
  export-proof-declarations <package-root> <exporter-bin> <module> <inventory.json> <export.txt>
  check-proof-export-coverage <inventory.json> <export.txt> <coverage.json>
  check-statement-expr-stability <package-root> <lean4export-bin> <module-name> <standalone-statement-file> <generated-namespace>
  check-promoted-statements-expr-stability <package-root> <lean4export-bin> <candidates-dir>
  write-proof-verification-receipt <report-root> <lean4export-rev> <nanoda-rev>
  check-proof-axiom-audit <audit-output.txt> [registry.json]
  fetch-promotion-candidates --out-dir <dir> [--api <url>] [--network <net>] [--bucket <gcs-bucket>] [--project <gcp-project>] [--registry <path>] [--catalog-dir <dir>] [--allowed-poster <wallet>] [--tasks-file <json>] [--rejected-file <json>] [--dry-run-registry <tmp-dir>] [--task-id <id>]
  audit-staged-candidates --staged-dir <dir> --verdicts-out <json> [--package <dir>] [--network <net>] [--skip-nanoda]
  apply-verified-promotions --staged-dir <dir> --verdicts <json> [--registry <path>] [--package <dir>] [--rejected-file <json>] [--public-base-url <url>]
  promote-pipeline --network <net> --dry-run-registry <tmp-dir> [--api <url>] [--bucket <gcs-bucket>] [--project <gcp-project>] [--catalog-dir <dir>] [--allowed-poster <wallet>] [--tasks-file <json>] [--task-id <id>] [--skip-nanoda]
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
        "check-frozen-defs-append-only" => {
            let root = workspace_root()?;
            let (pkg, base_reg) = parse_frozen_defs_append_only_args(rest)?;
            proof_audit::check_frozen_defs_append_only(&root, &pkg, base_reg.as_deref())
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
        "check-statement-expr-stability" => {
            ensure!(rest.len() == 5, "{USAGE}");
            proof_audit::check_statement_expr_stability(
                Path::new(&rest[0]),
                Path::new(&rest[1]),
                &rest[2],
                Path::new(&rest[3]),
                &rest[4],
            )
        }
        "check-promoted-statements-expr-stability" => {
            ensure!(rest.len() == 3, "{USAGE}");
            proof_audit::check_promoted_statements_expr_stability(
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
        "fetch-promotion-candidates" => {
            let fetch_args = parse_fetch_candidates_args(rest, true)?;
            fetch_promotion_candidates::run_fetch_promotion_candidates(&fetch_args)
        }
        "audit-staged-candidates" => {
            let root = workspace_root()?;
            let audit_args = parse_audit_staged_args(rest)?;
            promote_pipeline::run_audit_staged_candidates(&root, &audit_args)
        }
        "apply-verified-promotions" => {
            let apply_args = parse_apply_verified_args(rest)?;
            let count = promote_pipeline::apply_verified_promotions(&apply_args)?;
            println!("reconstructed and promoted {count} verified candidate(s)");
            Ok(())
        }
        "promote-pipeline" => {
            let root = workspace_root()?;
            let pipe_args = parse_promote_pipeline_args(rest)?;
            promote_pipeline::run_promote_pipeline(&root, &pipe_args)
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

fn parse_fetch_candidates_args(
    args: &[String],
    require_out_dir: bool,
) -> Result<FetchCandidatesArgs> {
    let root = workspace_root()?;
    let mut api_base = String::from(fetch_promotion_candidates::DEFAULT_SHILLBOT_API);
    let mut network = String::from(fetch_promotion_candidates::DEFAULT_NETWORK);
    let mut bucket = String::from(fetch_promotion_candidates::DEFAULT_ARTIFACT_BUCKET);
    let mut project = String::from(fetch_promotion_candidates::DEFAULT_GCP_PROJECT);
    let mut registry_path = root
        .join("proofs")
        .join("SwarmProofs")
        .join("registry.json");
    let default_rejected = root
        .join("proofs")
        .join("SwarmProofs")
        .join("rejected-candidates.json");
    let mut rejected_file: Option<PathBuf> = if default_rejected.is_file() {
        Some(default_rejected)
    } else {
        None
    };
    let mut catalog_dir = root
        .join("tests")
        .join("fixtures")
        .join("lean")
        .join("catalog");
    let mut custom_posters: Vec<String> = Vec::new();
    let mut out_dir: Option<PathBuf> = None;
    let mut tasks_file: Option<PathBuf> = None;
    let mut dry_run_registry: Option<PathBuf> = None;
    let mut only_task_id: Option<String> = None;
    let mut firestore_fallback = true;

    let mut i = 0usize;
    while i < args.len() {
        let arg = args[i].as_str();
        let take_val = |flag: &str, idx: &mut usize| -> Result<String> {
            if let Some(eq_val) = arg.strip_prefix(&format!("{flag}=")) {
                ensure!(!eq_val.is_empty(), "{flag} requires a non-empty value");
                *idx += 1;
                return Ok(eq_val.to_string());
            }
            let val = args
                .get(*idx + 1)
                .ok_or_else(|| anyhow::anyhow!("missing value for {flag}"))?;
            ensure!(!val.is_empty(), "{flag} requires a non-empty value");
            *idx += 2;
            Ok(val.clone())
        };

        if arg == "--api" || arg.starts_with("--api=") {
            api_base = take_val("--api", &mut i)?;
        } else if arg == "--network" || arg.starts_with("--network=") {
            network = take_val("--network", &mut i)?;
        } else if arg == "--bucket" || arg.starts_with("--bucket=") {
            bucket = take_val("--bucket", &mut i)?;
        } else if arg == "--project" || arg.starts_with("--project=") {
            project = take_val("--project", &mut i)?;
        } else if arg == "--registry" || arg.starts_with("--registry=") {
            registry_path = PathBuf::from(take_val("--registry", &mut i)?);
        } else if arg == "--catalog-dir" || arg.starts_with("--catalog-dir=") {
            catalog_dir = PathBuf::from(take_val("--catalog-dir", &mut i)?);
        } else if arg == "--allowed-poster" || arg.starts_with("--allowed-poster=") {
            custom_posters.push(take_val("--allowed-poster", &mut i)?);
        } else if arg == "--out-dir" || arg.starts_with("--out-dir=") {
            out_dir = Some(PathBuf::from(take_val("--out-dir", &mut i)?));
        } else if arg == "--tasks-file" || arg.starts_with("--tasks-file=") {
            tasks_file = Some(PathBuf::from(take_val("--tasks-file", &mut i)?));
        } else if arg == "--rejected-file" || arg.starts_with("--rejected-file=") {
            rejected_file = Some(PathBuf::from(take_val("--rejected-file", &mut i)?));
        } else if arg == "--dry-run-registry" || arg.starts_with("--dry-run-registry=") {
            dry_run_registry = Some(PathBuf::from(take_val("--dry-run-registry", &mut i)?));
        } else if arg == "--task-id" || arg.starts_with("--task-id=") {
            only_task_id = Some(take_val("--task-id", &mut i)?);
        } else if arg == "--no-firestore-fallback" {
            firestore_fallback = false;
            i += 1;
        } else if arg == "--firestore-fallback" {
            firestore_fallback = true;
            i += 1;
        } else {
            bail!("unexpected argument `{arg}`\n\n{USAGE}");
        }
    }

    let allowed_posters = if custom_posters.is_empty() {
        fetch_promotion_candidates::PINNED_POSTER_WALLETS
            .iter()
            .map(|w| (*w).to_string())
            .collect()
    } else {
        custom_posters
    };
    let out_dir = match out_dir {
        Some(d) => d,
        None if !require_out_dir => dry_run_registry
            .as_ref()
            .map(|d| d.join("candidates"))
            .unwrap_or_else(|| PathBuf::from(".")),
        None => bail!("--out-dir is required\n\n{USAGE}"),
    };
    Ok(FetchCandidatesArgs {
        api_base,
        network,
        bucket,
        project,
        registry_path,
        catalog_dir,
        allowed_posters,
        out_dir,
        tasks_file,
        firestore_fallback,
        rejected_file,
        dry_run_registry,
        only_task_id,
    })
}

fn parse_audit_staged_args(args: &[String]) -> Result<promote_pipeline::AuditStagedArgs> {
    let root = workspace_root()?;
    let mut staged_dir: Option<PathBuf> = None;
    let mut package_root = root.join("proofs").join("SwarmProofs");
    let mut verdicts_out: Option<PathBuf> = None;
    let mut network = String::from(fetch_promotion_candidates::DEFAULT_NETWORK);
    let mut skip_nanoda = false;

    let mut i = 0usize;
    while i < args.len() {
        let arg = args[i].as_str();
        let take_val = |flag: &str, idx: &mut usize| -> Result<String> {
            if let Some(eq_val) = arg.strip_prefix(&format!("{flag}=")) {
                ensure!(!eq_val.is_empty(), "{flag} requires a non-empty value");
                *idx += 1;
                return Ok(eq_val.to_string());
            }
            let val = args
                .get(*idx + 1)
                .ok_or_else(|| anyhow::anyhow!("missing value for {flag}"))?;
            ensure!(!val.is_empty(), "{flag} requires a non-empty value");
            *idx += 2;
            Ok(val.clone())
        };
        if arg == "--staged-dir" || arg.starts_with("--staged-dir=") {
            staged_dir = Some(PathBuf::from(take_val("--staged-dir", &mut i)?));
        } else if arg == "--package" || arg.starts_with("--package=") {
            package_root = PathBuf::from(take_val("--package", &mut i)?);
        } else if arg == "--verdicts-out" || arg.starts_with("--verdicts-out=") {
            verdicts_out = Some(PathBuf::from(take_val("--verdicts-out", &mut i)?));
        } else if arg == "--network" || arg.starts_with("--network=") {
            network = take_val("--network", &mut i)?;
        } else if arg == "--skip-nanoda" {
            skip_nanoda = true;
            i += 1;
        } else {
            bail!("unexpected argument `{arg}`\n\n{USAGE}");
        }
    }

    if network != "mainnet" {
        let canonical_swarm_proofs_root = root.join("proofs").join("SwarmProofs");
        promote_pipeline::assert_dry_run_isolation_for_network(
            &network,
            Some(&package_root),
            &package_root.join("registry.json"),
            &canonical_swarm_proofs_root,
        )?;
    }

    Ok(promote_pipeline::AuditStagedArgs {
        staged_candidates_dir: staged_dir
            .ok_or_else(|| anyhow::anyhow!("--staged-dir is required\n\n{USAGE}"))?,
        package_root,
        verdicts_out: verdicts_out
            .ok_or_else(|| anyhow::anyhow!("--verdicts-out is required\n\n{USAGE}"))?,
        network,
        skip_nanoda,
    })
}

fn parse_frozen_defs_append_only_args(args: &[String]) -> Result<(PathBuf, Option<PathBuf>)> {
    let root = workspace_root()?;
    let mut package_root = root.join("proofs").join("SwarmProofs");
    let mut base_registry: Option<PathBuf> = None;

    let mut i = 0usize;
    while i < args.len() {
        let arg = args[i].as_str();
        let take_val = |flag: &str, idx: &mut usize| -> Result<String> {
            if let Some(eq_val) = arg.strip_prefix(&format!("{flag}=")) {
                ensure!(!eq_val.is_empty(), "{flag} requires a non-empty value");
                *idx += 1;
                return Ok(eq_val.to_string());
            }
            let val = args
                .get(*idx + 1)
                .ok_or_else(|| anyhow::anyhow!("missing value for {flag}"))?;
            ensure!(!val.is_empty(), "{flag} requires a non-empty value");
            *idx += 2;
            Ok(val.clone())
        };
        if arg == "--base-registry" || arg.starts_with("--base-registry=") {
            base_registry = Some(PathBuf::from(take_val("--base-registry", &mut i)?));
        } else if arg == "--package" || arg.starts_with("--package=") {
            package_root = PathBuf::from(take_val("--package", &mut i)?);
        } else {
            bail!("unexpected argument `{arg}`\n\n{USAGE}");
        }
    }
    Ok((package_root, base_registry))
}

fn parse_apply_verified_args(args: &[String]) -> Result<promote_pipeline::ApplyVerifiedArgs> {
    let root = workspace_root()?;
    let mut staged_dir: Option<PathBuf> = None;
    let mut verdicts_path: Option<PathBuf> = None;
    let mut package_root = root.join("proofs").join("SwarmProofs");
    let mut registry_path: Option<PathBuf> = None;
    let mut rejected_file: Option<PathBuf> = None;
    let mut public_base_url =
        String::from(fetch_promotion_candidates::DEFAULT_PUBLIC_REGISTRY_BASE_URL);

    let mut i = 0usize;
    while i < args.len() {
        let arg = args[i].as_str();
        let take_val = |flag: &str, idx: &mut usize| -> Result<String> {
            if let Some(eq_val) = arg.strip_prefix(&format!("{flag}=")) {
                ensure!(!eq_val.is_empty(), "{flag} requires a non-empty value");
                *idx += 1;
                return Ok(eq_val.to_string());
            }
            let val = args
                .get(*idx + 1)
                .ok_or_else(|| anyhow::anyhow!("missing value for {flag}"))?;
            ensure!(!val.is_empty(), "{flag} requires a non-empty value");
            *idx += 2;
            Ok(val.clone())
        };
        if arg == "--staged-dir" || arg.starts_with("--staged-dir=") {
            staged_dir = Some(PathBuf::from(take_val("--staged-dir", &mut i)?));
        } else if arg == "--verdicts" || arg.starts_with("--verdicts=") {
            verdicts_path = Some(PathBuf::from(take_val("--verdicts", &mut i)?));
        } else if arg == "--package" || arg.starts_with("--package=") {
            package_root = PathBuf::from(take_val("--package", &mut i)?);
        } else if arg == "--registry" || arg.starts_with("--registry=") {
            registry_path = Some(PathBuf::from(take_val("--registry", &mut i)?));
        } else if arg == "--rejected-file" || arg.starts_with("--rejected-file=") {
            rejected_file = Some(PathBuf::from(take_val("--rejected-file", &mut i)?));
        } else if arg == "--public-base-url" || arg.starts_with("--public-base-url=") {
            public_base_url = take_val("--public-base-url", &mut i)?;
        } else {
            bail!("unexpected argument `{arg}`\n\n{USAGE}");
        }
    }

    let reg = registry_path.unwrap_or_else(|| package_root.join("registry.json"));
    let rej = rejected_file.unwrap_or_else(|| package_root.join("rejected-candidates.json"));
    Ok(promote_pipeline::ApplyVerifiedArgs {
        staged_candidates_dir: staged_dir
            .ok_or_else(|| anyhow::anyhow!("--staged-dir is required\n\n{USAGE}"))?,
        verdicts_path: verdicts_path
            .ok_or_else(|| anyhow::anyhow!("--verdicts is required\n\n{USAGE}"))?,
        registry_path: reg,
        package_root,
        rejected_file: rej,
        public_base_url,
    })
}

fn parse_promote_pipeline_args(args: &[String]) -> Result<promote_pipeline::PromotePipelineArgs> {
    let mut filtered = Vec::with_capacity(args.len());
    let mut skip_nanoda = false;
    for a in args {
        if a == "--skip-nanoda" {
            skip_nanoda = true;
        } else {
            filtered.push(a.clone());
        }
    }
    let fetch = parse_fetch_candidates_args(&filtered, false)?;
    ensure!(
        fetch.dry_run_registry.is_some(),
        "promote-pipeline requires --dry-run-registry <tmp_dir>\n\n{USAGE}"
    );
    Ok(promote_pipeline::PromotePipelineArgs { fetch, skip_nanoda })
}

#[cfg(test)]
mod cli_tests {
    use super::*;

    #[test]
    fn sf4_parse_audit_staged_args_rejects_devnet_against_default_or_public_package() {
        let default_pkg_args: Vec<String> = [
            "--staged-dir",
            "/tmp/sf4-cands",
            "--verdicts-out",
            "/tmp/sf4-verdicts.json",
            "--network",
            "devnet",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let err = parse_audit_staged_args(&default_pkg_args)
            .unwrap_err()
            .to_string();
        assert!(err.contains("devnet proofs never enter the public registry"));

        let isolated_args: Vec<String> = [
            "--staged-dir",
            "/tmp/sf4-cands",
            "--verdicts-out",
            "/tmp/sf4-verdicts.json",
            "--network",
            "devnet",
            "--package",
            "/tmp/sf4-isolated-dry-run/SwarmProofs",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let ok = parse_audit_staged_args(&isolated_args).unwrap();
        assert_eq!(ok.network, "devnet");
    }
}
