use crate::fetch_promotion_candidates::{
    fetch_promotion_candidates_with, load_rejected_candidates_manifest,
    save_rejected_candidates_manifest, FetchCandidatesArgs, PersistedRejectionEntry,
    DEFAULT_PUBLIC_REGISTRY_BASE_URL,
};
use crate::http::{gcloud_access_token, CurlTransport, HttpTransport};
use crate::proof_audit::{
    check_proof_axiom_audit, check_proof_sources_classified, prepare_proof_promotion_audit,
    SourcePolicyCheckError,
};
use anyhow::{bail, ensure, Context, Result};
use proof_registry::{
    autopromote_candidate, preview_autopromote_manifest, validate_frozen_library_modules,
    validate_registry, PromotionCandidate, RegistryManifest,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

pub const AUDIT_VERDICTS_SCHEMA: &str = "swarm.lean-audit-verdicts/v1";
pub const LEAN4EXPORT_REVISION: &str = "9fb131bb100eb32ccf6836f14e4f8328d13b6792";
pub const NANODA_REVISION: &str = "3a2407216ee84a75f9e1aead6803d0578be06ae7";
pub const DEFAULT_AUDIT_TOOLS_DIR: &str = "/tmp/swarm-audit-tools";

pub const ALLOWED_AUDIT_REJECTION_CODES: &[&str] = &[
    "autopromote_policy_failed",
    "registry_validation_failed",
    "source_policy_failed",
    "lean_compile_failed",
    "nanoda_or_expr_stability_failed",
    "axiom_audit_failed",
];

pub const INFRA_OUTPUT_MARKERS: &[&str] = &[
    "No space left on device",
    "out of memory",
    "Cannot allocate memory",
    "fatal: unable to access",
    "Could not resolve host",
    "Connection timed out",
    "TLS handshake",
];

#[derive(Debug)]
pub enum StepOutcome {
    Passed,
    CandidateRejected { code: &'static str },
    InfraError(anyhow::Error),
}

#[derive(Debug, Clone)]
pub struct AuditToolPaths {
    pub lean4export_bin: PathBuf,
    pub nanoda_bin: PathBuf,
}

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

pub fn contains_infra_marker(text: &str) -> bool {
    if INFRA_OUTPUT_MARKERS.iter().any(|m| text.contains(m)) {
        return true;
    }
    let lower = text.to_ascii_lowercase();
    lower.contains("no space left on device")
        || lower.contains("out of memory")
        || lower.contains("cannot allocate memory")
}

pub fn classify_process_output(
    step_label: &str,
    output_res: std::io::Result<Output>,
    reject_code: &'static str,
) -> StepOutcome {
    let out = match output_res {
        Ok(o) => o,
        Err(io_err) => {
            return StepOutcome::InfraError(
                anyhow::Error::new(io_err).context(format!("failed spawning `{step_label}`")),
            );
        }
    };
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    match out.status.code() {
        Some(0) => StepOutcome::Passed,
        None => StepOutcome::InfraError(anyhow::anyhow!(
            "`{step_label}` terminated by signal: {}",
            sanitize_reason(&combined)
        )),
        Some(code) if !(1..=124).contains(&code) => StepOutcome::InfraError(anyhow::anyhow!(
            "`{step_label}` exited with infrastructure status {code}: {}",
            sanitize_reason(&combined)
        )),
        Some(code) => {
            if contains_infra_marker(&combined) {
                StepOutcome::InfraError(anyhow::anyhow!(
                    "`{step_label}` hit infrastructure failure (exit {code}): {}",
                    sanitize_reason(&combined)
                ))
            } else {
                StepOutcome::CandidateRejected { code: reject_code }
            }
        }
    }
}

fn is_executable_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn run_checked_setup_cmd(cmd: &mut Command, label: &str) -> Result<()> {
    let out = cmd
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("failed spawning `{label}`"))?;
    if out.status.success() {
        return Ok(());
    }
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    bail!(
        "`{label}` failed (status {:?}): {}",
        out.status.code(),
        sanitize_reason(&combined)
    );
}

/// Build or locate pinned `lean4export` and `nanoda_bin` ONCE before the candidate audit loop.
/// Honors pre-built `LEAN4EXPORT_BIN` and `NANODA_BIN` environment variables when set and executable.
pub fn prepare_audit_tools(package_root: &Path, tools_root: &Path) -> Result<AuditToolPaths> {
    let env_exporter = std::env::var_os("LEAN4EXPORT_BIN").filter(|v| !v.is_empty());
    let env_nanoda = std::env::var_os("NANODA_BIN").filter(|v| !v.is_empty());

    let lean4export_bin = if let Some(raw) = env_exporter {
        let p = PathBuf::from(raw);
        ensure!(
            is_executable_file(&p),
            "LEAN4EXPORT_BIN is set but not an executable file: {}",
            p.display()
        );
        std::fs::canonicalize(&p)
            .with_context(|| format!("failed canonicalizing {}", p.display()))?
    } else {
        std::fs::create_dir_all(tools_root)
            .with_context(|| format!("failed creating {}", tools_root.display()))?;
        let repo_dir = tools_root.join("lean4export");
        if repo_dir.exists() {
            std::fs::remove_dir_all(&repo_dir)
                .with_context(|| format!("failed cleaning {}", repo_dir.display()))?;
        }
        run_checked_setup_cmd(
            Command::new("git").args([
                "clone",
                "--quiet",
                "https://github.com/leanprover/lean4export.git",
                &repo_dir.to_string_lossy(),
            ]),
            "git clone lean4export",
        )?;
        run_checked_setup_cmd(
            Command::new("git").arg("-C").arg(&repo_dir).args([
                "checkout",
                "--quiet",
                LEAN4EXPORT_REVISION,
            ]),
            "git checkout lean4export",
        )?;
        let toolchain_src = package_root.join("lean-toolchain");
        let toolchain_dst = repo_dir.join("lean-toolchain");
        std::fs::copy(&toolchain_src, &toolchain_dst).with_context(|| {
            format!(
                "failed copying {} -> {}",
                toolchain_src.display(),
                toolchain_dst.display()
            )
        })?;
        run_checked_setup_cmd(
            Command::new("lake").current_dir(&repo_dir).arg("build"),
            "lake build lean4export",
        )?;
        let built = repo_dir
            .join(".lake")
            .join("build")
            .join("bin")
            .join("lean4export");
        ensure!(
            is_executable_file(&built),
            "built lean4export binary missing or not executable at {}",
            built.display()
        );
        std::fs::canonicalize(&built)?
    };

    let nanoda_bin = if let Some(raw) = env_nanoda {
        let p = PathBuf::from(raw);
        ensure!(
            is_executable_file(&p),
            "NANODA_BIN is set but not an executable file: {}",
            p.display()
        );
        std::fs::canonicalize(&p)
            .with_context(|| format!("failed canonicalizing {}", p.display()))?
    } else {
        std::fs::create_dir_all(tools_root)
            .with_context(|| format!("failed creating {}", tools_root.display()))?;
        let repo_dir = tools_root.join("nanoda");
        if repo_dir.exists() {
            std::fs::remove_dir_all(&repo_dir)
                .with_context(|| format!("failed cleaning {}", repo_dir.display()))?;
        }
        run_checked_setup_cmd(
            Command::new("git").args([
                "clone",
                "--quiet",
                "https://github.com/ammkrn/nanoda_lib.git",
                &repo_dir.to_string_lossy(),
            ]),
            "git clone nanoda_lib",
        )?;
        run_checked_setup_cmd(
            Command::new("git").arg("-C").arg(&repo_dir).args([
                "checkout",
                "--quiet",
                NANODA_REVISION,
            ]),
            "git checkout nanoda_lib",
        )?;
        run_checked_setup_cmd(
            Command::new("cargo")
                .current_dir(&repo_dir)
                .args(["build", "--release", "--locked"]),
            "cargo build nanoda_bin",
        )?;
        let built = repo_dir.join("target").join("release").join("nanoda_bin");
        ensure!(
            is_executable_file(&built),
            "built nanoda_bin binary missing or not executable at {}",
            built.display()
        );
        std::fs::canonicalize(&built)?
    };

    Ok(AuditToolPaths {
        lean4export_bin,
        nanoda_bin,
    })
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

fn remove_entry_path(path: &Path, ft: &std::fs::FileType) -> Result<()> {
    if ft.is_dir() {
        std::fs::remove_dir_all(path)
            .with_context(|| format!("failed removing directory {}", path.display()))?;
    } else {
        std::fs::remove_file(path)
            .with_context(|| format!("failed removing file {}", path.display()))?;
    }
    Ok(())
}

/// Stage a clean isolated copy of `proofs/SwarmProofs` under `<dry_run_root>/SwarmProofs`,
/// wiping any stale files/Generated modules from prior runs while preserving `.lake/packages`
/// (if symlinked) so `lake build` reuses cached Mathlib/Zeta23 without touching `proofs/SwarmProofs`.
pub fn stage_dry_run_swarm_proofs_copy(
    public_swarm_proofs_dir: &Path,
    dry_run_root: &Path,
) -> Result<PathBuf> {
    let staged_pkg = dry_run_root.join("SwarmProofs");
    if staged_pkg.is_dir() {
        for entry in std::fs::read_dir(&staged_pkg)
            .with_context(|| format!("failed reading {}", staged_pkg.display()))?
        {
            let entry = entry?;
            if entry.file_name() == ".lake" {
                continue;
            }
            remove_entry_path(&entry.path(), &entry.file_type()?)?;
        }
        let dst_lake = staged_pkg.join(".lake");
        if dst_lake.is_dir() {
            for entry in std::fs::read_dir(&dst_lake)
                .with_context(|| format!("failed reading {}", dst_lake.display()))?
            {
                let entry = entry?;
                if entry.file_name() == "packages" {
                    continue;
                }
                remove_entry_path(&entry.path(), &entry.file_type()?)?;
            }
        }
    } else {
        std::fs::create_dir_all(&staged_pkg)
            .with_context(|| format!("failed creating {}", staged_pkg.display()))?;
    }

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
        if src.is_file() {
            std::fs::copy(&src, &dst).with_context(|| {
                format!("failed copying {} -> {}", src.display(), dst.display())
            })?;
        }
    }

    let src_lib = public_swarm_proofs_dir.join("SwarmProofs");
    let dst_lib = staged_pkg.join("SwarmProofs");
    if src_lib.is_dir() {
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
    F: FnMut(&Path, &Path, &PromotionCandidate, bool) -> StepOutcome,
{
    if args.verdicts_out.is_file() {
        std::fs::remove_file(&args.verdicts_out)
            .with_context(|| format!("failed removing stale {}", args.verdicts_out.display()))?;
    }
    if args.network != "mainnet" {
        let canonical_swarm_proofs_root = repo_root.join("proofs").join("SwarmProofs");
        assert_dry_run_isolation_for_network(
            &args.network,
            Some(&args.package_root),
            &args.package_root.join("registry.json"),
            &canonical_swarm_proofs_root,
        )?;
    }

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

        let step_outcome: StepOutcome = (|| {
            let mut reg: RegistryManifest = match serde_json::from_slice(&pre_reg_bytes) {
                Ok(r) => r,
                Err(e) => {
                    return StepOutcome::InfraError(
                        anyhow::Error::new(e).context("invalid baseline registry.json"),
                    );
                }
            };
            if let Some(ref alias_key) = alias_opt {
                let (preview, _) = match preview_autopromote_manifest(
                    &cand,
                    &reg,
                    DEFAULT_PUBLIC_REGISTRY_BASE_URL,
                ) {
                    Ok(v) => v,
                    Err(e) => {
                        let msg = e.to_string();
                        return if contains_infra_marker(&msg) {
                            StepOutcome::InfraError(anyhow::anyhow!(msg))
                        } else {
                            StepOutcome::CandidateRejected {
                                code: "autopromote_policy_failed",
                            }
                        };
                    }
                };
                if let Some(existing_id) = reg.aliases.get(alias_key) {
                    if existing_id != &preview.module_id {
                        return StepOutcome::CandidateRejected {
                            code: "autopromote_policy_failed",
                        };
                    }
                }
            }
            let manifest = match autopromote_candidate(
                &cand,
                &mut reg,
                &args.package_root,
                DEFAULT_PUBLIC_REGISTRY_BASE_URL,
            ) {
                Ok(m) => m,
                Err(e) => {
                    let msg = e.to_string();
                    return if contains_infra_marker(&msg) {
                        StepOutcome::InfraError(anyhow::anyhow!(msg))
                    } else {
                        StepOutcome::CandidateRejected {
                            code: "autopromote_policy_failed",
                        }
                    };
                }
            };
            if let Some(alias_key) = alias_opt {
                reg.aliases.insert(alias_key, manifest.module_id);
            }
            if let Err(e) = validate_registry(&reg) {
                let msg = e.to_string();
                return if contains_infra_marker(&msg) {
                    StepOutcome::InfraError(anyhow::anyhow!(msg))
                } else {
                    StepOutcome::CandidateRejected {
                        code: "registry_validation_failed",
                    }
                };
            }
            if let Err(e) = validate_frozen_library_modules(&args.package_root, &reg) {
                let msg = e.to_string();
                return if contains_infra_marker(&msg) {
                    StepOutcome::InfraError(anyhow::anyhow!(msg))
                } else {
                    StepOutcome::CandidateRejected {
                        code: "registry_validation_failed",
                    }
                };
            }

            let mut updated_bytes = match serde_json::to_vec_pretty(&reg) {
                Ok(b) => b,
                Err(e) => return StepOutcome::InfraError(anyhow::Error::new(e)),
            };
            updated_bytes.push(b'\n');
            if let Err(e) = std::fs::write(&reg_path, updated_bytes) {
                return StepOutcome::InfraError(
                    anyhow::Error::new(e).context("failed writing updated registry.json"),
                );
            }

            single_candidate_auditor(repo_root, &args.package_root, &cand, args.skip_nanoda)
        })();

        restore_audit_umbrella(&args.package_root);

        match step_outcome {
            StepOutcome::Passed => {
                verdicts.passed_task_ids.push(cand.task_id);
            }
            StepOutcome::CandidateRejected { code } => {
                ensure!(
                    ALLOWED_AUDIT_REJECTION_CODES.contains(&code),
                    "internal error: non-canonical audit rejection code `{code}`"
                );
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
                    reason: code.to_string(),
                });
            }
            StepOutcome::InfraError(err) => {
                let _ = std::fs::write(&reg_path, &pre_reg_bytes);
                let post_gen_files = list_generated_files(&gen_dir);
                for added in post_gen_files.difference(&pre_gen_files) {
                    let _ = std::fs::remove_file(added);
                }
                // Intentionally return Err WITHOUT writing audit-verdicts.json so transient
                // infrastructure errors never become permanent candidate rejections!
                return Err(err);
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

pub fn real_lean_single_candidate_auditor_with_tools(
    repo_root: &Path,
    package_root: &Path,
    cand: &PromotionCandidate,
    skip_nanoda: bool,
    tools: Option<&AuditToolPaths>,
) -> StepOutcome {
    if let Err(err) = prepare_proof_promotion_audit(package_root) {
        return StepOutcome::InfraError(err);
    }
    match check_proof_sources_classified(repo_root, package_root) {
        Ok(()) => {}
        Err(SourcePolicyCheckError::Infra(err)) => return StepOutcome::InfraError(err),
        Err(SourcePolicyCheckError::CandidatePolicy(_)) => {
            return StepOutcome::CandidateRejected {
                code: "source_policy_failed",
            };
        }
    }

    let build_out = Command::new("lake")
        .current_dir(package_root)
        .stdin(Stdio::null())
        .args(["build", "SwarmProofs", "PromotionAudit"])
        .output();
    match classify_process_output(
        "lake build SwarmProofs PromotionAudit",
        build_out,
        "lean_compile_failed",
    ) {
        StepOutcome::Passed => {}
        other => return other,
    }

    if !skip_nanoda {
        let single_cand_dir = package_root
            .join(".lake")
            .join("single-candidate-stability");
        let _ = std::fs::remove_dir_all(&single_cand_dir);
        if let Err(e) = std::fs::create_dir_all(&single_cand_dir) {
            return StepOutcome::InfraError(anyhow::Error::new(e));
        }
        let cand_json = match serde_json::to_vec_pretty(cand) {
            Ok(b) => b,
            Err(e) => return StepOutcome::InfraError(anyhow::Error::new(e)),
        };
        if let Err(e) = std::fs::write(single_cand_dir.join("candidate.json"), cand_json) {
            let _ = std::fs::remove_dir_all(&single_cand_dir);
            return StepOutcome::InfraError(anyhow::Error::new(e));
        }
        let report_dir = package_root.join(".lake").join("proof-audit");
        let nanoda_script = repo_root.join("scripts").join("check-proof-nanoda.sh");
        let mut nanoda_cmd = Command::new("bash");
        nanoda_cmd
            .current_dir(repo_root)
            .stdin(Stdio::null())
            .arg(&nanoda_script)
            .arg(package_root)
            .arg("PromotionAudit")
            .arg(&report_dir)
            .arg("")
            .arg(&single_cand_dir);
        if let Some(tp) = tools {
            nanoda_cmd
                .env("LEAN4EXPORT_BIN", &tp.lean4export_bin)
                .env("NANODA_BIN", &tp.nanoda_bin);
        }
        let nanoda_out = nanoda_cmd.output();
        let _ = std::fs::remove_dir_all(&single_cand_dir);
        match classify_process_output(
            "scripts/check-proof-nanoda.sh",
            nanoda_out,
            "nanoda_or_expr_stability_failed",
        ) {
            StepOutcome::Passed => {}
            other => return other,
        }
    }

    let axiom_out_res = Command::new("lake")
        .current_dir(package_root)
        .stdin(Stdio::null())
        .args(["env", "lean", "PromotionAudit.lean"])
        .output();
    let axiom_out = match axiom_out_res {
        Ok(o) => o,
        Err(io_err) => {
            return StepOutcome::InfraError(
                anyhow::Error::new(io_err)
                    .context("failed spawning `lake env lean PromotionAudit.lean`"),
            );
        }
    };
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&axiom_out.stdout),
        String::from_utf8_lossy(&axiom_out.stderr)
    );
    match classify_process_output(
        "lake env lean PromotionAudit.lean",
        Ok(axiom_out),
        "axiom_audit_failed",
    ) {
        StepOutcome::Passed => {}
        other => return other,
    }
    let axiom_file = package_root.join(".lake").join("swarm-proof-axioms.txt");
    if let Some(parent) = axiom_file.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return StepOutcome::InfraError(anyhow::Error::new(e));
        }
    }
    if let Err(e) = std::fs::write(&axiom_file, combined.as_bytes()) {
        return StepOutcome::InfraError(anyhow::Error::new(e));
    }
    if let Err(err) =
        check_proof_axiom_audit(&axiom_file, Some(&package_root.join("registry.json")))
    {
        let msg = format!("{err:#}");
        return if contains_infra_marker(&msg) {
            StepOutcome::InfraError(err)
        } else {
            StepOutcome::CandidateRejected {
                code: "axiom_audit_failed",
            }
        };
    }

    StepOutcome::Passed
}

pub fn run_audit_staged_candidates(repo_root: &Path, args: &AuditStagedArgs) -> Result<()> {
    if args.network != "mainnet" {
        let canonical_swarm_proofs_root = repo_root.join("proofs").join("SwarmProofs");
        assert_dry_run_isolation_for_network(
            &args.network,
            Some(&args.package_root),
            &args.package_root.join("registry.json"),
            &canonical_swarm_proofs_root,
        )?;
    }
    let plan = parse_autopromote_plan(&args.staged_candidates_dir.join("autopromote-plan.tsv"))?;
    let prebuilt_tools = if !args.skip_nanoda && !plan.is_empty() {
        Some(prepare_audit_tools(
            &args.package_root,
            Path::new(DEFAULT_AUDIT_TOOLS_DIR),
        )?)
    } else {
        None
    };
    let verdicts =
        audit_staged_candidates_with(repo_root, args, |root, pkg, cand, skip_nanoda| {
            real_lean_single_candidate_auditor_with_tools(
                root,
                pkg,
                cand,
                skip_nanoda,
                prebuilt_tools.as_ref(),
            )
        })?;
    println!("{}", serde_json::to_string_pretty(&verdicts)?);
    Ok(())
}

/// Pure-Rust reconstruction in `open-pr`: trusts ZERO Lean/registry files from `audit-untrusted-lean`!
/// Validates `audit-verdicts.json` strictly against the trusted `staged-candidates-dir` plan,
/// re-runs `autopromote_candidate` from the trusted candidate JSONs, and persists any Lean-audit
/// rejections into `rejected-candidates.json` using strictly canonical fixed ASCII reason codes.
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
            ALLOWED_AUDIT_REJECTION_CODES.contains(&rej.reason.as_str()),
            "invalid or non-canonical rejection reason `{}` in audit-verdicts.json",
            rej.reason
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
                    reason: item.reason.clone(),
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
    F: FnMut(&Path, &Path, &PromotionCandidate, bool) -> StepOutcome,
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

    let candidates_dir = dry_root.join("candidates");
    let verdicts_path = dry_root.join("audit-verdicts.json");
    if candidates_dir.exists() {
        std::fs::remove_dir_all(&candidates_dir)?;
    }
    if verdicts_path.is_file() {
        std::fs::remove_file(&verdicts_path)?;
    }

    let staged_pkg = stage_dry_run_swarm_proofs_copy(&public_pkg, dry_root)?;

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
    let mut cached_tools: Option<AuditToolPaths> = None;
    let verdicts = run_promote_pipeline_with(
        repo_root,
        &transport,
        args,
        token.as_deref(),
        |root, pkg, cand, skip_nanoda| {
            if !skip_nanoda && cached_tools.is_none() {
                match prepare_audit_tools(pkg, Path::new(DEFAULT_AUDIT_TOOLS_DIR)) {
                    Ok(tp) => cached_tools = Some(tp),
                    Err(err) => return StepOutcome::InfraError(err),
                }
            }
            real_lean_single_candidate_auditor_with_tools(
                root,
                pkg,
                cand,
                skip_nanoda,
                cached_tools.as_ref(),
            )
        },
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

#[cfg(test)]
mod tests {
    use super::*;
    use proof_registry::{
        bundle_id, resolve_bundle, sha256_hex, ChallengeManifest, APACHE_2_0,
        CHALLENGE_SCHEMA_VERSION,
    };

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .unwrap()
            .to_path_buf()
    }

    fn make_staged_batch(tmp: &Path, network: &str) -> (PathBuf, PathBuf) {
        let src_pkg = repo_root().join("proofs").join("SwarmProofs");
        let staged_pkg = stage_dry_run_swarm_proofs_copy(&src_pkg, tmp).unwrap();
        let cand_dir = tmp.join("staged-cands");
        std::fs::create_dir_all(&cand_dir).unwrap();

        let reg: RegistryManifest =
            serde_json::from_slice(&std::fs::read(staged_pkg.join("registry.json")).unwrap())
                .unwrap();
        let env_id = reg.environments[0].environment_id.clone();
        let empty_bundle = resolve_bundle(&reg, &env_id, &[]).unwrap();
        let dep_bundle_id = bundle_id(&empty_bundle).unwrap();

        let stmt_a = std::fs::read_to_string(
            repo_root().join(
                "tests/fixtures/lean/catalog/open/chain-rh-analytic-1-mertens-first-prime-log-bound.lean",
            ),
        )
        .unwrap();
        let stmt_b = std::fs::read_to_string(
            repo_root()
                .join("tests/fixtures/lean/catalog/open/chain-bsd-1-weierstrass-bijection.lean"),
        )
        .unwrap();
        let proof_a =
            "-- SPDX-License-Identifier: Apache-2.0\ntheorem proof : statementProp := trivial\n";
        let proof_b =
            "-- SPDX-License-Identifier: Apache-2.0\ntheorem proof : statementProp := rfl\n";

        let cand_a = PromotionCandidate {
            schema: "swarm.lean-promotion-candidate/v1".into(),
            task_id: "camp-a:task-a".into(),
            campaign_id: "camp-a".into(),
            network: network.into(),
            author_wallet: "SolverA".into(),
            artifact_sha256: sha256_hex(proof_a.as_bytes()),
            statement: stmt_a.clone(),
            proof_source: proof_a.into(),
            challenge: ChallengeManifest {
                schema: CHALLENGE_SCHEMA_VERSION.into(),
                statement_sha256: sha256_hex(stmt_a.as_bytes()),
                policy_version: 4,
                environment_id: env_id.clone(),
                dependency_bundle_id: dep_bundle_id.clone(),
                reuse_license: APACHE_2_0.into(),
                reusable: true,
            },
            direct_dependencies: Vec::new(),
            dependency_closure: Vec::new(),
            exact_imports: Vec::new(),
            verification_axioms: vec!["propext".into()],
            status: "verified_pending_finalization".into(),
        };
        let cand_b = PromotionCandidate {
            schema: "swarm.lean-promotion-candidate/v1".into(),
            task_id: "camp-b:task-b".into(),
            campaign_id: "camp-b".into(),
            network: network.into(),
            author_wallet: "SolverB".into(),
            artifact_sha256: sha256_hex(proof_b.as_bytes()),
            statement: stmt_b.clone(),
            proof_source: proof_b.into(),
            challenge: ChallengeManifest {
                schema: CHALLENGE_SCHEMA_VERSION.into(),
                statement_sha256: sha256_hex(stmt_b.as_bytes()),
                policy_version: 4,
                environment_id: env_id,
                dependency_bundle_id: dep_bundle_id,
                reuse_license: APACHE_2_0.into(),
                reusable: true,
            },
            direct_dependencies: Vec::new(),
            dependency_closure: Vec::new(),
            exact_imports: Vec::new(),
            verification_axioms: vec!["propext".into()],
            status: "verified_pending_finalization".into(),
        };

        let path_a = cand_dir.join("cand-a.json");
        let path_b = cand_dir.join("cand-b.json");
        std::fs::write(&path_a, serde_json::to_vec_pretty(&cand_a).unwrap()).unwrap();
        std::fs::write(&path_b, serde_json::to_vec_pretty(&cand_b).unwrap()).unwrap();
        std::fs::write(
            cand_dir.join("autopromote-plan.tsv"),
            format!(
                "{}\tcatalog:chain-rh-analytic-1-mertens-first-prime-log-bound\n{}\tcatalog:chain-bsd-1-weierstrass-bijection\n",
                path_a.display(),
                path_b.display()
            ),
        )
        .unwrap();

        (staged_pkg, cand_dir)
    }

    #[test]
    fn sf1_infra_errors_abort_without_writing_verdicts_file() {
        let tmp = std::env::temp_dir().join(format!("sf1-infra-abort-{}", uuid::Uuid::new_v4()));
        let (staged_pkg, cand_dir) = make_staged_batch(&tmp, "mainnet");
        let verdicts_out = tmp.join("audit-verdicts.json");
        let orig_reg = std::fs::read(staged_pkg.join("registry.json")).unwrap();

        // Simulate all required SF1 infrastructure failure modes:
        // 1. Spawn io::Error
        // 2. Exit 137 (OOM)
        // 3. Exit 128 (git failure)
        // 4. Exit 1 with "No space left on device" on stderr
        type SimOutcomeFn = fn() -> StepOutcome;
        let cases: &[(&str, SimOutcomeFn)] = &[
            ("spawn_io_error", || {
                classify_process_output(
                    "nonexistent-bin",
                    Command::new("/nonexistent-sf1-binary-xyz").output(),
                    "lean_compile_failed",
                )
            }),
            ("exit_137_oom", || {
                classify_process_output(
                    "lake build",
                    Command::new("sh").args(["-c", "exit 137"]).output(),
                    "lean_compile_failed",
                )
            }),
            ("exit_128_git", || {
                classify_process_output(
                    "check-proof-nanoda.sh",
                    Command::new("sh")
                        .args(["-c", "echo 'fatal: unable to access repo' >&2; exit 128"])
                        .output(),
                    "nanoda_or_expr_stability_failed",
                )
            }),
            ("enospc_stderr_exit_1", || {
                classify_process_output(
                    "lake build",
                    Command::new("sh")
                        .args(["-c", "echo 'error: No space left on device' >&2; exit 1"])
                        .output(),
                    "lean_compile_failed",
                )
            }),
        ];

        for (label, sim_fn) in cases {
            let res = audit_staged_candidates_with(
                &repo_root(),
                &AuditStagedArgs {
                    staged_candidates_dir: cand_dir.clone(),
                    package_root: staged_pkg.clone(),
                    verdicts_out: verdicts_out.clone(),
                    network: "mainnet".into(),
                    skip_nanoda: true,
                },
                |_root, _pkg, _cand, _skip| sim_fn(),
            );
            assert!(
                res.is_err(),
                "SF1 case `{label}` must return Err from audit_staged_candidates_with"
            );
            assert!(
                !verdicts_out.exists(),
                "SF1 case `{label}` must NOT write {}",
                verdicts_out.display()
            );
            assert_eq!(
                std::fs::read(staged_pkg.join("registry.json")).unwrap(),
                orig_reg,
                "SF1 case `{label}` must roll back registry.json"
            );
            assert!(
                list_generated_files(&staged_pkg.join("SwarmProofs/Generated")).is_empty(),
                "SF1 case `{label}` must roll back newly written Generated/*.lean files"
            );
        }

        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn sf1_n6_deterministic_source_policy_rejection_uses_fixed_code_while_healthy_passes() {
        let tmp = std::env::temp_dir().join(format!("sf1-det-reject-{}", uuid::Uuid::new_v4()));
        let (staged_pkg, cand_dir) = make_staged_batch(&tmp, "mainnet");
        let verdicts_out = tmp.join("audit-verdicts.json");

        let verdicts = audit_staged_candidates_with(
            &repo_root(),
            &AuditStagedArgs {
                staged_candidates_dir: cand_dir.clone(),
                package_root: staged_pkg.clone(),
                verdicts_out: verdicts_out.clone(),
                network: "mainnet".into(),
                skip_nanoda: true,
            },
            |_root, _pkg, cand, _skip| {
                if cand.task_id == "camp-a:task-a" {
                    classify_process_output(
                        "proof-source-policy.lean",
                        Command::new("sh")
                            .args([
                                "-c",
                                "echo 'UNTRUSTED_SOLVER_STDERR_PAYLOAD_NEVER_PERSIST' >&2; exit 1",
                            ])
                            .output(),
                        "source_policy_failed",
                    )
                } else {
                    StepOutcome::Passed
                }
            },
        )
        .unwrap();

        assert!(verdicts_out.is_file());
        assert_eq!(verdicts.passed_task_ids, vec!["camp-b:task-b"]);
        assert_eq!(verdicts.rejected.len(), 1);
        assert_eq!(verdicts.rejected[0].task_id, "camp-a:task-a");
        assert_eq!(verdicts.rejected[0].reason, "source_policy_failed");

        let verdicts_raw = std::fs::read_to_string(&verdicts_out).unwrap();
        assert!(!verdicts_raw.contains("UNTRUSTED_SOLVER_STDERR_PAYLOAD_NEVER_PERSIST"));

        // Verify open-pr reconstruction accepts fixed reason code and rejects raw/arbitrary reason strings:
        let reconstruct_dir = tmp.join("reconstruct");
        let reconstruct_pkg = stage_dry_run_swarm_proofs_copy(
            &repo_root().join("proofs/SwarmProofs"),
            &reconstruct_dir,
        )
        .unwrap();
        let rejected_json = reconstruct_pkg.join("rejected-candidates.json");
        let promoted = apply_verified_promotions(&ApplyVerifiedArgs {
            staged_candidates_dir: cand_dir.clone(),
            verdicts_path: verdicts_out.clone(),
            registry_path: reconstruct_pkg.join("registry.json"),
            package_root: reconstruct_pkg.clone(),
            rejected_file: rejected_json.clone(),
            public_base_url: DEFAULT_PUBLIC_REGISTRY_BASE_URL.into(),
        })
        .unwrap();
        assert_eq!(promoted, 1);
        let persisted = load_rejected_candidates_manifest(Some(&rejected_json)).unwrap();
        assert_eq!(
            persisted.rejected["mainnet:camp-a:task-a"].reason,
            "source_policy_failed"
        );

        let mut forged = verdicts;
        forged.rejected[0].reason = "arbitrary compiler stderr".into();
        let forged_path = tmp.join("forged-verdicts.json");
        std::fs::write(&forged_path, serde_json::to_vec_pretty(&forged).unwrap()).unwrap();
        assert!(apply_verified_promotions(&ApplyVerifiedArgs {
            staged_candidates_dir: cand_dir,
            verdicts_path: forged_path,
            registry_path: reconstruct_pkg.join("registry.json"),
            package_root: reconstruct_pkg,
            rejected_file: rejected_json,
            public_base_url: DEFAULT_PUBLIC_REGISTRY_BASE_URL.into(),
        })
        .is_err());

        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn sf4_audit_staged_candidates_rejects_devnet_against_public_swarm_proofs() {
        let tmp = std::env::temp_dir().join(format!("sf4-devnet-guard-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        let verdicts_out = tmp.join("verdicts.json");
        let public_pkg = repo_root().join("proofs").join("SwarmProofs");

        let err = audit_staged_candidates_with(
            &repo_root(),
            &AuditStagedArgs {
                staged_candidates_dir: tmp.clone(),
                package_root: public_pkg,
                verdicts_out: verdicts_out.clone(),
                network: "devnet".into(),
                skip_nanoda: true,
            },
            |_r, _p, _c, _s| StepOutcome::Passed,
        )
        .unwrap_err()
        .to_string();

        assert!(
            err.contains("devnet proofs never enter the public registry"),
            "unexpected SF4 error: {err}"
        );
        assert!(!verdicts_out.exists());
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn n3_stage_dry_run_swarm_proofs_copy_wipes_stale_files_and_refreshes_registry() {
        let tmp = std::env::temp_dir().join(format!("n3-stage-refresh-{}", uuid::Uuid::new_v4()));
        let src_root = tmp.join("mock-source");
        let dry_root = tmp.join("reused-dry-run");

        let real_pkg = repo_root().join("proofs").join("SwarmProofs");
        let mock_src_pkg = stage_dry_run_swarm_proofs_copy(&real_pkg, &src_root).unwrap();

        // First stage into dry_root:
        let staged1 = stage_dry_run_swarm_proofs_copy(&mock_src_pkg, &dry_root).unwrap();
        let stale_gen = staged1
            .join("SwarmProofs")
            .join("Generated")
            .join("Hstale.lean");
        std::fs::create_dir_all(stale_gen.parent().unwrap()).unwrap();
        std::fs::write(&stale_gen, b"-- stale generated module\n").unwrap();
        std::fs::write(staged1.join("PromotionAudit.lean"), b"-- stale umbrella\n").unwrap();
        assert!(stale_gen.is_file());

        // Mutate mock_src_pkg/registry.json (e.g. formatted comment/alias change in JSON):
        let mut reg_val: serde_json::Value =
            serde_json::from_slice(&std::fs::read(mock_src_pkg.join("registry.json")).unwrap())
                .unwrap();
        reg_val["aliases"] = serde_json::json!({
            "catalog:test-fresh": "sha256:1111111111111111111111111111111111111111111111111111111111111111"
        });
        let updated_src_bytes = serde_json::to_vec_pretty(&reg_val).unwrap();
        std::fs::write(mock_src_pkg.join("registry.json"), &updated_src_bytes).unwrap();

        // Second stage into the SAME dry_root MUST overwrite registry.json and wipe Generated/Hstale.lean!
        let staged2 = stage_dry_run_swarm_proofs_copy(&mock_src_pkg, &dry_root).unwrap();
        assert_eq!(staged1, staged2);
        assert_eq!(
            std::fs::read(staged2.join("registry.json")).unwrap(),
            updated_src_bytes
        );
        assert!(
            !stale_gen.exists(),
            "N3: stale Generated/Hstale.lean must be wiped when reusing dry_run_root"
        );
        assert!(
            !staged2.join("PromotionAudit.lean").exists(),
            "N3: stale PromotionAudit.lean must be wiped when reusing dry_run_root"
        );

        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
