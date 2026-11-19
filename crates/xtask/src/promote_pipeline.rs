use crate::fetch_promotion_candidates::{
    fetch_promotion_candidates_with, load_rejected_candidates_manifest,
    save_rejected_candidates_manifest, FetchCandidatesArgs, PersistedRejectionEntry,
    DEFAULT_PUBLIC_REGISTRY_BASE_URL,
};
use crate::http::{gcloud_access_token, CurlTransport, HttpTransport};
use crate::proof_audit::{
    check_proof_axiom_audit, check_proof_sources, prepare_proof_promotion_audit,
};
use anyhow::{bail, ensure, Context, Result};
use proof_registry::{
    autopromote_candidate, preview_autopromote_manifest, validate_frozen_library_modules,
    validate_registry, PromotionCandidate, RegistryManifest,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const AUDIT_VERDICTS_SCHEMA: &str = "swarm.lean-audit-verdicts/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditRejectedCandidate {
    pub task_id: String,
    pub artifact_sha256: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditVerdictsManifest {
    pub schema: String,
    pub network: String,
    pub passed_task_ids: Vec<String>,
    pub rejected: Vec<AuditRejectedCandidate>,
}

#[derive(Debug, Clone)]
pub struct AuditStagedArgs {
    pub staged_candidates_dir: PathBuf,
    pub package_root: PathBuf,
    pub verdicts_out: PathBuf,
    pub network: String,
    pub skip_nanoda: bool,
}

#[derive(Debug, Clone)]
pub struct ApplyVerifiedArgs {
    pub staged_candidates_dir: PathBuf,
    pub verdicts_path: PathBuf,
    pub registry_path: PathBuf,
    pub package_root: PathBuf,
    pub rejected_file: PathBuf,
    pub public_base_url: String,
}

#[derive(Debug, Clone)]
pub struct PromotePipelineArgs {
    pub fetch: FetchCandidatesArgs,
    pub skip_nanoda: bool,
}

fn canonicalize_existing_or_parent(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return std::fs::canonicalize(path)
            .with_context(|| format!("failed canonicalizing {}", path.display()));
    }
    let mut cur = path;
    let mut tail: Vec<&std::ffi::OsStr> = Vec::new();
    while !cur.exists() {
        let Some(name) = cur.file_name() else {
            break;
        };
        tail.push(name);
        cur = cur.parent().unwrap_or_else(|| Path::new("."));
    }
    let base = if cur.as_os_str().is_empty() {
        std::env::current_dir().context("failed reading current_dir")?
    } else {
        std::fs::canonicalize(cur)
            .with_context(|| format!("failed canonicalizing {}", cur.display()))?
    };
    let mut out = base;
    for part in tail.into_iter().rev() {
        out.push(part);
    }
    Ok(out)
}

pub fn assert_dry_run_isolation_for_network(
    network: &str,
    dry_run_registry: Option<&Path>,
    registry_path: &Path,
    public_swarm_proofs_dir: &Path,
) -> Result<()> {
    let pub_canon = canonicalize_existing_or_parent(public_swarm_proofs_dir)?;
    if let Some(dry_dir) = dry_run_registry {
        let dry_canon = canonicalize_existing_or_parent(dry_dir)?;
        if dry_canon.starts_with(&pub_canon) || pub_canon.starts_with(&dry_canon) {
            bail!(
                "non-mainnet network requires --dry-run-registry <tmp_path> outside proofs/SwarmProofs; devnet proofs never enter the public registry"
            );
        }
    } else if network != "mainnet" {
        bail!(
            "non-mainnet network requires --dry-run-registry <tmp_path> outside proofs/SwarmProofs; devnet proofs never enter the public registry"
        );
    }

    if network != "mainnet" {
        let reg_canon = canonicalize_existing_or_parent(registry_path)?;
        if reg_canon.starts_with(&pub_canon) {
            bail!(
                "non-mainnet network requires --dry-run-registry <tmp_path> outside proofs/SwarmProofs; devnet proofs never enter the public registry"
            );
        }
    }
    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)
        .with_context(|| format!("failed creating directory {}", dst.display()))?;
    for entry in
        std::fs::read_dir(src).with_context(|| format!("failed reading {}", src.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let target = dst.join(entry.file_name());
        let ft = entry.file_type()?;
        if ft.is_dir() {
            copy_dir_recursive(&path, &target)?;
        } else if ft.is_file() {
            std::fs::copy(&path, &target).with_context(|| {
                format!("failed copying {} -> {}", path.display(), target.display())
            })?;
        }
    }
    Ok(())
}

/// Stage an isolated copy of `proofs/SwarmProofs` under `<dry_run_root>/SwarmProofs`,
/// symlinking `.lake/packages` (if present) so `lake build` reuses cached Mathlib/Zeta23
/// without ever writing inside the repository's `proofs/SwarmProofs`.
pub fn stage_dry_run_swarm_proofs_copy(
    public_swarm_proofs_dir: &Path,
    dry_run_root: &Path,
) -> Result<PathBuf> {
    let staged_pkg = dry_run_root.join("SwarmProofs");
    std::fs::create_dir_all(&staged_pkg)
        .with_context(|| format!("failed creating {}", staged_pkg.display()))?;

    for file_name in [
        "registry.json",
        "rejected-candidates.json",
        "environment-lock.json",
        "lakefile.toml",
        "lean-toolchain",
        "lake-manifest.json",
        "SwarmProofs.lean",
    ] {
        let src = public_swarm_proofs_dir.join(file_name);
        let dst = staged_pkg.join(file_name);
        if src.is_file() && !dst.is_file() {
            std::fs::copy(&src, &dst).with_context(|| {
                format!("failed copying {} -> {}", src.display(), dst.display())
            })?;
        }
    }

    let src_lib = public_swarm_proofs_dir.join("SwarmProofs");
    let dst_lib = staged_pkg.join("SwarmProofs");
    if src_lib.is_dir() && !dst_lib.is_dir() {
        copy_dir_recursive(&src_lib, &dst_lib)?;
    }

    #[cfg(unix)]
    {
        let src_pkgs = public_swarm_proofs_dir.join(".lake").join("packages");
        let dst_lake = staged_pkg.join(".lake");
        let dst_pkgs = dst_lake.join("packages");
        if src_pkgs.is_dir() && !dst_pkgs.exists() {
            std::fs::create_dir_all(&dst_lake)?;
            let canon_pkgs = std::fs::canonicalize(&src_pkgs)?;
            std::os::unix::fs::symlink(&canon_pkgs, &dst_pkgs).with_context(|| {
                format!(
                    "failed symlinking {} -> {}",
                    canon_pkgs.display(),
                    dst_pkgs.display()
                )
            })?;
        }
    }

    Ok(staged_pkg)
}

fn is_strict_task_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'.' | b'_' | b'-'))
}

fn is_lower_hex_64(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn sanitize_reason(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_graphic() || c == ' ' {
                c
            } else {
                ' '
            }
        })
        .take(512)
        .collect();
    cleaned.trim().to_string()
}

pub fn parse_autopromote_plan(plan_path: &Path) -> Result<Vec<(PathBuf, Option<String>)>> {
    if !plan_path.is_file() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(plan_path)
        .with_context(|| format!("failed reading {}", plan_path.display()))?;
    let base_dir = plan_path.parent().unwrap_or_else(|| Path::new("."));
    let mut items = Vec::new();
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let (raw_path, raw_alias) = match line.split_once('\t') {
            Some((p, a)) => (p.trim(), a.trim()),
            None => (line.trim(), ""),
        };
        if raw_path.is_empty() {
            continue;
        }
        let candidate_path = {
            let p = PathBuf::from(raw_path);
            if p.is_file() {
                p
            } else if let Some(fname) = p.file_name() {
                base_dir.join(fname)
            } else {
                p
            }
        };
        let alias = if raw_alias.is_empty() {
            None
        } else {
            Some(raw_alias.to_string())
        };
        items.push((candidate_path, alias));
    }
    Ok(items)
}

fn restore_audit_umbrella(package_root: &Path) {
    let backup = package_root.join("lakefile.toml.original");
    let lakefile = package_root.join("lakefile.toml");
    if backup.is_file() {
        let _ = std::fs::rename(&backup, &lakefile);
    }
    let _ = std::fs::remove_file(package_root.join("PromotionAudit.lean"));
}

fn list_generated_files(gen_dir: &Path) -> BTreeSet<PathBuf> {
    let Ok(entries) = std::fs::read_dir(gen_dir) else {
        return BTreeSet::new();
    };
    entries.flatten().map(|e| e.path()).collect()
}

pub fn audit_staged_candidates_with<F>(
    repo_root: &Path,
    args: &AuditStagedArgs,
    mut single_candidate_auditor: F,
) -> Result<AuditVerdictsManifest>
where
    F: FnMut(&Path, &Path, &PromotionCandidate, bool) -> Result<()>,
{
    let plan_path = args.staged_candidates_dir.join("autopromote-plan.tsv");
    let plan = parse_autopromote_plan(&plan_path)?;
    let reg_path = args.package_root.join("registry.json");
    let gen_dir = args.package_root.join("SwarmProofs").join("Generated");
    std::fs::create_dir_all(&gen_dir)?;

    let mut verdicts = AuditVerdictsManifest {
        schema: AUDIT_VERDICTS_SCHEMA.to_string(),
        network: args.network.clone(),
        passed_task_ids: Vec::new(),
        rejected: Vec::new(),
    };

    for (cand_path, alias_opt) in plan {
        let cand_bytes = std::fs::read(&cand_path)
            .with_context(|| format!("failed reading staged candidate {}", cand_path.display()))?;
        let cand: PromotionCandidate = serde_json::from_slice(&cand_bytes)
            .with_context(|| format!("invalid PromotionCandidate JSON {}", cand_path.display()))?;

        let pre_reg_bytes = std::fs::read(&reg_path)
            .with_context(|| format!("failed reading {}", reg_path.display()))?;
        let pre_gen_files = list_generated_files(&gen_dir);

        let step_res: Result<()> = (|| {
            let mut reg: RegistryManifest =
                serde_json::from_slice(&pre_reg_bytes).context("invalid registry.json")?;
            if let Some(ref alias_key) = alias_opt {
                let (preview, _) =
                    preview_autopromote_manifest(&cand, &reg, DEFAULT_PUBLIC_REGISTRY_BASE_URL)
                        .map_err(|e| anyhow::anyhow!("preview_autopromote_manifest failed: {e}"))?;
                if let Some(existing_id) = reg.aliases.get(alias_key) {
                    ensure!(
                        existing_id == &preview.module_id,
                        "refusing to overwrite existing alias `{alias_key}`"
                    );
                }
            }
            let manifest = autopromote_candidate(
                &cand,
                &mut reg,
                &args.package_root,
                DEFAULT_PUBLIC_REGISTRY_BASE_URL,
            )
            .map_err(|e| anyhow::anyhow!("autopromote_candidate failed: {e}"))?;
            if let Some(alias_key) = alias_opt {
                reg.aliases.insert(alias_key, manifest.module_id);
            }
            validate_registry(&reg)
                .map_err(|e| anyhow::anyhow!("validate_registry failed: {e}"))?;
            validate_frozen_library_modules(&args.package_root, &reg)
                .map_err(|e| anyhow::anyhow!("validate_frozen_library_modules failed: {e}"))?;

            let mut updated_bytes = serde_json::to_vec_pretty(&reg)?;
            updated_bytes.push(b'\n');
            std::fs::write(&reg_path, updated_bytes)?;

            single_candidate_auditor(repo_root, &args.package_root, &cand, args.skip_nanoda)
        })();

        restore_audit_umbrella(&args.package_root);

        match step_res {
            Ok(()) => {
                verdicts.passed_task_ids.push(cand.task_id);
            }
            Err(err) => {
                // Roll back registry.json and any newly created Generated/*.lean files so one broken
                // candidate never blocks independent healthy candidates in the same batch!
                std::fs::write(&reg_path, &pre_reg_bytes)?;
                let post_gen_files = list_generated_files(&gen_dir);
                for added in post_gen_files.difference(&pre_gen_files) {
                    let _ = std::fs::remove_file(added);
                }
                verdicts.rejected.push(AuditRejectedCandidate {
                    task_id: cand.task_id,
                    artifact_sha256: cand.artifact_sha256,
                    reason: sanitize_reason(&format!("{err:#}")),
                });
            }
        }
    }

    if let Some(parent) = args.verdicts_out.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let mut out_bytes = serde_json::to_vec_pretty(&verdicts)?;
    out_bytes.push(b'\n');
    std::fs::write(&args.verdicts_out, out_bytes)
        .with_context(|| format!("failed writing {}", args.verdicts_out.display()))?;

    Ok(verdicts)
}

pub fn real_lean_single_candidate_auditor(
    repo_root: &Path,
    package_root: &Path,
    cand: &PromotionCandidate,
    skip_nanoda: bool,
) -> Result<()> {
    prepare_proof_promotion_audit(package_root)?;
    check_proof_sources(repo_root, package_root)?;

    let build_status = Command::new("lake")
        .current_dir(package_root)
        .stdin(Stdio::null())
        .args(["build", "SwarmProofs", "PromotionAudit"])
        .status()
        .context("failed executing `lake build SwarmProofs PromotionAudit`")?;
    ensure!(
        build_status.success(),
        "`lake build SwarmProofs PromotionAudit` failed for task {}",
        cand.task_id
    );

    if !skip_nanoda {
        let single_cand_dir = package_root
            .join(".lake")
            .join("single-candidate-stability");
        let _ = std::fs::remove_dir_all(&single_cand_dir);
        std::fs::create_dir_all(&single_cand_dir)?;
        std::fs::write(
            single_cand_dir.join("candidate.json"),
            serde_json::to_vec_pretty(cand)?,
        )?;
        let report_dir = package_root.join(".lake").join("proof-audit");
        let nanoda_script = repo_root.join("scripts").join("check-proof-nanoda.sh");
        let nanoda_status = Command::new("bash")
            .current_dir(repo_root)
            .stdin(Stdio::null())
            .arg(&nanoda_script)
            .arg(package_root)
            .arg("PromotionAudit")
            .arg(&report_dir)
            .arg("")
            .arg(&single_cand_dir)
            .status()
            .context("failed executing scripts/check-proof-nanoda.sh")?;
        let _ = std::fs::remove_dir_all(&single_cand_dir);
        ensure!(
            nanoda_status.success(),
            "`check-proof-nanoda.sh` failed for task {}",
            cand.task_id
        );
    }

    let axiom_out = Command::new("lake")
        .current_dir(package_root)
        .stdin(Stdio::null())
        .args(["env", "lean", "PromotionAudit.lean"])
        .output()
        .context("failed executing `lake env lean PromotionAudit.lean`")?;
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&axiom_out.stdout),
        String::from_utf8_lossy(&axiom_out.stderr)
    );
    ensure!(
        axiom_out.status.success(),
        "`lake env lean PromotionAudit.lean` failed for task {}: {}",
        cand.task_id,
        combined.trim()
    );
    let axiom_file = package_root.join(".lake").join("swarm-proof-axioms.txt");
    if let Some(parent) = axiom_file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&axiom_file, combined.as_bytes())?;
    check_proof_axiom_audit(&axiom_file, Some(&package_root.join("registry.json")))?;

    Ok(())
}

pub fn run_audit_staged_candidates(repo_root: &Path, args: &AuditStagedArgs) -> Result<()> {
    let verdicts =
        audit_staged_candidates_with(repo_root, args, real_lean_single_candidate_auditor)?;
    println!("{}", serde_json::to_string_pretty(&verdicts)?);
    Ok(())
}

/// Pure-Rust reconstruction in `open-pr`: trusts ZERO Lean/registry files from `audit-untrusted-lean`!
/// Validates `audit-verdicts.json` strictly against the trusted `staged-candidates-dir` plan,
/// re-runs `autopromote_candidate` from the trusted candidate JSONs, and persists any Lean-audit
/// rejections into `rejected-candidates.json`.
pub fn apply_verified_promotions(args: &ApplyVerifiedArgs) -> Result<usize> {
    let verdicts_bytes = std::fs::read(&args.verdicts_path)
        .with_context(|| format!("failed reading {}", args.verdicts_path.display()))?;
    let verdicts: AuditVerdictsManifest =
        serde_json::from_slice(&verdicts_bytes).context("invalid audit-verdicts.json")?;
    ensure!(
        verdicts.schema == AUDIT_VERDICTS_SCHEMA,
        "unexpected audit-verdicts schema `{}`",
        verdicts.schema
    );
    ensure!(
        verdicts.network == "mainnet",
        "refusing to apply promotion verdicts for non-mainnet network `{}`",
        verdicts.network
    );

    let plan = parse_autopromote_plan(&args.staged_candidates_dir.join("autopromote-plan.tsv"))?;
    let mut trusted_by_task: BTreeMap<String, (PromotionCandidate, Option<String>)> =
        BTreeMap::new();
    for (cand_path, alias_opt) in &plan {
        let cand: PromotionCandidate = serde_json::from_slice(&std::fs::read(cand_path)?)
            .with_context(|| format!("invalid staged candidate {}", cand_path.display()))?;
        trusted_by_task.insert(cand.task_id.clone(), (cand, alias_opt.clone()));
    }

    let mut passed_set = BTreeSet::new();
    for tid in &verdicts.passed_task_ids {
        ensure!(
            is_strict_task_id(tid),
            "invalid task_id `{tid}` in audit-verdicts.json"
        );
        ensure!(
            trusted_by_task.contains_key(tid),
            "audit-verdicts.json references unknown passed task_id `{tid}` not in trusted staged plan"
        );
        ensure!(
            passed_set.insert(tid.clone()),
            "duplicate passed task_id `{tid}` in audit-verdicts.json"
        );
    }

    for rej in &verdicts.rejected {
        ensure!(
            is_strict_task_id(&rej.task_id),
            "invalid rejected task_id `{}` in audit-verdicts.json",
            rej.task_id
        );
        ensure!(
            is_lower_hex_64(&rej.artifact_sha256),
            "invalid rejected artifact_sha256 `{}` in audit-verdicts.json",
            rej.artifact_sha256
        );
        ensure!(
            !passed_set.contains(&rej.task_id),
            "task `{}` listed as both passed and rejected in audit-verdicts.json",
            rej.task_id
        );
        let (trusted_cand, _) = trusted_by_task.get(&rej.task_id).ok_or_else(|| {
            anyhow::anyhow!(
                "audit-verdicts.json references unknown rejected task_id `{}`",
                rej.task_id
            )
        })?;
        ensure!(
            trusted_cand.artifact_sha256 == rej.artifact_sha256,
            "rejected artifact_sha256 mismatch against trusted staged candidate for `{}`",
            rej.task_id
        );
    }

    let mut reg: RegistryManifest = serde_json::from_slice(
        &std::fs::read(&args.registry_path)
            .with_context(|| format!("failed reading {}", args.registry_path.display()))?,
    )
    .context("invalid registry.json")?;

    let mut promoted_count = 0usize;
    for (cand_path, alias_opt) in plan {
        let cand: PromotionCandidate = serde_json::from_slice(&std::fs::read(&cand_path)?)?;
        if !passed_set.contains(&cand.task_id) {
            continue;
        }
        if let Some(ref alias_key) = alias_opt {
            let (preview, _) = preview_autopromote_manifest(&cand, &reg, &args.public_base_url)
                .map_err(|e| anyhow::anyhow!("preview_autopromote_manifest failed: {e}"))?;
            if let Some(existing_id) = reg.aliases.get(alias_key) {
                ensure!(
                    existing_id == &preview.module_id,
                    "refusing to overwrite existing alias `{alias_key}`"
                );
            }
        }
        let manifest =
            autopromote_candidate(&cand, &mut reg, &args.package_root, &args.public_base_url)
                .map_err(|e| anyhow::anyhow!("autopromote_candidate failed: {e}"))?;
        if let Some(alias_key) = alias_opt {
            reg.aliases.insert(alias_key, manifest.module_id);
        }
        promoted_count += 1;
    }

    validate_registry(&reg).map_err(|e| anyhow::anyhow!("validate_registry failed: {e}"))?;
    validate_frozen_library_modules(&args.package_root, &reg)
        .map_err(|e| anyhow::anyhow!("validate_frozen_library_modules failed: {e}"))?;

    if promoted_count > 0 {
        let mut reg_bytes = serde_json::to_vec_pretty(&reg)?;
        reg_bytes.push(b'\n');
        std::fs::write(&args.registry_path, reg_bytes)?;
    }

    if !verdicts.rejected.is_empty() {
        let mut rej_manifest = load_rejected_candidates_manifest(Some(&args.rejected_file))?;
        for item in &verdicts.rejected {
            let key = format!("{}:{}", verdicts.network, item.task_id);
            rej_manifest.rejected.insert(
                key,
                PersistedRejectionEntry {
                    artifact_sha256: item.artifact_sha256.clone(),
                    reason: sanitize_reason(&item.reason),
                },
            );
        }
        save_rejected_candidates_manifest(&args.rejected_file, &rej_manifest)?;
    }

    Ok(promoted_count)
}

pub fn run_promote_pipeline_with<T, F>(
    repo_root: &Path,
    transport: &T,
    args: &PromotePipelineArgs,
    gcp_token: Option<&str>,
    auditor: F,
) -> Result<AuditVerdictsManifest>
where
    T: HttpTransport,
    F: FnMut(&Path, &Path, &PromotionCandidate, bool) -> Result<()>,
{
    let public_pkg = repo_root.join("proofs").join("SwarmProofs");
    let dry_root = args.fetch.dry_run_registry.as_deref().ok_or_else(|| {
        anyhow::anyhow!(
            "promote-pipeline requires --dry-run-registry <tmp_path> outside proofs/SwarmProofs"
        )
    })?;
    assert_dry_run_isolation_for_network(
        &args.fetch.network,
        Some(dry_root),
        &dry_root.join("SwarmProofs").join("registry.json"),
        &public_pkg,
    )?;

    let staged_pkg = stage_dry_run_swarm_proofs_copy(&public_pkg, dry_root)?;
    let candidates_dir = dry_root.join("candidates");
    let verdicts_path = dry_root.join("audit-verdicts.json");

    let mut fetch_args = args.fetch.clone();
    fetch_args.registry_path = staged_pkg.join("registry.json");
    fetch_args.rejected_file = Some(staged_pkg.join("rejected-candidates.json"));
    fetch_args.out_dir = candidates_dir.clone();

    let fetch_report = fetch_promotion_candidates_with(transport, &fetch_args, gcp_token)?;
    if fetch_report.candidates.is_empty() {
        let empty = AuditVerdictsManifest {
            schema: AUDIT_VERDICTS_SCHEMA.to_string(),
            network: fetch_args.network,
            passed_task_ids: Vec::new(),
            rejected: Vec::new(),
        };
        let mut bytes = serde_json::to_vec_pretty(&empty)?;
        bytes.push(b'\n');
        std::fs::write(&verdicts_path, bytes)?;
        return Ok(empty);
    }

    audit_staged_candidates_with(
        repo_root,
        &AuditStagedArgs {
            staged_candidates_dir: candidates_dir,
            package_root: staged_pkg,
            verdicts_out: verdicts_path,
            network: fetch_args.network,
            skip_nanoda: args.skip_nanoda,
        },
        auditor,
    )
}

pub fn run_promote_pipeline(repo_root: &Path, args: &PromotePipelineArgs) -> Result<()> {
    let token = gcloud_access_token().ok();
    let transport = CurlTransport;
    let verdicts = run_promote_pipeline_with(
        repo_root,
        &transport,
        args,
        token.as_deref(),
        real_lean_single_candidate_auditor,
    )?;
    println!("{}", serde_json::to_string_pretty(&verdicts)?);
    ensure!(
        verdicts.rejected.is_empty() && !verdicts.passed_task_ids.is_empty(),
        "promote-pipeline finished with {} passed and {} rejected candidate(s)",
        verdicts.passed_task_ids.len(),
        verdicts.rejected.len()
    );
    Ok(())
}
