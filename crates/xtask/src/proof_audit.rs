use anyhow::{bail, ensure, Context, Result};
use proof_registry::{
    canonical_json, sha256_hex, PromotionCandidate, RegistryManifest, ALLOWED_AXIOMS,
    ALLOWED_SWARM_LIBRARY_IMPORTS, GENERATED_MODULE_PREFIX,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Command, Stdio};

const FORBIDDEN_AXIOMS: &[&str] = &["sorryAx", "Lean.ofReduceBool", "Lean.trustCompiler"];
const MAX_EXPORT_LINES: usize = 50_000_000;

#[derive(Debug, Deserialize)]
struct RegistryFile {
    #[serde(default)]
    modules: Vec<RegistryModule>,
}

#[derive(Debug, Deserialize)]
struct RegistryModule {
    module_id: String,
    module_name: String,
    theorem_name: String,
    source_sha256: String,
    source_bytes: u64,
    #[serde(default)]
    direct_dependencies: Vec<String>,
    #[serde(default = "default_active_status")]
    status: String,
}

fn default_active_status() -> String {
    "active".to_string()
}

#[derive(Debug, Deserialize)]
struct DeclarationInventory {
    declarations: Vec<InventoryEntry>,
}

#[derive(Debug, Deserialize)]
struct InventoryEntry {
    name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverageReport {
    pub axioms: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub export_sha256: Option<String>,
    pub exported_declarations: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inventory_sha256: Option<String>,
    pub roots: Vec<String>,
    pub schema: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct VerificationReceipt<'a> {
    allowed_axioms: &'a [&'a str],
    checked_declaration_count: usize,
    coverage_sha256: String,
    export_sha256: String,
    independent_kernel: bool,
    inventory_sha256: String,
    lean4export_revision: &'a str,
    nanoda_revision: &'a str,
    root_count: usize,
    schema: &'static str,
}

pub fn prepare_proof_promotion_audit(package_root: &Path) -> Result<()> {
    let registry_path = package_root.join("registry.json");
    ensure!(
        registry_path.is_file(),
        "missing registry.json at {}",
        registry_path.display()
    );
    let registry: RegistryFile = serde_json::from_slice(
        &std::fs::read(&registry_path)
            .with_context(|| format!("failed reading {}", registry_path.display()))?,
    )
    .context("invalid registry.json")?;

    let active: Vec<&RegistryModule> = registry
        .modules
        .iter()
        .filter(|m| m.status == "active")
        .collect();
    let imports = active
        .iter()
        .map(|m| format!("import {}", m.module_name))
        .collect::<Vec<_>>()
        .join("\n");
    let axioms = active
        .iter()
        .map(|m| format!("#print axioms {}", m.theorem_name))
        .collect::<Vec<_>>()
        .join("\n");
    let audit_source = format!("/- SPDX-License-Identifier: Apache-2.0 -/\n{imports}\n{axioms}\n");
    let audit_path = package_root.join("PromotionAudit.lean");
    std::fs::write(&audit_path, audit_source)
        .with_context(|| format!("failed writing {}", audit_path.display()))?;

    let lakefile = package_root.join("lakefile.toml");
    let backup = package_root.join("lakefile.toml.original");
    let original_bytes = if backup.is_file() {
        std::fs::read(&backup).with_context(|| format!("failed reading {}", backup.display()))?
    } else {
        let bytes = std::fs::read(&lakefile)
            .with_context(|| format!("failed reading {}", lakefile.display()))?;
        std::fs::write(&backup, &bytes)
            .with_context(|| format!("failed writing {}", backup.display()))?;
        bytes
    };
    let mut updated =
        String::from_utf8(original_bytes).context("lakefile.toml is not valid UTF-8")?;
    updated.push_str("\n[[lean_lib]]\nname = \"PromotionAudit\"\n");
    std::fs::write(&lakefile, updated)
        .with_context(|| format!("failed writing {}", lakefile.display()))?;
    ensure!(
        audit_path.is_file() && backup.is_file(),
        "audit files must exist after preparation"
    );
    Ok(())
}

#[derive(Debug)]
pub enum SourcePolicyCheckError {
    CandidatePolicy(anyhow::Error),
    Infra(anyhow::Error),
}

pub fn check_frozen_defs_append_only(
    repo_root: &Path,
    package_root: &Path,
    base_registry_path: Option<&Path>,
) -> Result<()> {
    let head_reg_path = package_root.join("registry.json");
    let head_bytes = std::fs::read(&head_reg_path)
        .with_context(|| format!("failed reading {}", head_reg_path.display()))?;
    let head: RegistryManifest = serde_json::from_slice(&head_bytes)
        .with_context(|| format!("invalid {}", head_reg_path.display()))?;
    proof_registry::validate_registry(&head)
        .map_err(|e| anyhow::anyhow!("head registry validation failed: {e}"))?;
    proof_registry::validate_frozen_library_modules(package_root, &head)
        .map_err(|e| anyhow::anyhow!("head frozen library validation failed: {e}"))?;

    if let Some(v4_env) = head
        .environments
        .iter()
        .find(|env| env.lock.policy_version == 4)
    {
        for (mod_name, rel_file, pinned_sha) in proof_registry::FROZEN_V1_DEFS_GOLDEN_HASHES {
            let actual = v4_env
                .frozen_library_modules
                .get(*mod_name)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "missing immutable V1 frozen library module `{mod_name}` in {}",
                        head_reg_path.display()
                    )
                })?;
            ensure!(
                actual == pinned_sha,
                "immutable V1 frozen library module `{mod_name}` hash drifted: expected {pinned_sha}, got {actual}"
            );
            let disk_bytes = std::fs::read(package_root.join(rel_file)).with_context(|| {
                format!("failed reading {}", package_root.join(rel_file).display())
            })?;
            let disk_sha = format!("sha256:{}", sha256_hex(&disk_bytes));
            ensure!(
                disk_sha == *pinned_sha,
                "immutable V1 frozen library file `{rel_file}` hash drifted: expected {pinned_sha}, got {disk_sha}"
            );
        }
    }

    if let Some(base_path) = base_registry_path {
        let base_bytes = std::fs::read(base_path)
            .with_context(|| format!("failed reading base registry {}", base_path.display()))?;
        let base: RegistryManifest = serde_json::from_slice(&base_bytes)
            .with_context(|| format!("invalid base registry {}", base_path.display()))?;
        proof_registry::validate_frozen_defs_append_only(&base, &head)
            .map_err(|e| anyhow::anyhow!("append-only frozen Defs check failed: {e}"))?;
    } else {
        let cat_status = Command::new("git")
            .current_dir(repo_root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .args([
                "cat-file",
                "-e",
                "origin/main:proofs/SwarmProofs/registry.json",
            ])
            .status();
        if let Ok(st) = cat_status {
            if st.success() {
                let show_out = Command::new("git")
                    .current_dir(repo_root)
                    .stdin(Stdio::null())
                    .args(["show", "origin/main:proofs/SwarmProofs/registry.json"])
                    .output()
                    .context(
                        "failed executing `git show origin/main:proofs/SwarmProofs/registry.json`",
                    )?;
                ensure!(
                    show_out.status.success(),
                    "`git show origin/main:proofs/SwarmProofs/registry.json` failed"
                );
                let base: RegistryManifest = serde_json::from_slice(&show_out.stdout)
                    .context("invalid origin/main:proofs/SwarmProofs/registry.json")?;
                proof_registry::validate_frozen_defs_append_only(&base, &head).map_err(|e| {
                    anyhow::anyhow!("append-only frozen Defs check against origin/main failed: {e}")
                })?;
            }
        }
    }

    println!(
        "verified append-only frozen Defs integrity for {}",
        head_reg_path.display()
    );
    Ok(())
}

pub fn check_proof_sources(repo_root: &Path, package_root: &Path) -> Result<()> {
    match check_proof_sources_classified(repo_root, package_root) {
        Ok(()) => Ok(()),
        Err(SourcePolicyCheckError::CandidatePolicy(e) | SourcePolicyCheckError::Infra(e)) => {
            Err(e)
        }
    }
}

pub fn check_proof_sources_classified(
    repo_root: &Path,
    package_root: &Path,
) -> std::result::Result<(), SourcePolicyCheckError> {
    let registry_path = package_root.join("registry.json");
    let policy_script = repo_root.join("scripts").join("proof-source-policy.lean");
    if !registry_path.is_file() {
        return Err(SourcePolicyCheckError::Infra(anyhow::anyhow!(
            "missing {}",
            registry_path.display()
        )));
    }
    if !policy_script.is_file() {
        return Err(SourcePolicyCheckError::Infra(anyhow::anyhow!(
            "missing {}",
            policy_script.display()
        )));
    }

    let reg_bytes = std::fs::read(&registry_path).map_err(|e| {
        SourcePolicyCheckError::Infra(
            anyhow::Error::new(e).context(format!("failed reading {}", registry_path.display())),
        )
    })?;
    let registry: RegistryFile = serde_json::from_slice(&reg_bytes).map_err(|e| {
        SourcePolicyCheckError::Infra(
            anyhow::Error::new(e).context(format!("invalid {}", registry_path.display())),
        )
    })?;
    let modules_by_id: BTreeMap<&str, &RegistryModule> = registry
        .modules
        .iter()
        .map(|m| (m.module_id.as_str(), m))
        .collect();

    for module in &registry.modules {
        verify_single_module_source_classified(
            package_root,
            &policy_script,
            module,
            &modules_by_id,
        )?;
    }
    Ok(())
}

fn verify_single_module_source_classified(
    package_root: &Path,
    policy_script: &Path,
    module: &RegistryModule,
    modules_by_id: &BTreeMap<&str, &RegistryModule>,
) -> std::result::Result<(), SourcePolicyCheckError> {
    if module.module_name.contains('/') || module.module_name.contains("..") {
        return Err(SourcePolicyCheckError::CandidatePolicy(anyhow::anyhow!(
            "invalid module_name: {}",
            module.module_name
        )));
    }
    let source_path = package_root.join(format!("{}.lean", module.module_name.replace('.', "/")));
    let payload = std::fs::read(&source_path).map_err(|e| {
        let msg = format!("missing module source {}: {e}", source_path.display());
        if crate::promote_pipeline::contains_infra_marker(&msg) {
            SourcePolicyCheckError::Infra(anyhow::anyhow!(msg))
        } else {
            SourcePolicyCheckError::CandidatePolicy(anyhow::anyhow!(msg))
        }
    })?;
    if sha256_hex(&payload) != module.source_sha256 || payload.len() as u64 != module.source_bytes {
        return Err(SourcePolicyCheckError::CandidatePolicy(anyhow::anyhow!(
            "source integrity mismatch: {}",
            module.module_id
        )));
    }

    let closure = resolve_dependency_names(module, modules_by_id)
        .map_err(SourcePolicyCheckError::CandidatePolicy)?;
    let (namespace, _) = module.theorem_name.rsplit_once('.').ok_or_else(|| {
        SourcePolicyCheckError::CandidatePolicy(anyhow::anyhow!(
            "unqualified theorem_name: {}",
            module.theorem_name
        ))
    })?;

    let mut cmd = Command::new("lake");
    cmd.current_dir(package_root)
        .stdin(Stdio::null())
        .arg("env")
        .arg("lean")
        .arg("--run")
        .arg(policy_script)
        .arg(&source_path)
        .arg(namespace)
        .arg("Mathlib")
        .arg("Zeta23");
    for lib_mod in ALLOWED_SWARM_LIBRARY_IMPORTS {
        cmd.arg(lib_mod);
    }
    for allowed_mod in &closure {
        cmd.arg(allowed_mod);
    }
    let out = cmd.output().map_err(|e| {
        SourcePolicyCheckError::Infra(
            anyhow::Error::new(e)
                .context("failed to execute `lake env lean --run proof-source-policy.lean`"),
        )
    })?;
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    match out.status.code() {
        Some(0) => Ok(()),
        None => Err(SourcePolicyCheckError::Infra(anyhow::anyhow!(
            "`proof-source-policy.lean` terminated by signal for {}",
            module.module_id
        ))),
        Some(code) if !(1..=124).contains(&code) => {
            Err(SourcePolicyCheckError::Infra(anyhow::anyhow!(
                "`proof-source-policy.lean` exited with infrastructure status {code} for {}",
                module.module_id
            )))
        }
        Some(code) => {
            if crate::promote_pipeline::contains_infra_marker(&combined) {
                Err(SourcePolicyCheckError::Infra(anyhow::anyhow!(
                    "`proof-source-policy.lean` hit infrastructure error (exit {code}) for {}",
                    module.module_id
                )))
            } else {
                eprint!("{}", String::from_utf8_lossy(&out.stderr));
                Err(SourcePolicyCheckError::CandidatePolicy(anyhow::anyhow!(
                    "proof-source-policy failed for {} (exit {code})",
                    module.module_id
                )))
            }
        }
    }
}

fn resolve_dependency_names(
    module: &RegistryModule,
    modules_by_id: &BTreeMap<&str, &RegistryModule>,
) -> Result<Vec<String>> {
    let max_visits = modules_by_id.len().saturating_add(1);
    let mut visited = BTreeSet::new();
    let mut stack: Vec<&str> = module
        .direct_dependencies
        .iter()
        .map(String::as_str)
        .collect();
    let mut steps = 0usize;
    while let Some(dep_id) = stack.pop() {
        if !visited.insert(dep_id) {
            continue;
        }
        steps = steps.saturating_add(1);
        ensure!(steps <= max_visits, "dependency graph bound exceeded");
        let dep = modules_by_id
            .get(dep_id)
            .ok_or_else(|| anyhow::anyhow!("unknown dependency: {dep_id}"))?;
        for next in &dep.direct_dependencies {
            stack.push(next.as_str());
        }
    }
    let mut names = Vec::with_capacity(visited.len());
    for dep_id in visited {
        let dep = modules_by_id
            .get(dep_id)
            .ok_or_else(|| anyhow::anyhow!("unknown dependency: {dep_id}"))?;
        names.push(dep.module_name.clone());
    }
    names.sort();
    Ok(names)
}

pub fn export_proof_declarations(
    package_root: &Path,
    exporter_bin: &Path,
    module_name: &str,
    inventory_path: &Path,
    export_path: &Path,
) -> Result<()> {
    ensure!(
        package_root.is_dir(),
        "package root must exist: {}",
        package_root.display()
    );
    ensure!(!module_name.is_empty(), "module name must be non-empty");
    let inventory: DeclarationInventory = serde_json::from_slice(
        &std::fs::read(inventory_path)
            .with_context(|| format!("failed reading {}", inventory_path.display()))?,
    )
    .context("invalid declaration inventory JSON")?;

    let out_file = File::create(export_path)
        .with_context(|| format!("failed creating {}", export_path.display()))?;
    let mut cmd = Command::new("lake");
    cmd.current_dir(package_root)
        .stdin(Stdio::null())
        .stdout(Stdio::from(out_file))
        .arg("env")
        .arg(exporter_bin)
        .arg(module_name)
        .arg("--");
    for item in &inventory.declarations {
        cmd.arg(&item.name);
    }
    let status = cmd
        .status()
        .context("failed running lean4export via `lake env`")?;
    ensure!(
        status.success(),
        "lean4export failed for module {module_name}"
    );
    ensure!(
        export_path.is_file(),
        "export file must exist after lean4export"
    );
    Ok(())
}

pub fn check_proof_export_coverage_files(
    inventory_path: &Path,
    export_path: &Path,
    report_path: &Path,
) -> Result<()> {
    let inventory_bytes = std::fs::read(inventory_path)
        .with_context(|| format!("failed reading {}", inventory_path.display()))?;
    let inventory: Value =
        serde_json::from_slice(&inventory_bytes).context("invalid inventory JSON")?;

    let export_file = File::open(export_path)
        .with_context(|| format!("failed opening {}", export_path.display()))?;
    let reader = BufReader::new(export_file);
    let mut report = check_coverage(&inventory, reader.lines())?;
    report.inventory_sha256 = Some(sha256_hex(&inventory_bytes));
    report.export_sha256 = Some(sha256_file(export_path)?);

    let mut rendered = serde_json::to_string_pretty(&report)?;
    rendered.push('\n');
    std::fs::write(report_path, rendered)
        .with_context(|| format!("failed writing {}", report_path.display()))?;
    ensure!(
        report_path.is_file(),
        "coverage report must exist after write"
    );
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let mut file =
        File::open(path).with_context(|| format!("failed opening {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buf)?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    let digest = hasher.finalize();
    Ok(sha256_digest_hex(&digest))
}

fn sha256_digest_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().saturating_mul(2));
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
    }
    out
}

pub fn check_coverage<I, E>(inventory: &Value, lines: I) -> Result<CoverageReport>
where
    I: IntoIterator<Item = std::result::Result<String, E>>,
    E: std::error::Error + Send + Sync + 'static,
{
    let raw_decls = inventory
        .get("declarations")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("inventory missing declarations array"))?;
    let mut roots_set = BTreeSet::new();
    for item in raw_decls {
        let name = item
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("inventory declaration missing string name"))?;
        if !roots_set.insert(name.to_string()) {
            bail!("duplicate inventory declaration");
        }
    }

    let mut names: HashMap<u64, String> = HashMap::new();
    names.insert(0, String::new());
    let mut declarations = BTreeSet::new();
    let mut axioms = BTreeSet::new();
    let mut line_count = 0usize;

    for line_res in lines {
        line_count = line_count.saturating_add(1);
        ensure!(
            line_count <= MAX_EXPORT_LINES,
            "export exceeded maximum line bound"
        );
        let line = line_res?;
        let record: Value = serde_json::from_str(&line).context("invalid JSON line in export")?;
        process_export_record(&record, &mut names, &mut declarations, &mut axioms)?;
    }

    let missing: Vec<&String> = roots_set.difference(&declarations).collect();
    if !missing.is_empty() {
        bail!("export omitted declarations: {missing:?}");
    }
    let unapproved: Vec<&String> = axioms
        .iter()
        .filter(|ax| !ALLOWED_AXIOMS.contains(&ax.as_str()))
        .collect();
    if !unapproved.is_empty() {
        bail!("unapproved axioms: {unapproved:?}");
    }

    Ok(CoverageReport {
        axioms: axioms.into_iter().collect(),
        export_sha256: None,
        exported_declarations: declarations.into_iter().collect(),
        inventory_sha256: None,
        roots: roots_set.into_iter().collect(),
        schema: "swarm.lean-export-coverage/v1".to_string(),
    })
}

fn process_export_record(
    record: &Value,
    names: &mut HashMap<u64, String>,
    declarations: &mut BTreeSet<String>,
    axioms: &mut BTreeSet<String>,
) -> Result<()> {
    if let Some(in_idx) = record.get("in").and_then(Value::as_u64) {
        let (value, part) = if let Some(v) = record.get("str") {
            let s = v
                .get("str")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow::anyhow!("malformed `str` name record"))?;
            (v, s.to_string())
        } else {
            let v = record
                .get("num")
                .ok_or_else(|| anyhow::anyhow!("name record missing `str` and `num`"))?;
            let i_val = v
                .get("i")
                .ok_or_else(|| anyhow::anyhow!("malformed `num` name record"))?;
            let part = match i_val {
                Value::Number(n) => n.to_string(),
                Value::String(s) => s.clone(),
                _ => bail!("invalid numeric name component"),
            };
            (v, part)
        };
        let pre = value
            .get("pre")
            .and_then(Value::as_u64)
            .ok_or_else(|| anyhow::anyhow!("name record missing `pre` index"))?;
        let parent = names
            .get(&pre)
            .ok_or_else(|| anyhow::anyhow!("unknown parent name index {pre}"))?;
        let full = if parent.is_empty() {
            part
        } else {
            format!("{parent}.{part}")
        };
        names.insert(in_idx, full);
    }

    let mut entries: Vec<&Value> = Vec::new();
    for kind in ["axiom", "def", "opaque", "thm", "quot"] {
        if let Some(entry) = record.get(kind) {
            entries.push(entry);
            if kind == "axiom" {
                let name_idx = entry
                    .get("name")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| anyhow::anyhow!("axiom entry missing name index"))?;
                let ax_name = names
                    .get(&name_idx)
                    .ok_or_else(|| anyhow::anyhow!("unknown axiom name index {name_idx}"))?;
                axioms.insert(ax_name.clone());
            }
        }
    }
    if let Some(ind) = record.get("inductive") {
        for kind in ["types", "ctors", "recs"] {
            if let Some(arr) = ind.get(kind).and_then(Value::as_array) {
                entries.extend(arr.iter());
            }
        }
    }
    for entry in entries {
        let is_unsafe = entry
            .get("isUnsafe")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let safety = entry
            .get("safety")
            .and_then(Value::as_str)
            .unwrap_or("safe");
        if is_unsafe || safety != "safe" {
            bail!("unsafe/partial exported declaration");
        }
        let name_idx = entry
            .get("name")
            .and_then(Value::as_u64)
            .ok_or_else(|| anyhow::anyhow!("declaration entry missing name index"))?;
        let decl_name = names
            .get(&name_idx)
            .ok_or_else(|| anyhow::anyhow!("unknown declaration name index {name_idx}"))?;
        declarations.insert(decl_name.clone());
    }
    Ok(())
}

pub fn write_proof_verification_receipt(
    report_root: &Path,
    lean4export_revision: &str,
    nanoda_revision: &str,
) -> Result<()> {
    ensure!(
        !lean4export_revision.is_empty(),
        "lean4export revision required"
    );
    ensure!(!nanoda_revision.is_empty(), "nanoda revision required");
    let coverage_path = report_root.join("coverage.json");
    let coverage_bytes = std::fs::read(&coverage_path)
        .with_context(|| format!("failed reading {}", coverage_path.display()))?;
    let coverage: CoverageReport =
        serde_json::from_slice(&coverage_bytes).context("invalid coverage.json")?;
    let inventory_sha256 = coverage
        .inventory_sha256
        .ok_or_else(|| anyhow::anyhow!("coverage.json missing inventory_sha256"))?;
    let export_sha256 = coverage
        .export_sha256
        .ok_or_else(|| anyhow::anyhow!("coverage.json missing export_sha256"))?;

    let receipt = VerificationReceipt {
        allowed_axioms: ALLOWED_AXIOMS,
        checked_declaration_count: coverage.exported_declarations.len(),
        coverage_sha256: sha256_hex(&coverage_bytes),
        export_sha256,
        independent_kernel: true,
        inventory_sha256,
        lean4export_revision,
        nanoda_revision,
        root_count: coverage.roots.len(),
        schema: "swarm.lean-independent-verification/v1",
    };
    let mut rendered = serde_json::to_string_pretty(&receipt)?;
    rendered.push('\n');
    let out_path = report_root.join("verification.json");
    std::fs::write(&out_path, rendered)
        .with_context(|| format!("failed writing {}", out_path.display()))?;
    ensure!(
        out_path.is_file(),
        "verification.json must exist after write"
    );
    Ok(())
}

pub fn check_proof_axiom_audit(audit_path: &Path, registry_path: Option<&Path>) -> Result<String> {
    let text = std::fs::read_to_string(audit_path)
        .with_context(|| format!("failed reading {}", audit_path.display()))?;
    let registry_json: Option<Value> = match registry_path {
        Some(path) => {
            let bytes = std::fs::read(path)
                .with_context(|| format!("failed reading {}", path.display()))?;
            Some(serde_json::from_slice(&bytes).context("invalid registry JSON")?)
        }
        None => None,
    };
    let msg = verify_axiom_audit_text(&text, registry_json.as_ref())?;
    println!("{msg}");
    Ok(msg)
}

pub fn verify_axiom_audit_text(text: &str, registry: Option<&Value>) -> Result<String> {
    for forbidden in FORBIDDEN_AXIOMS {
        if text.contains(forbidden) {
            bail!("forbidden axiom in audit: {forbidden}");
        }
    }

    let mut audited: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("error:") || line.contains(": error:") {
            bail!("lean error during axiom audit: {line}");
        }
        if let Some(thm) = parse_no_axioms_line(line) {
            audited.insert(thm.to_string(), BTreeSet::new());
            continue;
        }
        if let Some((thm, raw_list)) = parse_depends_line(line) {
            let found: BTreeSet<String> = raw_list
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToOwned::to_owned)
                .collect();
            if found.is_empty() {
                bail!("malformed empty axiom list in audit line: {line}");
            }
            let unapproved: Vec<&String> = found
                .iter()
                .filter(|ax| !ALLOWED_AXIOMS.contains(&ax.as_str()))
                .collect();
            if !unapproved.is_empty() {
                bail!("unapproved axioms: {unapproved:?}");
            }
            audited.insert(thm.to_string(), found);
            continue;
        }
        if line.contains("depends on axioms") || line.contains("does not depend on any axioms") {
            bail!("unrecognized axiom audit line: {line}");
        }
    }

    if let Some(reg) = registry {
        let active_theorems: Vec<&str> = reg
            .get("modules")
            .and_then(Value::as_array)
            .map(|mods| {
                mods.iter()
                    .filter(|m| {
                        m.get("status").and_then(Value::as_str).unwrap_or("active") == "active"
                    })
                    .filter_map(|m| m.get("theorem_name").and_then(Value::as_str))
                    .collect()
            })
            .unwrap_or_default();
        if audited.is_empty() {
            if active_theorems.is_empty() {
                return Ok("audited empty initial registry".to_string());
            }
            bail!("axiom audit produced no theorem lines");
        }
        let missing: Vec<&str> = active_theorems
            .into_iter()
            .filter(|thm| !audited.contains_key(*thm))
            .collect();
        if !missing.is_empty() {
            bail!("missing axiom audit output for theorem(s): {missing:?}");
        }
    } else if audited.is_empty() {
        bail!("axiom audit produced no theorem lines");
    }

    Ok(format!("audited {} exported theorem(s)", audited.len()))
}

fn parse_no_axioms_line(trimmed: &str) -> Option<&str> {
    let rest = trimmed.strip_prefix('\'')?;
    let (name, after) = rest.split_once('\'')?;
    if name.is_empty() || after != " does not depend on any axioms" {
        return None;
    }
    Some(name)
}

fn parse_depends_line(trimmed: &str) -> Option<(&str, &str)> {
    let rest = trimmed.strip_prefix('\'')?;
    let (name, after) = rest.split_once('\'')?;
    if name.is_empty() {
        return None;
    }
    let list_part = after.strip_prefix(" depends on axioms: [")?;
    let inner = list_part.strip_suffix(']')?;
    Some((name, inner))
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum NamePart {
    Str(String),
    Num(u64),
}

fn split_dotted_name(name: &str) -> Vec<NamePart> {
    if name.trim().is_empty() {
        return Vec::new();
    }
    name.split('.')
        .map(|seg| match seg.parse::<u64>() {
            Ok(n) => NamePart::Num(n),
            Err(_) => NamePart::Str(seg.to_string()),
        })
        .collect()
}

fn normalize_name_parts(
    parts: &[NamePart],
    strip_ns: &[NamePart],
    mod_parts: &[NamePart],
) -> Vec<NamePart> {
    let mut work = parts.to_vec();
    if !mod_parts.is_empty()
        && work.len() > mod_parts.len() + 2
        && work[0] == NamePart::Str("_private".to_string())
        && work[1..1 + mod_parts.len()] == *mod_parts
        && work[1 + mod_parts.len()] == NamePart::Num(0)
    {
        let tail = &work[2 + mod_parts.len()..];
        let clean_tail = if !strip_ns.is_empty() && tail.starts_with(strip_ns) {
            &tail[strip_ns.len()..]
        } else {
            tail
        };
        let mut rebuilt = vec![NamePart::Str("_private".to_string()), NamePart::Num(0)];
        rebuilt.extend_from_slice(clean_tail);
        work = rebuilt;
    } else if !strip_ns.is_empty() && work.starts_with(strip_ns) {
        work = work[strip_ns.len()..].to_vec();
    }

    if !mod_parts.is_empty() {
        let mut out = Vec::with_capacity(work.len());
        let mut i = 0usize;
        while i < work.len() {
            if work[i] == NamePart::Str("_@".to_string())
                && i + 1 + mod_parts.len() < work.len()
                && work[i + 1..1 + i + mod_parts.len()] == *mod_parts
                && work[i + 1 + mod_parts.len()] == NamePart::Str("_hyg".to_string())
            {
                out.push(NamePart::Str("_@".to_string()));
                out.push(NamePart::Str("_hyg".to_string()));
                i += 2 + mod_parts.len();
            } else {
                out.push(work[i].clone());
                i += 1;
            }
        }
        work = out;
    }
    work
}

struct CanonExportState {
    strip_ns: Vec<NamePart>,
    mod_parts: Vec<NamePart>,
    old_names: HashMap<u64, Vec<NamePart>>,
    canon_names: HashMap<Vec<NamePart>, u64>,
    old_il_to_canon: HashMap<u64, u64>,
    old_ie_to_canon: HashMap<u64, u64>,
    next_in: u64,
    next_il: u64,
    next_ie: u64,
    out_lines: Vec<String>,
    saw_statement_prop: bool,
}

impl CanonExportState {
    fn new(strip_namespace: Option<&str>, module_name: Option<&str>) -> Self {
        let mut old_names = HashMap::new();
        old_names.insert(0, Vec::new());
        let mut canon_names = HashMap::new();
        canon_names.insert(Vec::new(), 0);
        let mut old_il_to_canon = HashMap::new();
        old_il_to_canon.insert(0, 0);
        Self {
            strip_ns: strip_namespace.map(split_dotted_name).unwrap_or_default(),
            mod_parts: module_name.map(split_dotted_name).unwrap_or_default(),
            old_names,
            canon_names,
            old_il_to_canon,
            old_ie_to_canon: HashMap::new(),
            next_in: 1,
            next_il: 1,
            next_ie: 0,
            out_lines: Vec::new(),
            saw_statement_prop: false,
        }
    }

    fn emit_val(&mut self, val: &Value) -> Result<()> {
        let bytes = canonical_json(val)
            .map_err(|e| anyhow::anyhow!("canonical_json failed on export record: {e}"))?;
        let line = String::from_utf8(bytes).context("invalid UTF-8 from canonical_json")?;
        self.out_lines.push(line);
        Ok(())
    }

    fn intern_norm_parts(&mut self, parts: &[NamePart]) -> Result<u64> {
        if let Some(&idx) = self.canon_names.get(parts) {
            return Ok(idx);
        }
        let Some((last, prefix)) = parts.split_last() else {
            return Ok(0);
        };
        let pre_idx = self.intern_norm_parts(prefix)?;
        let idx = self.next_in;
        self.next_in = self.next_in.saturating_add(1);
        let rec = match last {
            NamePart::Str(s) => serde_json::json!({
                "in": idx,
                "str": { "pre": pre_idx, "str": s }
            }),
            NamePart::Num(n) => serde_json::json!({
                "in": idx,
                "num": { "pre": pre_idx, "i": n }
            }),
        };
        self.emit_val(&rec)?;
        self.canon_names.insert(parts.to_vec(), idx);
        Ok(idx)
    }

    fn map_name(&mut self, old_idx: u64) -> Result<u64> {
        let raw = self
            .old_names
            .get(&old_idx)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown name index {old_idx} in lean4export"))?;
        let norm = normalize_name_parts(&raw, &self.strip_ns, &self.mod_parts);
        self.intern_norm_parts(&norm)
    }

    fn map_name_array(&mut self, arr: &Value) -> Result<Value> {
        let items = arr
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("expected array of name indices"))?;
        let mut out = Vec::with_capacity(items.len());
        for v in items {
            let idx = v
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("expected u64 name index"))?;
            out.push(Value::from(self.map_name(idx)?));
        }
        Ok(Value::Array(out))
    }

    fn map_level(&self, old_il: u64) -> Result<u64> {
        self.old_il_to_canon
            .get(&old_il)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("unknown level index {old_il} in lean4export"))
    }

    fn map_level_array(&self, arr: &Value) -> Result<Value> {
        let items = arr
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("expected array of level indices"))?;
        let mut out = Vec::with_capacity(items.len());
        for v in items {
            let idx = v
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("expected u64 level index"))?;
            out.push(Value::from(self.map_level(idx)?));
        }
        Ok(Value::Array(out))
    }

    fn map_expr(&self, old_ie: u64) -> Result<u64> {
        self.old_ie_to_canon
            .get(&old_ie)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("unknown expr index {old_ie} in lean4export"))
    }
}

pub fn canonicalize_lean4export_stream<I, E>(
    lines: I,
    strip_namespace: Option<&str>,
    module_name: Option<&str>,
) -> Result<Vec<String>>
where
    I: IntoIterator<Item = std::result::Result<String, E>>,
    E: std::error::Error + Send + Sync + 'static,
{
    let mut st = CanonExportState::new(strip_namespace, module_name);
    let mut line_count = 0usize;

    for line_res in lines {
        line_count = line_count.saturating_add(1);
        ensure!(
            line_count <= MAX_EXPORT_LINES,
            "lean4export stream exceeded maximum line bound"
        );
        let line = line_res?;
        if line.trim().is_empty() {
            continue;
        }
        let rec: Value = serde_json::from_str(&line).context("invalid JSON line in lean4export")?;
        if rec.get("meta").is_some() {
            continue;
        }

        if let Some(in_idx) = rec.get("in").and_then(Value::as_u64) {
            let (parent_idx, part) = if let Some(v) = rec.get("str") {
                let pre = v
                    .get("pre")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| anyhow::anyhow!("str name missing pre"))?;
                let s = v
                    .get("str")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow::anyhow!("str name missing str"))?;
                (pre, NamePart::Str(s.to_string()))
            } else if let Some(v) = rec.get("num") {
                let pre = v
                    .get("pre")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| anyhow::anyhow!("num name missing pre"))?;
                let n = match v.get("i") {
                    Some(Value::Number(num)) => num
                        .as_u64()
                        .ok_or_else(|| anyhow::anyhow!("num.i must be u64"))?,
                    Some(Value::String(s)) => s.parse::<u64>().context("num.i parse failed")?,
                    _ => bail!("malformed num.i"),
                };
                (pre, NamePart::Num(n))
            } else {
                bail!("malformed in record");
            };
            let mut full = st
                .old_names
                .get(&parent_idx)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("unknown parent name index {parent_idx}"))?;
            full.push(part);
            st.old_names.insert(in_idx, full);
            continue;
        }

        if let Some(old_il) = rec.get("il").and_then(Value::as_u64) {
            let new_il = st.next_il;
            st.next_il = st.next_il.saturating_add(1);
            let mapped = if let Some(succ) = rec.get("succ").and_then(Value::as_u64) {
                serde_json::json!({ "il": new_il, "succ": st.map_level(succ)? })
            } else if let Some(arr) = rec.get("max") {
                serde_json::json!({ "il": new_il, "max": st.map_level_array(arr)? })
            } else if let Some(arr) = rec.get("imax") {
                serde_json::json!({ "il": new_il, "imax": st.map_level_array(arr)? })
            } else if let Some(param) = rec.get("param").and_then(Value::as_u64) {
                let p = st.map_name(param)?;
                serde_json::json!({ "il": new_il, "param": p })
            } else {
                bail!("unrecognized il record");
            };
            st.old_il_to_canon.insert(old_il, new_il);
            st.emit_val(&mapped)?;
            continue;
        }

        if let Some(old_ie) = rec.get("ie").and_then(Value::as_u64) {
            let new_ie = st.next_ie;
            st.next_ie = st.next_ie.saturating_add(1);
            let mapped = if let Some(bvar) = rec.get("bvar") {
                serde_json::json!({ "ie": new_ie, "bvar": bvar })
            } else if let Some(nv) = rec.get("natVal") {
                serde_json::json!({ "ie": new_ie, "natVal": nv })
            } else if let Some(sv) = rec.get("strVal") {
                serde_json::json!({ "ie": new_ie, "strVal": sv })
            } else if let Some(sort) = rec.get("sort").and_then(Value::as_u64) {
                serde_json::json!({ "ie": new_ie, "sort": st.map_level(sort)? })
            } else if let Some(c) = rec.get("const") {
                let nm = c
                    .get("name")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| anyhow::anyhow!("const missing name"))?;
                let us = c
                    .get("us")
                    .ok_or_else(|| anyhow::anyhow!("const missing us"))?;
                let mapped_nm = st.map_name(nm)?;
                let mapped_us = st.map_level_array(us)?;
                serde_json::json!({
                    "ie": new_ie,
                    "const": { "name": mapped_nm, "us": mapped_us }
                })
            } else if let Some(app) = rec.get("app") {
                let f = app
                    .get("fn")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| anyhow::anyhow!("app missing fn"))?;
                let a = app
                    .get("arg")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| anyhow::anyhow!("app missing arg"))?;
                serde_json::json!({
                    "ie": new_ie,
                    "app": { "fn": st.map_expr(f)?, "arg": st.map_expr(a)? }
                })
            } else if let Some(lam) = rec.get("lam") {
                let nm = lam
                    .get("name")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| anyhow::anyhow!("lam missing name"))?;
                let ty = lam
                    .get("type")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| anyhow::anyhow!("lam missing type"))?;
                let bd = lam
                    .get("body")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| anyhow::anyhow!("lam missing body"))?;
                let bi = lam
                    .get("binderInfo")
                    .ok_or_else(|| anyhow::anyhow!("lam missing binderInfo"))?;
                let mapped_nm = st.map_name(nm)?;
                serde_json::json!({
                    "ie": new_ie,
                    "lam": {
                        "name": mapped_nm,
                        "type": st.map_expr(ty)?,
                        "body": st.map_expr(bd)?,
                        "binderInfo": bi
                    }
                })
            } else if let Some(fe) = rec.get("forallE") {
                let nm = fe
                    .get("name")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| anyhow::anyhow!("forallE missing name"))?;
                let ty = fe
                    .get("type")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| anyhow::anyhow!("forallE missing type"))?;
                let bd = fe
                    .get("body")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| anyhow::anyhow!("forallE missing body"))?;
                let bi = fe
                    .get("binderInfo")
                    .ok_or_else(|| anyhow::anyhow!("forallE missing binderInfo"))?;
                let mapped_nm = st.map_name(nm)?;
                serde_json::json!({
                    "ie": new_ie,
                    "forallE": {
                        "name": mapped_nm,
                        "type": st.map_expr(ty)?,
                        "body": st.map_expr(bd)?,
                        "binderInfo": bi
                    }
                })
            } else if let Some(le) = rec.get("letE") {
                let nm = le
                    .get("name")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| anyhow::anyhow!("letE missing name"))?;
                let ty = le
                    .get("type")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| anyhow::anyhow!("letE missing type"))?;
                let val = le
                    .get("value")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| anyhow::anyhow!("letE missing value"))?;
                let bd = le
                    .get("body")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| anyhow::anyhow!("letE missing body"))?;
                let nondep = le
                    .get("nondep")
                    .ok_or_else(|| anyhow::anyhow!("letE missing nondep"))?;
                let mapped_nm = st.map_name(nm)?;
                serde_json::json!({
                    "ie": new_ie,
                    "letE": {
                        "name": mapped_nm,
                        "type": st.map_expr(ty)?,
                        "value": st.map_expr(val)?,
                        "body": st.map_expr(bd)?,
                        "nondep": nondep
                    }
                })
            } else if let Some(proj) = rec.get("proj") {
                let tn = proj
                    .get("typeName")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| anyhow::anyhow!("proj missing typeName"))?;
                let idx = proj
                    .get("idx")
                    .ok_or_else(|| anyhow::anyhow!("proj missing idx"))?;
                let s_ie = proj
                    .get("struct")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| anyhow::anyhow!("proj missing struct"))?;
                let mapped_tn = st.map_name(tn)?;
                serde_json::json!({
                    "ie": new_ie,
                    "proj": {
                        "typeName": mapped_tn,
                        "idx": idx,
                        "struct": st.map_expr(s_ie)?
                    }
                })
            } else if let Some(md) = rec.get("mdata") {
                let d = md
                    .get("data")
                    .ok_or_else(|| anyhow::anyhow!("mdata missing data"))?;
                let e_ie = md
                    .get("expr")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| anyhow::anyhow!("mdata missing expr"))?;
                serde_json::json!({
                    "ie": new_ie,
                    "mdata": { "data": d, "expr": st.map_expr(e_ie)? }
                })
            } else {
                bail!("unrecognized ie record");
            };
            st.old_ie_to_canon.insert(old_ie, new_ie);
            st.emit_val(&mapped)?;
            continue;
        }

        if let Some(ax) = rec.get("axiom") {
            let nm = ax
                .get("name")
                .and_then(Value::as_u64)
                .ok_or_else(|| anyhow::anyhow!("axiom missing name"))?;
            let lp = ax
                .get("levelParams")
                .ok_or_else(|| anyhow::anyhow!("axiom missing levelParams"))?;
            let ty = ax
                .get("type")
                .and_then(Value::as_u64)
                .ok_or_else(|| anyhow::anyhow!("axiom missing type"))?;
            let iu = ax
                .get("isUnsafe")
                .ok_or_else(|| anyhow::anyhow!("axiom missing isUnsafe"))?;
            let m_nm = st.map_name(nm)?;
            let m_lp = st.map_name_array(lp)?;
            let mapped = serde_json::json!({
                "axiom": {
                    "name": m_nm,
                    "levelParams": m_lp,
                    "type": st.map_expr(ty)?,
                    "isUnsafe": iu
                }
            });
            st.emit_val(&mapped)?;
            continue;
        }

        if let Some(df) = rec.get("def") {
            let nm = df
                .get("name")
                .and_then(Value::as_u64)
                .ok_or_else(|| anyhow::anyhow!("def missing name"))?;
            let raw_parts = st
                .old_names
                .get(&nm)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("unknown def name index"))?;
            let norm_parts = normalize_name_parts(&raw_parts, &st.strip_ns, &st.mod_parts);
            if norm_parts == [NamePart::Str("statementProp".to_string())] {
                st.saw_statement_prop = true;
            }
            let lp = df
                .get("levelParams")
                .ok_or_else(|| anyhow::anyhow!("def missing levelParams"))?;
            let ty = df
                .get("type")
                .and_then(Value::as_u64)
                .ok_or_else(|| anyhow::anyhow!("def missing type"))?;
            let val = df
                .get("value")
                .and_then(Value::as_u64)
                .ok_or_else(|| anyhow::anyhow!("def missing value"))?;
            let hints = df
                .get("hints")
                .ok_or_else(|| anyhow::anyhow!("def missing hints"))?;
            let safety = df
                .get("safety")
                .ok_or_else(|| anyhow::anyhow!("def missing safety"))?;
            let all = df
                .get("all")
                .ok_or_else(|| anyhow::anyhow!("def missing all"))?;
            let m_nm = st.map_name(nm)?;
            let m_lp = st.map_name_array(lp)?;
            let m_all = st.map_name_array(all)?;
            let mapped = serde_json::json!({
                "def": {
                    "name": m_nm,
                    "levelParams": m_lp,
                    "type": st.map_expr(ty)?,
                    "value": st.map_expr(val)?,
                    "hints": hints,
                    "safety": safety,
                    "all": m_all
                }
            });
            st.emit_val(&mapped)?;
            continue;
        }

        if let Some(op) = rec.get("opaque") {
            let nm = op
                .get("name")
                .and_then(Value::as_u64)
                .ok_or_else(|| anyhow::anyhow!("opaque missing name"))?;
            let lp = op
                .get("levelParams")
                .ok_or_else(|| anyhow::anyhow!("opaque missing levelParams"))?;
            let ty = op
                .get("type")
                .and_then(Value::as_u64)
                .ok_or_else(|| anyhow::anyhow!("opaque missing type"))?;
            let val = op
                .get("value")
                .and_then(Value::as_u64)
                .ok_or_else(|| anyhow::anyhow!("opaque missing value"))?;
            let all = op
                .get("all")
                .ok_or_else(|| anyhow::anyhow!("opaque missing all"))?;
            let iu = op
                .get("isUnsafe")
                .ok_or_else(|| anyhow::anyhow!("opaque missing isUnsafe"))?;
            let m_nm = st.map_name(nm)?;
            let m_lp = st.map_name_array(lp)?;
            let m_all = st.map_name_array(all)?;
            let mapped = serde_json::json!({
                "opaque": {
                    "name": m_nm,
                    "levelParams": m_lp,
                    "type": st.map_expr(ty)?,
                    "value": st.map_expr(val)?,
                    "all": m_all,
                    "isUnsafe": iu
                }
            });
            st.emit_val(&mapped)?;
            continue;
        }

        if let Some(th) = rec.get("thm") {
            let nm = th
                .get("name")
                .and_then(Value::as_u64)
                .ok_or_else(|| anyhow::anyhow!("thm missing name"))?;
            let lp = th
                .get("levelParams")
                .ok_or_else(|| anyhow::anyhow!("thm missing levelParams"))?;
            let ty = th
                .get("type")
                .and_then(Value::as_u64)
                .ok_or_else(|| anyhow::anyhow!("thm missing type"))?;
            let val = th
                .get("value")
                .and_then(Value::as_u64)
                .ok_or_else(|| anyhow::anyhow!("thm missing value"))?;
            let all = th
                .get("all")
                .ok_or_else(|| anyhow::anyhow!("thm missing all"))?;
            let m_nm = st.map_name(nm)?;
            let m_lp = st.map_name_array(lp)?;
            let m_all = st.map_name_array(all)?;
            let mapped = serde_json::json!({
                "thm": {
                    "name": m_nm,
                    "levelParams": m_lp,
                    "type": st.map_expr(ty)?,
                    "value": st.map_expr(val)?,
                    "all": m_all
                }
            });
            st.emit_val(&mapped)?;
            continue;
        }

        if let Some(qt) = rec.get("quot") {
            let nm = qt
                .get("name")
                .and_then(Value::as_u64)
                .ok_or_else(|| anyhow::anyhow!("quot missing name"))?;
            let lp = qt
                .get("levelParams")
                .ok_or_else(|| anyhow::anyhow!("quot missing levelParams"))?;
            let ty = qt
                .get("type")
                .and_then(Value::as_u64)
                .ok_or_else(|| anyhow::anyhow!("quot missing type"))?;
            let kind = qt
                .get("kind")
                .ok_or_else(|| anyhow::anyhow!("quot missing kind"))?;
            let m_nm = st.map_name(nm)?;
            let m_lp = st.map_name_array(lp)?;
            let mapped = serde_json::json!({
                "quot": {
                    "name": m_nm,
                    "levelParams": m_lp,
                    "type": st.map_expr(ty)?,
                    "kind": kind
                }
            });
            st.emit_val(&mapped)?;
            continue;
        }

        if let Some(ind) = rec.get("inductive") {
            let raw_types = ind
                .get("types")
                .and_then(Value::as_array)
                .ok_or_else(|| anyhow::anyhow!("inductive missing types"))?;
            let raw_ctors = ind
                .get("ctors")
                .and_then(Value::as_array)
                .ok_or_else(|| anyhow::anyhow!("inductive missing ctors"))?;
            let raw_recs = ind
                .get("recs")
                .and_then(Value::as_array)
                .ok_or_else(|| anyhow::anyhow!("inductive missing recs"))?;

            let mut m_types = Vec::with_capacity(raw_types.len());
            for iv in raw_types {
                let nm = iv
                    .get("name")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| anyhow::anyhow!("inductive type missing name"))?;
                let lp = iv
                    .get("levelParams")
                    .ok_or_else(|| anyhow::anyhow!("inductive type missing levelParams"))?;
                let ty = iv
                    .get("type")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| anyhow::anyhow!("inductive type missing type"))?;
                let all = iv
                    .get("all")
                    .ok_or_else(|| anyhow::anyhow!("inductive type missing all"))?;
                let ctors = iv
                    .get("ctors")
                    .ok_or_else(|| anyhow::anyhow!("inductive type missing ctors"))?;
                let m_nm = st.map_name(nm)?;
                let m_lp = st.map_name_array(lp)?;
                let m_all = st.map_name_array(all)?;
                let m_ct = st.map_name_array(ctors)?;
                m_types.push(serde_json::json!({
                    "name": m_nm,
                    "levelParams": m_lp,
                    "type": st.map_expr(ty)?,
                    "numParams": iv.get("numParams"),
                    "numIndices": iv.get("numIndices"),
                    "all": m_all,
                    "ctors": m_ct,
                    "numNested": iv.get("numNested"),
                    "isRec": iv.get("isRec"),
                    "isReflexive": iv.get("isReflexive"),
                    "isUnsafe": iv.get("isUnsafe")
                }));
            }

            let mut m_ctors = Vec::with_capacity(raw_ctors.len());
            for cv in raw_ctors {
                let nm = cv
                    .get("name")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| anyhow::anyhow!("ctor missing name"))?;
                let lp = cv
                    .get("levelParams")
                    .ok_or_else(|| anyhow::anyhow!("ctor missing levelParams"))?;
                let ty = cv
                    .get("type")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| anyhow::anyhow!("ctor missing type"))?;
                let ind_nm = cv
                    .get("induct")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| anyhow::anyhow!("ctor missing induct"))?;
                let m_nm = st.map_name(nm)?;
                let m_lp = st.map_name_array(lp)?;
                let m_ind = st.map_name(ind_nm)?;
                m_ctors.push(serde_json::json!({
                    "name": m_nm,
                    "levelParams": m_lp,
                    "type": st.map_expr(ty)?,
                    "induct": m_ind,
                    "cidx": cv.get("cidx"),
                    "numParams": cv.get("numParams"),
                    "numFields": cv.get("numFields"),
                    "isUnsafe": cv.get("isUnsafe")
                }));
            }

            let mut m_recs = Vec::with_capacity(raw_recs.len());
            for rv in raw_recs {
                let nm = rv
                    .get("name")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| anyhow::anyhow!("rec missing name"))?;
                let lp = rv
                    .get("levelParams")
                    .ok_or_else(|| anyhow::anyhow!("rec missing levelParams"))?;
                let ty = rv
                    .get("type")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| anyhow::anyhow!("rec missing type"))?;
                let all = rv
                    .get("all")
                    .ok_or_else(|| anyhow::anyhow!("rec missing all"))?;
                let raw_rules = rv
                    .get("rules")
                    .and_then(Value::as_array)
                    .ok_or_else(|| anyhow::anyhow!("rec missing rules"))?;
                let m_nm = st.map_name(nm)?;
                let m_lp = st.map_name_array(lp)?;
                let m_all = st.map_name_array(all)?;
                let mut m_rules = Vec::with_capacity(raw_rules.len());
                for rule in raw_rules {
                    let ctor = rule
                        .get("ctor")
                        .and_then(Value::as_u64)
                        .ok_or_else(|| anyhow::anyhow!("rec rule missing ctor"))?;
                    let rhs = rule
                        .get("rhs")
                        .and_then(Value::as_u64)
                        .ok_or_else(|| anyhow::anyhow!("rec rule missing rhs"))?;
                    let m_ctor = st.map_name(ctor)?;
                    m_rules.push(serde_json::json!({
                        "ctor": m_ctor,
                        "nfields": rule.get("nfields"),
                        "rhs": st.map_expr(rhs)?
                    }));
                }
                m_recs.push(serde_json::json!({
                    "name": m_nm,
                    "levelParams": m_lp,
                    "type": st.map_expr(ty)?,
                    "all": m_all,
                    "numParams": rv.get("numParams"),
                    "numIndices": rv.get("numIndices"),
                    "numMotives": rv.get("numMotives"),
                    "numMinors": rv.get("numMinors"),
                    "rules": m_rules,
                    "k": rv.get("k"),
                    "isUnsafe": rv.get("isUnsafe")
                }));
            }

            let mapped = serde_json::json!({
                "inductive": {
                    "types": m_types,
                    "ctors": m_ctors,
                    "recs": m_recs
                }
            });
            st.emit_val(&mapped)?;
            continue;
        }

        bail!("unrecognized lean4export record: {line}");
    }

    ensure!(
        st.saw_statement_prop,
        "lean4export output did not export statementProp"
    );
    Ok(st.out_lines)
}

fn compose_standalone_statement_module(raw_statement: &str) -> Result<String> {
    let mut swarm_lib_imports = BTreeSet::new();
    let mut generated_dep_imports = BTreeSet::new();
    let mut body_lines = Vec::new();

    for line in raw_statement.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("import ") || trimmed.starts_with("import\t") {
            let without_comment = trimmed.split("--").next().unwrap_or(trimmed);
            for mod_name in without_comment
                .trim_start_matches("import")
                .split_whitespace()
            {
                if mod_name.starts_with(GENERATED_MODULE_PREFIX) {
                    generated_dep_imports.insert(mod_name.to_string());
                } else if mod_name.starts_with("SwarmProofs") {
                    ensure!(
                        ALLOWED_SWARM_LIBRARY_IMPORTS.contains(&mod_name),
                        "noncanonical SwarmProofs import {mod_name} in standalone statement"
                    );
                    swarm_lib_imports.insert(mod_name.to_string());
                }
            }
            continue;
        }
        body_lines.push(line);
    }

    let mut out =
        String::from("/- SPDX-License-Identifier: Apache-2.0 -/\nimport Mathlib\nimport Zeta23\n");
    for lib_imp in &swarm_lib_imports {
        out.push_str(&format!("import {lib_imp}\n"));
    }
    for gen_imp in &generated_dep_imports {
        out.push_str(&format!("import {gen_imp}\n"));
    }
    out.push('\n');
    out.push_str(&body_lines.join("\n"));
    out.push('\n');
    Ok(out)
}

struct TempScratchArtifacts {
    paths: Vec<std::path::PathBuf>,
}

impl Drop for TempScratchArtifacts {
    fn drop(&mut self) {
        for p in &self.paths {
            let _ = std::fs::remove_file(p);
        }
    }
}

pub fn check_statement_expr_stability(
    package_root: &Path,
    exporter_bin: &Path,
    module_name: &str,
    standalone_statement_file: &Path,
    generated_namespace: &str,
) -> Result<()> {
    ensure!(
        package_root.is_dir(),
        "package root must exist: {}",
        package_root.display()
    );
    ensure!(
        exporter_bin.is_file(),
        "lean4export binary must exist: {}",
        exporter_bin.display()
    );
    ensure!(!module_name.is_empty(), "module_name must be non-empty");
    ensure!(
        !generated_namespace.is_empty(),
        "generated_namespace must be non-empty"
    );

    let raw_statement = std::fs::read_to_string(standalone_statement_file).with_context(|| {
        format!(
            "failed reading standalone statement {}",
            standalone_statement_file.display()
        )
    })?;
    let standalone_src = compose_standalone_statement_module(&raw_statement)?;
    let hash_tag = &sha256_hex(standalone_src.as_bytes())[..16];
    let scratch_mod = format!("SwarmExprStability_{}_{hash_tag}", std::process::id());

    let work_dir = package_root.join(".lake").join("expr-stability");
    let lib_dir = package_root
        .join(".lake")
        .join("build")
        .join("lib")
        .join("lean");
    std::fs::create_dir_all(&work_dir)?;
    std::fs::create_dir_all(&lib_dir)?;

    let standalone_lean = work_dir.join(format!("{scratch_mod}.lean"));
    let standalone_olean = lib_dir.join(format!("{scratch_mod}.olean"));
    let standalone_export = work_dir.join(format!("{scratch_mod}.standalone.jsonl"));
    let module_export = work_dir.join(format!("{scratch_mod}.namespaced.jsonl"));

    let _cleanup = TempScratchArtifacts {
        paths: vec![
            standalone_lean.clone(),
            standalone_olean.clone(),
            standalone_export.clone(),
            module_export.clone(),
        ],
    };

    std::fs::write(&standalone_lean, &standalone_src)?;

    let compile_status = Command::new("lake")
        .current_dir(package_root)
        .stdin(Stdio::null())
        .arg("env")
        .arg("lean")
        .arg("-o")
        .arg(&standalone_olean)
        .arg(&standalone_lean)
        .status()
        .context("failed invoking lake env lean on standalone statement")?;
    ensure!(
        compile_status.success(),
        "standalone statement failed elaboration for {}",
        standalone_statement_file.display()
    );

    {
        let out_file = File::create(&standalone_export)?;
        let status = Command::new("lake")
            .current_dir(package_root)
            .stdin(Stdio::null())
            .stdout(Stdio::from(out_file))
            .arg("env")
            .arg(exporter_bin)
            .arg(&scratch_mod)
            .arg("--")
            .arg("statementProp")
            .status()
            .context("failed invoking lean4export on standalone statementProp")?;
        ensure!(
            status.success(),
            "lean4export failed on standalone {scratch_mod}.statementProp"
        );
    }

    let namespaced_decl = format!("{generated_namespace}.statementProp");
    {
        let out_file = File::create(&module_export)?;
        let status = Command::new("lake")
            .current_dir(package_root)
            .stdin(Stdio::null())
            .stdout(Stdio::from(out_file))
            .arg("env")
            .arg(exporter_bin)
            .arg(module_name)
            .arg("--")
            .arg(&namespaced_decl)
            .status()
            .context("failed invoking lean4export on generated statementProp")?;
        ensure!(
            status.success(),
            "lean4export failed on {namespaced_decl} in {module_name}"
        );
    }

    let standalone_canon = canonicalize_lean4export_stream(
        BufReader::new(File::open(&standalone_export)?).lines(),
        None,
        Some(&scratch_mod),
    )?;
    let module_canon = canonicalize_lean4export_stream(
        BufReader::new(File::open(&module_export)?).lines(),
        Some(generated_namespace),
        Some(module_name),
    )?;

    let standalone_sha = sha256_hex(standalone_canon.join("\n").as_bytes());
    let module_sha = sha256_hex(module_canon.join("\n").as_bytes());
    ensure!(
        standalone_sha == module_sha,
        "statementProp kernel Expr drift detected for {module_name} ({namespaced_decl}): standalone sha256={standalone_sha} != module sha256={module_sha}"
    );
    println!(
        "verified statementProp Expr stability for {module_name} ({namespaced_decl} == sha256:{standalone_sha})"
    );
    Ok(())
}

pub fn check_promoted_statements_expr_stability(
    package_root: &Path,
    exporter_bin: &Path,
    candidates_dir: &Path,
) -> Result<()> {
    let reg_path = package_root.join("registry.json");
    let reg: RegistryManifest = serde_json::from_slice(&std::fs::read(&reg_path)?)?;
    let work_dir = package_root.join(".lake").join("expr-stability");
    std::fs::create_dir_all(&work_dir)?;

    let mut checked = 0usize;
    let mut entries: Vec<_> = std::fs::read_dir(candidates_dir)?
        .filter_map(|e| e.ok().map(|it| it.path()))
        .collect();
    entries.sort();

    for path in entries {
        let Some(fname) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if !fname.ends_with(".json") || fname == "manifest.json" {
            continue;
        }
        let cand: PromotionCandidate = serde_json::from_slice(&std::fs::read(&path)?)?;
        let matched = reg
            .modules
            .iter()
            .find(|m| {
                m.originating_task_id == cand.task_id && m.originating_network == cand.network
            })
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "candidate {} ({}) missing from registry.json during expr stability check",
                    cand.task_id,
                    cand.network
                )
            })?;
        let (ns, _) = matched
            .theorem_name
            .rsplit_once('.')
            .ok_or_else(|| anyhow::anyhow!("unqualified theorem_name {}", matched.theorem_name))?;
        let stmt_tmp = work_dir.join(format!("cand_{fname}.lean"));
        std::fs::write(&stmt_tmp, &cand.statement)?;
        let res = check_statement_expr_stability(
            package_root,
            exporter_bin,
            &matched.module_name,
            &stmt_tmp,
            ns,
        );
        let _ = std::fs::remove_file(&stmt_tmp);
        res?;
        checked += 1;
    }
    println!("checked statementProp Expr stability on {checked} promoted candidate(s)");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn run_coverage(names: &[&str], records: &[Value]) -> Result<CoverageReport> {
        let inventory = json!({
            "declarations": names.iter().map(|n| json!({ "name": n })).collect::<Vec<_>>()
        });
        let lines = records
            .iter()
            .map(|r| Ok::<String, std::io::Error>(serde_json::to_string(r).unwrap()));
        check_coverage(&inventory, lines)
    }

    #[test]
    fn every_helper_required() {
        let records = vec![
            json!({"in": 1, "str": {"pre": 0, "str": "proof"}}),
            json!({"thm": {"name": 1}}),
        ];
        let err = run_coverage(&["proof", "privateHelper"], &records)
            .unwrap_err()
            .to_string();
        assert!(err.contains("omitted"), "unexpected error: {err}");
        let ok = run_coverage(&["proof"], &records).unwrap();
        assert_eq!(ok.roots, vec!["proof".to_string()]);
    }

    #[test]
    fn forbidden_axiom_even_when_not_a_root() {
        let records = vec![
            json!({"in": 1, "str": {"pre": 0, "str": "sorryAx"}}),
            json!({"axiom": {"name": 1}}),
        ];
        let err = run_coverage(&[], &records).unwrap_err().to_string();
        assert!(err.contains("unapproved"), "unexpected error: {err}");
    }

    #[test]
    fn unsafe_rejected() {
        let records = vec![
            json!({"in": 1, "str": {"pre": 0, "str": "helper"}}),
            json!({"def": {"name": 1, "safety": "unsafe"}}),
        ];
        let err = run_coverage(&["helper"], &records).unwrap_err().to_string();
        assert!(err.contains("unsafe"), "unexpected error: {err}");
    }

    #[test]
    fn inductive_constructors_and_recursors_required() {
        let records = vec![
            json!({"in": 1, "str": {"pre": 0, "str": "T"}}),
            json!({"in": 2, "str": {"pre": 0, "str": "T.mk"}}),
            json!({"in": 3, "str": {"pre": 0, "str": "T.rec"}}),
            json!({"inductive": {"types": [{"name": 1}], "ctors": [{"name": 2}], "recs": [{"name": 3}]}}),
        ];
        let report = run_coverage(&["T", "T.mk", "T.rec"], &records).unwrap();
        assert_eq!(report.exported_declarations.len(), 3);
    }

    #[test]
    fn axiom_audit_handles_constructive_zero_axiom_and_standard_theorems() {
        let audit = "'SwarmProofs.Constructive.proof' does not depend on any axioms\n\
                     'SwarmProofs.Classical.proof' depends on axioms: [propext, Classical.choice, Quot.sound]\n";
        let reg = json!({
            "modules": [
                {"status": "active", "theorem_name": "SwarmProofs.Constructive.proof"},
                {"status": "active", "theorem_name": "SwarmProofs.Classical.proof"},
                {"status": "revoked", "theorem_name": "SwarmProofs.Old.proof"}
            ]
        });
        let summary = verify_axiom_audit_text(audit, Some(&reg)).unwrap();
        assert_eq!(summary, "audited 2 exported theorem(s)");
    }

    #[test]
    fn axiom_audit_accepts_empty_initial_registry_and_rejects_errors() {
        let empty_reg = json!({"modules": []});
        assert_eq!(
            verify_axiom_audit_text("", Some(&empty_reg)).unwrap(),
            "audited empty initial registry"
        );
        assert!(verify_axiom_audit_text("", None).is_err());
        assert!(
            verify_axiom_audit_text("'X.proof' depends on axioms: [sorryAx]", None)
                .unwrap_err()
                .to_string()
                .contains("forbidden axiom")
        );
        assert!(
            verify_axiom_audit_text("'X.proof' depends on axioms: []", None)
                .unwrap_err()
                .to_string()
                .contains("malformed empty axiom list")
        );
        assert!(verify_axiom_audit_text(
            "PromotionAudit.lean:2:0: error: unknown identifier",
            None
        )
        .unwrap_err()
        .to_string()
        .contains("lean error"));
    }

    #[test]
    fn prepare_promotion_audit_is_idempotent_and_restorable() {
        let dir = std::env::temp_dir().join(format!("xtask-audit-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("registry.json"),
            serde_json::to_vec(&json!({
                "modules": [
                    {
                        "module_id": "sha256:1111",
                        "module_name": "SwarmProofs.Generated.H1111",
                        "theorem_name": "SwarmProofs.Generated.ArtifactH2222.proof",
                        "source_sha256": "00",
                        "source_bytes": 1,
                        "status": "active"
                    }
                ]
            }))
            .unwrap(),
        )
        .unwrap();
        let original_lake = "name = \"SwarmProofs\"\n";
        std::fs::write(dir.join("lakefile.toml"), original_lake).unwrap();

        prepare_proof_promotion_audit(&dir).unwrap();
        let first_lake = std::fs::read_to_string(dir.join("lakefile.toml")).unwrap();
        prepare_proof_promotion_audit(&dir).unwrap();
        let second_lake = std::fs::read_to_string(dir.join("lakefile.toml")).unwrap();
        assert_eq!(first_lake, second_lake);
        assert_eq!(
            std::fs::read_to_string(dir.join("lakefile.toml.original")).unwrap(),
            original_lake
        );
        let audit_lean = std::fs::read_to_string(dir.join("PromotionAudit.lean")).unwrap();
        assert!(audit_lean.contains("import SwarmProofs.Generated.H1111"));
        assert!(audit_lean.contains("#print axioms SwarmProofs.Generated.ArtifactH2222.proof"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn canonicalize_lean4export_normalizes_generated_namespace_and_detects_expr_tampering() {
        let standalone = [
            json!({"meta": {"lean": "4.33.0-rc2"}}),
            json!({"in": 1, "str": {"pre": 0, "str": "True"}}),
            json!({"ie": 0, "sort": 0}),
            json!({"ie": 1, "const": {"name": 1, "us": []}}),
            json!({"in": 2, "str": {"pre": 0, "str": "statementProp"}}),
            json!({"def": {"name": 2, "levelParams": [], "type": 0, "value": 1, "hints": {"regular": 1}, "safety": "safe", "all": [2]}}),
        ];
        let namespaced_same = [
            json!({"meta": {"lean": "4.33.0-rc2"}}),
            json!({"in": 1, "str": {"pre": 0, "str": "True"}}),
            json!({"ie": 0, "sort": 0}),
            json!({"ie": 1, "const": {"name": 1, "us": []}}),
            json!({"in": 2, "str": {"pre": 0, "str": "SwarmProofs"}}),
            json!({"in": 3, "str": {"pre": 2, "str": "Generated"}}),
            json!({"in": 4, "str": {"pre": 3, "str": "ArtifactH9999"}}),
            json!({"in": 5, "str": {"pre": 4, "str": "statementProp"}}),
            json!({"def": {"name": 5, "levelParams": [], "type": 0, "value": 1, "hints": {"regular": 1}, "safety": "safe", "all": [5]}}),
        ];
        let namespaced_tampered = [
            json!({"meta": {"lean": "4.33.0-rc2"}}),
            json!({"in": 1, "str": {"pre": 0, "str": "False"}}),
            json!({"ie": 0, "sort": 0}),
            json!({"ie": 1, "const": {"name": 1, "us": []}}),
            json!({"in": 2, "str": {"pre": 0, "str": "SwarmProofs"}}),
            json!({"in": 3, "str": {"pre": 2, "str": "Generated"}}),
            json!({"in": 4, "str": {"pre": 3, "str": "ArtifactH9999"}}),
            json!({"in": 5, "str": {"pre": 4, "str": "statementProp"}}),
            json!({"def": {"name": 5, "levelParams": [], "type": 0, "value": 1, "hints": {"regular": 1}, "safety": "safe", "all": [5]}}),
        ];

        let c_standalone = canonicalize_lean4export_stream(
            standalone
                .iter()
                .map(|v| Ok::<_, std::io::Error>(v.to_string())),
            None,
            Some("ScratchMod"),
        )
        .unwrap();
        let c_same = canonicalize_lean4export_stream(
            namespaced_same
                .iter()
                .map(|v| Ok::<_, std::io::Error>(v.to_string())),
            Some("SwarmProofs.Generated.ArtifactH9999"),
            Some("SwarmProofs.Generated.H1111"),
        )
        .unwrap();
        let c_tampered = canonicalize_lean4export_stream(
            namespaced_tampered
                .iter()
                .map(|v| Ok::<_, std::io::Error>(v.to_string())),
            Some("SwarmProofs.Generated.ArtifactH9999"),
            Some("SwarmProofs.Generated.H1111"),
        )
        .unwrap();

        assert_eq!(c_standalone, c_same);
        assert_ne!(
            sha256_hex(c_standalone.join("\n").as_bytes()),
            sha256_hex(c_tampered.join("\n").as_bytes())
        );
    }

    #[test]
    fn swarm_proofs_frozen_library_modules_golden_and_mutation_guard() {
        let pkg_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .unwrap()
            .join("proofs")
            .join("SwarmProofs");
        let reg: RegistryManifest =
            serde_json::from_slice(&std::fs::read(pkg_root.join("registry.json")).unwrap())
                .unwrap();
        proof_registry::validate_frozen_library_modules(&pkg_root, &reg).unwrap();

        let tmp = std::env::temp_dir().join(format!("xtask-frozen-defs-{}", uuid::Uuid::new_v4()));
        for rel in [
            "SwarmProofs/Registry.lean",
            "SwarmProofs/Chains/BSD/Defs/V1.lean",
            "SwarmProofs/Chains/Circuits/Defs/V1.lean",
            "SwarmProofs/Chains/RHAnalytic/Defs/V1.lean",
            "SwarmProofs/Chains/RHLagarias/Defs/V1.lean",
            "environment-lock.json",
            "registry.json",
        ] {
            let dst = tmp.join(rel);
            std::fs::create_dir_all(dst.parent().unwrap()).unwrap();
            std::fs::copy(pkg_root.join(rel), &dst).unwrap();
        }
        proof_registry::validate_frozen_library_modules(&tmp, &reg).unwrap();

        let target = tmp.join("SwarmProofs/Chains/RHAnalytic/Defs/V1.lean");
        let mut bytes = std::fs::read(&target).unwrap();
        bytes[0] ^= 0x01;
        std::fs::write(&target, &bytes).unwrap();
        assert!(proof_registry::validate_frozen_library_modules(&tmp, &reg).is_err());

        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn check_frozen_defs_append_only_allows_v2_addition_and_rejects_v1_mutation_or_deletion() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .unwrap();
        let pkg_root = root.join("proofs").join("SwarmProofs");
        let base_reg_path = pkg_root.join("registry.json");

        let tmp = std::env::temp_dir().join(format!("xtask-append-only-{}", uuid::Uuid::new_v4()));
        for rel in [
            "SwarmProofs/Registry.lean",
            "SwarmProofs/Chains/BSD/Defs/V1.lean",
            "SwarmProofs/Chains/Circuits/Defs/V1.lean",
            "SwarmProofs/Chains/RHAnalytic/Defs/V1.lean",
            "SwarmProofs/Chains/RHLagarias/Defs/V1.lean",
            "environment-lock.json",
            "registry.json",
        ] {
            let dst = tmp.join(rel);
            std::fs::create_dir_all(dst.parent().unwrap()).unwrap();
            std::fs::copy(pkg_root.join(rel), &dst).unwrap();
        }

        // 1. Unchanged -> Ok
        check_frozen_defs_append_only(root, &tmp, Some(&base_reg_path)).unwrap();

        // 2. Adding a new V2 module on disk + in registry.json -> Ok (append-only)
        let v2_bytes = b"/- SPDX-License-Identifier: Apache-2.0 -/\ndef v2Helper : Nat := 42\n";
        let v2_path = tmp.join("SwarmProofs/Chains/BSD/Defs/V2.lean");
        std::fs::write(&v2_path, v2_bytes).unwrap();
        let mut head_reg: RegistryManifest =
            serde_json::from_slice(&std::fs::read(tmp.join("registry.json")).unwrap()).unwrap();
        head_reg.environments[0].frozen_library_modules.insert(
            "SwarmProofs.Chains.BSD.Defs.V2".into(),
            format!("sha256:{}", sha256_hex(v2_bytes)),
        );
        std::fs::write(
            tmp.join("registry.json"),
            serde_json::to_vec_pretty(&head_reg).unwrap(),
        )
        .unwrap();
        check_frozen_defs_append_only(root, &tmp, Some(&base_reg_path)).unwrap();

        // 3. Mutating existing V1 on disk AND updating head registry.json -> Error!
        let bsd_v1_path = tmp.join("SwarmProofs/Chains/BSD/Defs/V1.lean");
        let orig_bsd_v1 = std::fs::read(&bsd_v1_path).unwrap();
        let mut mutated_v1 = orig_bsd_v1.clone();
        mutated_v1.push(b'\n');
        std::fs::write(&bsd_v1_path, &mutated_v1).unwrap();
        let mut mutated_reg = head_reg.clone();
        mutated_reg.environments[0].frozen_library_modules.insert(
            "SwarmProofs.Chains.BSD.Defs.V1".into(),
            format!("sha256:{}", sha256_hex(&mutated_v1)),
        );
        std::fs::write(
            tmp.join("registry.json"),
            serde_json::to_vec_pretty(&mutated_reg).unwrap(),
        )
        .unwrap();
        assert!(check_frozen_defs_append_only(root, &tmp, Some(&base_reg_path)).is_err());

        // 4. Deleting existing V1 on disk + in head registry.json -> Error!
        std::fs::remove_file(&bsd_v1_path).unwrap();
        let mut deleted_reg = head_reg;
        deleted_reg.environments[0]
            .frozen_library_modules
            .remove("SwarmProofs.Chains.BSD.Defs.V1");
        std::fs::write(
            tmp.join("registry.json"),
            serde_json::to_vec_pretty(&deleted_reg).unwrap(),
        )
        .unwrap();
        assert!(check_frozen_defs_append_only(root, &tmp, Some(&base_reg_path)).is_err());

        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
