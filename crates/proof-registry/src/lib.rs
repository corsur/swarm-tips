//! Canonical schemas and hashes for composable Lean proof environments.
//!
//! The wire representation deliberately uses only strings, integers, booleans,
//! arrays, and objects. [`canonical_json`] therefore implements the RFC 8785
//! rules relevant to this schema without accepting floating-point values.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const SCHEMA_VERSION: &str = "swarm.lean-proof-registry/v1";
pub const CHALLENGE_SCHEMA_VERSION: &str = "swarm.lean-challenge/v1";
pub const MODULE_ID_PREFIX: &str = "sha256:";
pub const APACHE_2_0: &str = "Apache-2.0";
pub const GENERATED_MODULE_PREFIX: &str = "SwarmProofs.Generated.H";
pub const MAX_DIRECT_DEPENDENCIES: usize = 16;
pub const MAX_TRANSITIVE_DEPENDENCIES: usize = 128;
pub const MAX_DEPENDENCY_DEPTH: usize = 16;
pub const MAX_CLOSURE_SOURCE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalPackage {
    pub name: String,
    pub repository: String,
    pub revision: String,
    pub subdirectory: Option<String>,
    pub license: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentLock {
    pub schema: String,
    pub name: String,
    pub toolchain: String,
    pub mathlib_revision: String,
    pub policy_version: u32,
    pub registry_schema: String,
    pub allowed_import_roots: Vec<String>,
    pub external_packages: Vec<ExternalPackage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleStatus {
    Active,
    Deprecated,
    Revoked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleVerification {
    pub lean_kernel: bool,
    pub axiom_audit: bool,
    pub independent_kernel: bool,
    pub axioms: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofModuleManifest {
    pub schema: String,
    /// `sha256:<hex>`; omitted from the hashed preimage when deriving the id.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub module_id: String,
    pub module_name: String,
    pub theorem_name: String,
    pub theorem_signature: String,
    pub environment_id: String,
    pub source_sha256: String,
    pub source_url: String,
    pub source_bytes: u64,
    pub direct_dependencies: Vec<String>,
    pub originating_task_id: String,
    pub originating_network: String,
    pub author_wallet: String,
    pub license: String,
    pub verification: ModuleVerification,
    pub status: ModuleStatus,
    pub replacement: Option<String>,
    pub title: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyBundle {
    pub schema: String,
    pub environment_id: String,
    /// Complete dependency closure in deterministic topological order.
    pub modules: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChallengeManifest {
    pub schema: String,
    pub statement_sha256: String,
    pub policy_version: u32,
    pub environment_id: String,
    pub dependency_bundle_id: String,
    pub reuse_license: String,
    pub reusable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromotionCandidate {
    pub schema: String,
    pub task_id: String,
    pub campaign_id: String,
    pub network: String,
    pub author_wallet: String,
    pub artifact_sha256: String,
    pub statement: String,
    pub proof_source: String,
    pub challenge: ChallengeManifest,
    pub direct_dependencies: Vec<String>,
    pub dependency_closure: Vec<String>,
    pub exact_imports: Vec<String>,
    pub verification_axioms: Vec<String>,
    pub status: String,
}

pub struct GeneratedModule {
    pub source: String,
    pub theorem_name: String,
    pub theorem_signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryManifest {
    pub schema: String,
    pub environments: Vec<EnvironmentRecord>,
    pub modules: Vec<ProofModuleManifest>,
    pub aliases: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentRecord {
    pub environment_id: String,
    pub lock: EnvironmentLock,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RegistryError {
    #[error("canonical proof-registry JSON does not permit floating-point numbers")]
    FloatingPoint,
    #[error("serialization failed: {0}")]
    Serialization(String),
    #[error("invalid sha256 identifier `{0}`")]
    InvalidIdentifier(String),
    #[error("module `{0}` is unknown")]
    UnknownModule(String),
    #[error("module `{0}` is not active")]
    InactiveModule(String),
    #[error("dependency cycle at module `{0}`")]
    Cycle(String),
    #[error("dependency environment mismatch for module `{0}`")]
    EnvironmentMismatch(String),
    #[error("dependency closure exceeds a registry limit")]
    ClosureLimit,
}

/// RFC-8785-style canonical JSON for the proof-registry schema.
///
/// Object keys are sorted recursively, arrays keep their semantic order, and
/// floating-point values are rejected. `serde_json` supplies the required JSON
/// string escaping and integer formatting.
pub fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>, RegistryError> {
    let value = serde_json::to_value(value)
        .map_err(|error| RegistryError::Serialization(error.to_string()))?;
    let canonical = canonical_value(value)?;
    serde_json::to_vec(&canonical).map_err(|error| RegistryError::Serialization(error.to_string()))
}

fn canonical_value(value: Value) -> Result<Value, RegistryError> {
    match value {
        Value::Object(map) => {
            let mut sorted = BTreeMap::new();
            for (key, value) in map {
                sorted.insert(key, canonical_value(value)?);
            }
            let mut canonical = serde_json::Map::new();
            for (key, value) in sorted {
                canonical.insert(key, value);
            }
            Ok(Value::Object(canonical))
        }
        Value::Array(values) => values
            .into_iter()
            .map(canonical_value)
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::Number(number) if number.is_f64() => Err(RegistryError::FloatingPoint),
        other => Ok(other),
    }
}

pub fn sha256_id<T: Serialize>(value: &T) -> Result<String, RegistryError> {
    let digest = Sha256::digest(canonical_json(value)?);
    Ok(format!("{MODULE_ID_PREFIX}{}", hex_lower(&digest)))
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex_lower(&Sha256::digest(bytes))
}

pub fn environment_id(lock: &EnvironmentLock) -> Result<String, RegistryError> {
    sha256_id(lock)
}

pub fn bundle_id(bundle: &DependencyBundle) -> Result<String, RegistryError> {
    sha256_id(bundle)
}

pub fn challenge_commitment(manifest: &ChallengeManifest) -> Result<[u8; 32], RegistryError> {
    Ok(Sha256::digest(canonical_json(manifest)?).into())
}

pub fn module_id(manifest: &ProofModuleManifest) -> Result<String, RegistryError> {
    #[derive(Serialize)]
    struct Identity<'a> {
        schema: &'a str,
        theorem_name: &'a str,
        theorem_signature: &'a str,
        environment_id: &'a str,
        source_sha256: &'a str,
        source_bytes: u64,
        direct_dependencies: &'a [String],
        originating_task_id: &'a str,
        originating_network: &'a str,
        author_wallet: &'a str,
        license: &'a str,
        verification: &'a ModuleVerification,
    }
    // Catalog fields are deliberately excluded. The generated import and
    // source URL are derived after the ID exists; status/replacement/title/
    // summary may change without mutating the immutable proof identity.
    sha256_id(&Identity {
        schema: &manifest.schema,
        theorem_name: &manifest.theorem_name,
        theorem_signature: &manifest.theorem_signature,
        environment_id: &manifest.environment_id,
        source_sha256: &manifest.source_sha256,
        source_bytes: manifest.source_bytes,
        direct_dependencies: &manifest.direct_dependencies,
        originating_task_id: &manifest.originating_task_id,
        originating_network: &manifest.originating_network,
        author_wallet: &manifest.author_wallet,
        license: &manifest.license,
        verification: &manifest.verification,
    })
}

pub fn generated_module_name(module_id: &str) -> Result<String, RegistryError> {
    let hex = validate_id(module_id)?;
    Ok(format!("{GENERATED_MODULE_PREFIX}{hex}"))
}

/// Build the deterministic, namespaced source promoted from a verified task.
/// The namespace uses the statement+proof payload hash, avoiding a circular
/// dependency between published bytes and the manifest-derived module ID.
pub fn compose_generated_module(
    candidate: &PromotionCandidate,
) -> Result<GeneratedModule, RegistryError> {
    if candidate.challenge.policy_version != 4
        || candidate.challenge.reuse_license != APACHE_2_0
        || !candidate.challenge.reusable
        || !candidate
            .proof_source
            .lines()
            .take(5)
            .any(|line| line.contains("SPDX-License-Identifier: Apache-2.0"))
    {
        return Err(RegistryError::Serialization(
            "promotion candidate violates the reusable policy".into(),
        ));
    }
    let forbidden = [
        "axiom ",
        "sorry",
        "unsafe ",
        "extern ",
        "macro ",
        "syntax ",
        "elab ",
        "command_elab",
        "#eval",
        "#check",
        "namespace ",
        "section ",
    ];
    let combined = format!("{}\n{}", candidate.statement, candidate.proof_source);
    if forbidden.iter().any(|token| combined.contains(token))
        || combined
            .lines()
            .any(|line| line.trim_start().starts_with("end "))
    {
        return Err(RegistryError::Serialization(
            "promotion source contains a forbidden declaration or command".into(),
        ));
    }
    let payload_hash = sha256_hex(combined.as_bytes());
    let namespace = format!("SwarmProofs.Generated.ArtifactH{payload_hash}");
    let mut imports = candidate.exact_imports.clone();
    imports.sort();
    imports.dedup();
    for import in &imports {
        if !import.starts_with(GENERATED_MODULE_PREFIX) {
            return Err(RegistryError::Serialization(format!(
                "noncanonical dependency import `{import}`"
            )));
        }
    }
    let strip_imports = |source: &str| {
        source
            .lines()
            .filter(|line| !line.trim_start().starts_with("import "))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let mut source = String::from(
        "/-\nCopyright 2026 Swarm Tips contributors\nSPDX-License-Identifier: Apache-2.0\n-/\n\nimport Mathlib\nimport Zeta23\n",
    );
    for import in imports {
        source.push_str(&format!("import {import}\n"));
    }
    source.push_str(&format!(
        "\nnamespace {namespace}\n\n{}\n\n{}\n\nend {namespace}\n",
        strip_imports(&candidate.statement),
        strip_imports(&candidate.proof_source)
    ));
    Ok(GeneratedModule {
        source,
        theorem_name: format!("{namespace}.proof"),
        theorem_signature: format!("theorem {namespace}.proof : {namespace}.statementProp"),
    })
}

fn validate_id(value: &str) -> Result<&str, RegistryError> {
    let Some(hex) = value.strip_prefix(MODULE_ID_PREFIX) else {
        return Err(RegistryError::InvalidIdentifier(value.to_string()));
    };
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(RegistryError::InvalidIdentifier(value.to_string()));
    }
    Ok(hex)
}

/// Resolve a frozen active dependency closure in deterministic topological
/// order. Direct dependency order is irrelevant; module ids are sorted before
/// traversal and every manifest's dependency ids are likewise sorted.
pub fn resolve_bundle(
    registry: &RegistryManifest,
    environment: &str,
    direct: &[String],
) -> Result<DependencyBundle, RegistryError> {
    resolve_bundle_with_status(registry, environment, direct, true)
}

/// Resolve a frozen historical closure without applying current discovery
/// status. This is for inspection and replay only; new campaign creation must
/// always use [`resolve_bundle`].
pub fn resolve_historical_bundle(
    registry: &RegistryManifest,
    environment: &str,
    direct: &[String],
) -> Result<DependencyBundle, RegistryError> {
    resolve_bundle_with_status(registry, environment, direct, false)
}

fn resolve_bundle_with_status(
    registry: &RegistryManifest,
    environment: &str,
    direct: &[String],
    require_active: bool,
) -> Result<DependencyBundle, RegistryError> {
    if direct.len() > MAX_DIRECT_DEPENDENCIES {
        return Err(RegistryError::ClosureLimit);
    }
    let modules: BTreeMap<&str, &ProofModuleManifest> = registry
        .modules
        .iter()
        .map(|module| (module.module_id.as_str(), module))
        .collect();
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    let mut ordered = Vec::new();
    let mut roots = direct.to_vec();
    roots.sort();
    roots.dedup();
    for module_id in roots {
        visit(
            &module_id,
            environment,
            &modules,
            &mut visiting,
            &mut visited,
            &mut ordered,
            0,
            require_active,
        )?;
    }
    let source_bytes = ordered.iter().try_fold(0u64, |total, id| {
        let module = modules
            .get(id.as_str())
            .ok_or_else(|| RegistryError::UnknownModule(id.clone()))?;
        total
            .checked_add(module.source_bytes)
            .ok_or(RegistryError::ClosureLimit)
    })?;
    if ordered.len() > MAX_TRANSITIVE_DEPENDENCIES || source_bytes > MAX_CLOSURE_SOURCE_BYTES {
        return Err(RegistryError::ClosureLimit);
    }
    Ok(DependencyBundle {
        schema: SCHEMA_VERSION.to_string(),
        environment_id: environment.to_string(),
        modules: ordered,
    })
}

#[allow(clippy::too_many_arguments)]
fn visit(
    id: &str,
    environment: &str,
    modules: &BTreeMap<&str, &ProofModuleManifest>,
    visiting: &mut BTreeSet<String>,
    visited: &mut BTreeSet<String>,
    ordered: &mut Vec<String>,
    depth: usize,
    require_active: bool,
) -> Result<(), RegistryError> {
    validate_id(id)?;
    if depth > MAX_DEPENDENCY_DEPTH {
        return Err(RegistryError::ClosureLimit);
    }
    if visited.contains(id) {
        return Ok(());
    }
    if !visiting.insert(id.to_string()) {
        return Err(RegistryError::Cycle(id.to_string()));
    }
    let module = modules
        .get(id)
        .ok_or_else(|| RegistryError::UnknownModule(id.to_string()))?;
    if require_active && module.status != ModuleStatus::Active {
        return Err(RegistryError::InactiveModule(id.to_string()));
    }
    if module.environment_id != environment {
        return Err(RegistryError::EnvironmentMismatch(id.to_string()));
    }
    let mut dependencies = module.direct_dependencies.clone();
    dependencies.sort();
    dependencies.dedup();
    for dependency in dependencies {
        visit(
            &dependency,
            environment,
            modules,
            visiting,
            visited,
            ordered,
            depth.saturating_add(1),
            require_active,
        )?;
    }
    visiting.remove(id);
    visited.insert(id.to_string());
    ordered.push(id.to_string());
    Ok(())
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verification() -> ModuleVerification {
        ModuleVerification {
            lean_kernel: true,
            axiom_audit: true,
            independent_kernel: true,
            axioms: vec!["Classical.choice".to_string()],
        }
    }

    fn module(id: &str, deps: &[&str], status: ModuleStatus) -> ProofModuleManifest {
        ProofModuleManifest {
            schema: SCHEMA_VERSION.to_string(),
            module_id: id.to_string(),
            module_name: generated_module_name(id).unwrap(),
            theorem_name: "result".to_string(),
            theorem_signature: "True".to_string(),
            environment_id:
                "sha256:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
                    .to_string(),
            source_sha256: "a".repeat(64),
            source_url: "https://storage.googleapis.com/example/module.lean".to_string(),
            source_bytes: 10,
            direct_dependencies: deps.iter().map(|value| (*value).to_string()).collect(),
            originating_task_id: "task-1".to_string(),
            originating_network: "devnet".to_string(),
            author_wallet: "wallet".to_string(),
            license: APACHE_2_0.to_string(),
            verification: verification(),
            status,
            replacement: None,
            title: "title".to_string(),
            summary: "summary".to_string(),
        }
    }

    fn id(ch: char) -> String {
        format!("sha256:{}", ch.to_string().repeat(64))
    }

    #[test]
    fn canonical_json_sorts_nested_keys() {
        let value = serde_json::json!({"z": 1, "a": {"y": 2, "b": 3}});
        assert_eq!(
            String::from_utf8(canonical_json(&value).unwrap()).unwrap(),
            r#"{"a":{"b":3,"y":2},"z":1}"#
        );
    }

    #[test]
    fn challenge_commitment_is_stable() {
        let challenge = ChallengeManifest {
            schema: CHALLENGE_SCHEMA_VERSION.to_string(),
            statement_sha256: "11".repeat(32),
            policy_version: 4,
            environment_id: id('e'),
            dependency_bundle_id: id('b'),
            reuse_license: APACHE_2_0.to_string(),
            reusable: true,
        };
        assert_eq!(
            hex_lower(&challenge_commitment(&challenge).unwrap()),
            "05aebd594d3493e42b7c144db9988636ffa96fdfb9a99ee2d7d9ad74640063dd"
        );
    }

    #[test]
    fn diamond_closure_is_topological_and_deduplicated() {
        let a = id('a');
        let b = id('b');
        let c = id('c');
        let d = id('d');
        let registry = RegistryManifest {
            schema: SCHEMA_VERSION.to_string(),
            environments: Vec::new(),
            modules: vec![
                module(&a, &[], ModuleStatus::Active),
                module(&b, &[&a], ModuleStatus::Active),
                module(&c, &[&a], ModuleStatus::Active),
                module(&d, &[&b, &c], ModuleStatus::Active),
            ],
            aliases: BTreeMap::new(),
        };
        let bundle = resolve_bundle(&registry, &id('e'), std::slice::from_ref(&d)).unwrap();
        assert_eq!(bundle.modules, vec![a, b, c, d]);
    }

    #[test]
    fn inactive_and_cyclic_modules_fail_closed() {
        let a = id('a');
        let b = id('b');
        let registry = RegistryManifest {
            schema: SCHEMA_VERSION.to_string(),
            environments: Vec::new(),
            modules: vec![
                module(&a, &[&b], ModuleStatus::Active),
                module(&b, &[&a], ModuleStatus::Active),
            ],
            aliases: BTreeMap::new(),
        };
        assert!(matches!(
            resolve_bundle(&registry, &id('e'), std::slice::from_ref(&a)),
            Err(RegistryError::Cycle(_))
        ));

        let registry = RegistryManifest {
            schema: SCHEMA_VERSION.to_string(),
            environments: Vec::new(),
            modules: vec![module(&a, &[], ModuleStatus::Deprecated)],
            aliases: BTreeMap::new(),
        };
        assert!(matches!(
            resolve_bundle(&registry, &id('e'), &[a]),
            Err(RegistryError::InactiveModule(_))
        ));
    }

    #[test]
    fn historical_replay_keeps_yanked_modules_but_new_work_rejects_them() {
        let a = id('a');
        let registry = RegistryManifest {
            schema: SCHEMA_VERSION.to_string(),
            environments: Vec::new(),
            modules: vec![module(&a, &[], ModuleStatus::Revoked)],
            aliases: BTreeMap::new(),
        };
        assert!(matches!(
            resolve_bundle(&registry, &id('e'), std::slice::from_ref(&a)),
            Err(RegistryError::InactiveModule(_))
        ));
        assert_eq!(
            resolve_historical_bundle(&registry, &id('e'), std::slice::from_ref(&a))
                .unwrap()
                .modules,
            vec![a]
        );
    }

    #[test]
    fn environment_mismatch_fails_closed() {
        let a = id('a');
        let registry = RegistryManifest {
            schema: SCHEMA_VERSION.to_string(),
            environments: Vec::new(),
            modules: vec![module(&a, &[], ModuleStatus::Active)],
            aliases: BTreeMap::new(),
        };
        assert!(matches!(
            resolve_bundle(&registry, &id('f'), &[a]),
            Err(RegistryError::EnvironmentMismatch(_))
        ));
    }

    #[test]
    fn module_identity_ignores_mutable_and_derived_catalog_fields() {
        let placeholder = id('a');
        let mut first = module(&placeholder, &[], ModuleStatus::Active);
        let identity = module_id(&first).unwrap();
        assert_eq!(
            identity,
            "sha256:1cdf626e34d63032b103c053e5c0fdfbd2441ff0520201d66d26ade4f8da81f5"
        );
        first.module_id = identity.clone();
        first.module_name = generated_module_name(&identity).unwrap();
        first.source_url = "https://example.invalid/other.lean".into();
        first.status = ModuleStatus::Deprecated;
        first.replacement = Some(id('b'));
        first.title = "new title".into();
        first.summary = "new summary".into();
        assert_eq!(module_id(&first).unwrap(), identity);
    }

    #[test]
    fn generated_module_is_namespaced_and_requires_spdx() {
        let proof =
            "-- SPDX-License-Identifier: Apache-2.0\ntheorem proof : statementProp := trivial";
        let candidate = PromotionCandidate {
            schema: "swarm.lean-promotion-candidate/v1".into(),
            task_id: "task".into(),
            campaign_id: "campaign".into(),
            network: "devnet".into(),
            author_wallet: "wallet".into(),
            artifact_sha256: sha256_hex(proof.as_bytes()),
            statement: "def statementProp : Prop := True".into(),
            proof_source: proof.into(),
            challenge: ChallengeManifest {
                schema: CHALLENGE_SCHEMA_VERSION.into(),
                statement_sha256: sha256_hex(b"def statementProp : Prop := True"),
                policy_version: 4,
                environment_id: id('e'),
                dependency_bundle_id: id('b'),
                reuse_license: APACHE_2_0.into(),
                reusable: true,
            },
            direct_dependencies: Vec::new(),
            dependency_closure: Vec::new(),
            exact_imports: Vec::new(),
            verification_axioms: vec!["propext".into()],
            status: "verified_pending_finalization".into(),
        };
        let generated = compose_generated_module(&candidate).unwrap();
        assert!(generated
            .source
            .contains("namespace SwarmProofs.Generated.ArtifactH"));
        assert!(generated.source.contains("import Zeta23"));
        assert!(generated
            .theorem_signature
            .starts_with("theorem SwarmProofs."));
    }
}
