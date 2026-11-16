use anyhow::{bail, ensure, Context, Result};
use proof_registry::{sha256_hex, ALLOWED_AXIOMS};
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

pub fn check_proof_sources(repo_root: &Path, package_root: &Path) -> Result<()> {
    let registry_path = package_root.join("registry.json");
    let policy_script = repo_root.join("scripts").join("proof-source-policy.lean");
    ensure!(
        registry_path.is_file(),
        "missing {}",
        registry_path.display()
    );
    ensure!(
        policy_script.is_file(),
        "missing {}",
        policy_script.display()
    );

    let registry: RegistryFile = serde_json::from_slice(&std::fs::read(&registry_path)?)?;
    let modules_by_id: BTreeMap<&str, &RegistryModule> = registry
        .modules
        .iter()
        .map(|m| (m.module_id.as_str(), m))
        .collect();

    for module in &registry.modules {
        verify_single_module_source(package_root, &policy_script, module, &modules_by_id)?;
    }
    Ok(())
}

fn verify_single_module_source(
    package_root: &Path,
    policy_script: &Path,
    module: &RegistryModule,
    modules_by_id: &BTreeMap<&str, &RegistryModule>,
) -> Result<()> {
    ensure!(
        !module.module_name.contains('/') && !module.module_name.contains(".."),
        "invalid module_name: {}",
        module.module_name
    );
    let source_path = package_root.join(format!("{}.lean", module.module_name.replace('.', "/")));
    let payload = std::fs::read(&source_path)
        .with_context(|| format!("missing module source {}", source_path.display()))?;
    if sha256_hex(&payload) != module.source_sha256 || payload.len() as u64 != module.source_bytes {
        bail!("source integrity mismatch: {}", module.module_id);
    }

    let closure = resolve_dependency_names(module, modules_by_id)?;
    let (namespace, _) = module
        .theorem_name
        .rsplit_once('.')
        .ok_or_else(|| anyhow::anyhow!("unqualified theorem_name: {}", module.theorem_name))?;

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
    for allowed_mod in &closure {
        cmd.arg(allowed_mod);
    }
    let status = cmd
        .status()
        .context("failed to execute `lake env lean --run proof-source-policy.lean`")?;
    ensure!(
        status.success(),
        "proof-source-policy failed for {}",
        module.module_id
    );
    Ok(())
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
}
