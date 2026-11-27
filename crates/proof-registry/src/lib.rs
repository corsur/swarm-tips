//! Canonical schemas and hashes for composable Lean proof environments.
//!
//! The wire representation deliberately uses only strings, integers, booleans,
//! arrays, and objects. [`canonical_json`] therefore implements the RFC 8785
//! rules relevant to this schema without accepting floating-point values.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: &str = "swarm.lean-proof-registry/v1";
pub const CHALLENGE_SCHEMA_VERSION: &str = "swarm.lean-challenge/v1";
pub const MODULE_ID_PREFIX: &str = "sha256:";
pub const APACHE_2_0: &str = "Apache-2.0";
pub const GENERATED_MODULE_PREFIX: &str = "SwarmProofs.Generated.H";
pub const ALLOWED_SWARM_LIBRARY_IMPORTS: &[&str] = &[
    "SwarmProofs.Chains.BSD.Defs.V1",
    "SwarmProofs.Chains.BoolFourier.Defs.V1",
    "SwarmProofs.Chains.Circuits.Defs.V1",
    "SwarmProofs.Chains.NavierStokes.Defs.V1",
    "SwarmProofs.Chains.ProtocolMath.Defs.V1",
    "SwarmProofs.Chains.QuantumInfo.Defs.V1",
    "SwarmProofs.Chains.RHAnalytic.Defs.V1",
    "SwarmProofs.Chains.RHLagarias.Defs.V1",
];
pub const ALLOWED_AXIOMS: &[&str] = &["propext", "Classical.choice", "Quot.sound"];
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
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub frozen_library_modules: BTreeMap<String, String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RegistryError {
    #[error("canonical proof-registry JSON does not permit floating-point numbers")]
    FloatingPoint,
    #[error("canonical proof-registry JSON permits safe integers only")]
    UnsafeInteger,
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
    #[error("invalid proof registry: {0}")]
    InvalidRegistry(String),
}

/// Validate every identity and reference before a registry is trusted by an
/// API, attester, runner, publisher, or discovery surface.
pub fn validate_registry(registry: &RegistryManifest) -> Result<(), RegistryError> {
    if registry.schema != SCHEMA_VERSION {
        return Err(RegistryError::InvalidRegistry(
            "unexpected registry schema".into(),
        ));
    }
    let mut environments = BTreeSet::new();
    for environment in &registry.environments {
        for (mod_name, mod_hash) in &environment.frozen_library_modules {
            validate_id(mod_hash)?;
            if !is_canonical_frozen_module_name(mod_name) {
                return Err(RegistryError::InvalidRegistry(format!(
                    "noncanonical frozen library module name `{mod_name}`"
                )));
            }
        }
        if environment.lock.schema != "swarm.lean-environment/v1"
            || environment.lock.registry_schema != SCHEMA_VERSION
            || environment.lock.policy_version != 4
            || environment_id(&environment.lock)? != environment.environment_id
            || !environments.insert(environment.environment_id.as_str())
        {
            return Err(RegistryError::InvalidRegistry(format!(
                "invalid or duplicate environment {}",
                environment.environment_id
            )));
        }
    }

    let mut modules = BTreeMap::new();
    for module in &registry.modules {
        validate_id(&module.module_id)?;
        let mut canonical_dependencies = module.direct_dependencies.clone();
        canonical_dependencies.sort();
        canonical_dependencies.dedup();
        if module.schema != SCHEMA_VERSION
            || module_id(module)? != module.module_id
            || generated_module_name(&module.module_id)? != module.module_name
            || module.license != APACHE_2_0
            || !module.verification.lean_kernel
            || !module.verification.axiom_audit
            || !module.verification.independent_kernel
            || module.verification.axioms.iter().any(|axiom| {
                !["propext", "Classical.choice", "Quot.sound"].contains(&axiom.as_str())
            })
            || module.direct_dependencies.len() > MAX_DIRECT_DEPENDENCIES
            || module.direct_dependencies != canonical_dependencies
            || module.source_bytes == 0
            || module.source_sha256.len() != 64
            || !module
                .source_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || !environments.contains(module.environment_id.as_str())
            || modules.insert(module.module_id.as_str(), module).is_some()
        {
            return Err(RegistryError::InvalidRegistry(format!(
                "invalid or duplicate module {}",
                module.module_id
            )));
        }
    }

    for module in &registry.modules {
        resolve_historical_bundle(
            registry,
            &module.environment_id,
            std::slice::from_ref(&module.module_id),
        )?;
        for dependency in &module.direct_dependencies {
            let target = modules.get(dependency.as_str()).ok_or_else(|| {
                RegistryError::InvalidRegistry(format!("unknown dependency {dependency}"))
            })?;
            if target.environment_id != module.environment_id {
                return Err(RegistryError::EnvironmentMismatch(dependency.clone()));
            }
        }
        if let Some(replacement) = module.replacement.as_ref() {
            validate_id(replacement)?;
            if !modules.contains_key(replacement.as_str()) || replacement == &module.module_id {
                return Err(RegistryError::InvalidRegistry(format!(
                    "invalid replacement {replacement}"
                )));
            }
        }
    }
    for (alias, module_id) in &registry.aliases {
        if alias.trim().is_empty() || !modules.contains_key(module_id.as_str()) {
            return Err(RegistryError::InvalidRegistry(format!(
                "invalid alias {alias}"
            )));
        }
    }
    Ok(())
}

fn is_canonical_frozen_module_name(name: &str) -> bool {
    let parts: Vec<&str> = name.split('.').collect();
    if parts.len() != 5 {
        return false;
    }
    if parts[0] != "SwarmProofs" || parts[1] != "Chains" || parts[3] != "Defs" {
        return false;
    }
    let chain = parts[2];
    let ver = parts[4];
    !chain.is_empty()
        && chain
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        && ver
            .strip_prefix('V')
            .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
}

pub const FROZEN_V1_DEFS_GOLDEN_HASHES: &[(&str, &str, &str)] = &[
    (
        "SwarmProofs.Chains.BSD.Defs.V1",
        "SwarmProofs/Chains/BSD/Defs/V1.lean",
        "sha256:c2c3edf31528a5287c992f26b010140c99dd10536ec1cd7527041545222e704d",
    ),
    (
        "SwarmProofs.Chains.BoolFourier.Defs.V1",
        "SwarmProofs/Chains/BoolFourier/Defs/V1.lean",
        "sha256:c2dc1cb8e6f53729d1a47bb6bac2156d1c411c0b2ddb19526347e685bf467c09",
    ),
    (
        "SwarmProofs.Chains.Circuits.Defs.V1",
        "SwarmProofs/Chains/Circuits/Defs/V1.lean",
        "sha256:00cd43b77425913085d1a1d8ab6d2664c5857ee7495a5c1b5b89e798ceef667d",
    ),
    (
        "SwarmProofs.Chains.NavierStokes.Defs.V1",
        "SwarmProofs/Chains/NavierStokes/Defs/V1.lean",
        "sha256:74ee21968d313a9d67b8900f478a0da591c0ad38565eab01915637e8d17489ed",
    ),
    (
        "SwarmProofs.Chains.ProtocolMath.Defs.V1",
        "SwarmProofs/Chains/ProtocolMath/Defs/V1.lean",
        "sha256:b262371196d872e244cd60b44dabfd3654f10a247ad75226e85bc9d55bb40e5c",
    ),
    (
        "SwarmProofs.Chains.QuantumInfo.Defs.V1",
        "SwarmProofs/Chains/QuantumInfo/Defs/V1.lean",
        "sha256:9b0d768bdd78bdc3e84efa900022b1c94e6e0a8aef3f84b57e7d19dc471ee8b4",
    ),
    (
        "SwarmProofs.Chains.RHAnalytic.Defs.V1",
        "SwarmProofs/Chains/RHAnalytic/Defs/V1.lean",
        "sha256:5777771ee0a643240cb1f6d84994658c0008dd04ea32ae665c560a48329dfbe6",
    ),
    (
        "SwarmProofs.Chains.RHLagarias.Defs.V1",
        "SwarmProofs/Chains/RHLagarias/Defs/V1.lean",
        "sha256:c590bd97d7b40f604c9a258c6f2c9197ab64d24036cea0ecee34b91efa873300",
    ),
];

/// Enforce append-only immutability of `frozen_library_modules` across registry revisions:
/// every frozen environment and `SwarmProofs.Chains.*.Defs.V<N>` entry present in `base`
/// must remain present in `head` with the exact same SHA-256 digest (new `V<N+1>` modules may be added).
pub fn validate_frozen_defs_append_only(
    base: &RegistryManifest,
    head: &RegistryManifest,
) -> Result<(), RegistryError> {
    validate_registry(base)?;
    validate_registry(head)?;
    for base_env in &base.environments {
        let Some(head_env) = head
            .environments
            .iter()
            .find(|env| env.environment_id == base_env.environment_id)
        else {
            return Err(RegistryError::InvalidRegistry(format!(
                "frozen environment {} was deleted from registry",
                base_env.environment_id
            )));
        };
        for (mod_name, base_hash) in &base_env.frozen_library_modules {
            match head_env.frozen_library_modules.get(mod_name) {
                None => {
                    return Err(RegistryError::InvalidRegistry(format!(
                        "frozen library module `{mod_name}` was deleted from environment {}",
                        base_env.environment_id
                    )));
                }
                Some(head_hash) if head_hash != base_hash => {
                    return Err(RegistryError::InvalidRegistry(format!(
                        "frozen library module `{mod_name}` hash mutated from {base_hash} to {head_hash}; add a new V<N+1> module instead"
                    )));
                }
                Some(_) => {}
            }
        }
    }
    Ok(())
}

/// Validate that every frozen shared library module under `<package_root>/SwarmProofs`
/// is listed in `frozen_library_modules` and matches its SHA-256 digest byte-for-byte.
pub fn validate_frozen_library_modules(
    package_root: &Path,
    registry: &RegistryManifest,
) -> Result<(), RegistryError> {
    let swarm_dir = package_root.join("SwarmProofs");
    if !swarm_dir.is_dir() {
        return Err(RegistryError::InvalidRegistry(format!(
            "missing SwarmProofs directory under {}",
            package_root.display()
        )));
    }
    let mut disk_modules: BTreeMap<String, String> = BTreeMap::new();
    let mut stack = vec![swarm_dir];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|e| {
            RegistryError::InvalidRegistry(format!("failed reading {}: {e}", dir.display()))
        })?;
        for entry in entries {
            let entry = entry.map_err(|e| {
                RegistryError::InvalidRegistry(format!("directory entry error: {e}"))
            })?;
            let path = entry.path();
            let ft = entry
                .file_type()
                .map_err(|e| RegistryError::InvalidRegistry(format!("file_type error: {e}")))?;
            if ft.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|s| s.to_str()) != Some("lean") {
                continue;
            }
            let rel = path
                .strip_prefix(package_root)
                .map_err(|e| RegistryError::InvalidRegistry(e.to_string()))?;
            let rel_slash = rel.to_string_lossy().replace('\\', "/");
            if rel_slash == "SwarmProofs/Registry.lean"
                || rel_slash.starts_with("SwarmProofs/Generated/")
            {
                continue;
            }
            let Some(stem) = rel_slash.strip_suffix(".lean") else {
                continue;
            };
            let mod_name = stem.replace('/', ".");
            if !is_canonical_frozen_module_name(&mod_name) {
                return Err(RegistryError::InvalidRegistry(format!(
                    "unversioned or noncanonical library file on disk: {rel_slash}"
                )));
            }
            let bytes = std::fs::read(&path).map_err(|e| {
                RegistryError::InvalidRegistry(format!("failed reading {}: {e}", path.display()))
            })?;
            let sha = format!("{MODULE_ID_PREFIX}{}", sha256_hex(&bytes));
            disk_modules.insert(mod_name, sha);
        }
    }
    if disk_modules.is_empty() {
        return Err(RegistryError::InvalidRegistry(
            "no frozen Defs/V*.lean modules found under SwarmProofs/Chains".into(),
        ));
    }

    let lock_path = package_root.join("environment-lock.json");
    if lock_path.is_file() {
        let lock_bytes = std::fs::read(&lock_path).map_err(|e| {
            RegistryError::InvalidRegistry(format!("failed reading {}: {e}", lock_path.display()))
        })?;
        let disk_lock: EnvironmentLock = serde_json::from_slice(&lock_bytes).map_err(|e| {
            RegistryError::InvalidRegistry(format!("invalid environment-lock.json: {e}"))
        })?;
        let expected_env_id = environment_id(&disk_lock)?;
        let Some(matched_env) = registry
            .environments
            .iter()
            .find(|env| env.environment_id == expected_env_id)
        else {
            return Err(RegistryError::InvalidRegistry(format!(
                "registry.json missing environment {expected_env_id} from environment-lock.json"
            )));
        };
        if matched_env.lock != disk_lock {
            return Err(RegistryError::InvalidRegistry(
                "registry.json environment lock differs from environment-lock.json".into(),
            ));
        }
        if matched_env.frozen_library_modules != disk_modules {
            return Err(RegistryError::InvalidRegistry(
                "registry.json EnvironmentRecord frozen_library_modules mismatch with Defs/V*.lean on disk"
                    .into(),
            ));
        }
    } else {
        let Some(active_env) = registry
            .environments
            .iter()
            .rfind(|env| env.lock.policy_version == 4)
        else {
            return Err(RegistryError::InvalidRegistry(
                "registry.json contains no policy_version=4 environment".into(),
            ));
        };
        if active_env.frozen_library_modules != disk_modules {
            return Err(RegistryError::InvalidRegistry(
                "registry.json EnvironmentRecord frozen_library_modules mismatch with Defs/V*.lean on disk"
                    .into(),
            ));
        }
    }

    Ok(())
}

/// RFC-8785-style canonical JSON for the proof-registry schema.
///
/// Object keys are sorted recursively, arrays keep their semantic order, and
/// floating-point values are rejected. `serde_json` supplies the required JSON
/// string escaping and integer formatting.
pub fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>, RegistryError> {
    let value = serde_json::to_value(value)
        .map_err(|error| RegistryError::Serialization(error.to_string()))?;
    let mut output = String::new();
    write_canonical(&value, &mut output)?;
    Ok(output.into_bytes())
}

fn write_canonical(value: &Value, output: &mut String) -> Result<(), RegistryError> {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<_> = map.iter().collect();
            entries.sort_by(|(a, _), (b, _)| a.encode_utf16().cmp(b.encode_utf16()));
            output.push('{');
            for (index, (key, child)) in entries.into_iter().enumerate() {
                if index != 0 {
                    output.push(',');
                }
                output.push_str(
                    &serde_json::to_string(key)
                        .map_err(|e| RegistryError::Serialization(e.to_string()))?,
                );
                output.push(':');
                write_canonical(child, output)?;
            }
            output.push('}');
        }
        Value::Array(values) => {
            output.push('[');
            for (index, child) in values.iter().enumerate() {
                if index != 0 {
                    output.push(',');
                }
                write_canonical(child, output)?;
            }
            output.push(']');
        }
        Value::Number(number) => {
            if number.is_f64() {
                return Err(RegistryError::FloatingPoint);
            }
            const MAX_SAFE: u64 = 9_007_199_254_740_991;
            let safe = number
                .as_i64()
                .map(|n| n.unsigned_abs() <= MAX_SAFE)
                .or_else(|| number.as_u64().map(|n| n <= MAX_SAFE))
                .unwrap_or(false);
            if !safe {
                return Err(RegistryError::UnsafeInteger);
            }
            output.push_str(&number.to_string());
        }
        other => output.push_str(
            &serde_json::to_string(other)
                .map_err(|e| RegistryError::Serialization(e.to_string()))?,
        ),
    }
    Ok(())
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
    }
    // Catalog fields are deliberately excluded. The generated import and
    // source URL are derived after the ID exists; status/replacement/title/
    // summary and verification evidence may change without mutating the
    // immutable proof identity.
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
    })
}

pub fn generated_module_name(module_id: &str) -> Result<String, RegistryError> {
    let hex = validate_id(module_id)?;
    Ok(format!("{GENERATED_MODULE_PREFIX}{hex}"))
}

pub const FORBIDDEN_SHARED_WORD_TOKENS: &[&str] = &[
    "axiom",
    "sorry",
    "sorryAx",
    "admit",
    "unsafe",
    "partial",
    "opaque",
    "meta",
    "constant",
    "extern",
    "implemented_by",
    "csimp",
    "macro",
    "macro_rules",
    "syntax",
    "elab",
    "elab_rules",
    "command_elab",
    "term_elab",
    "notation",
    "infix",
    "infixl",
    "infixr",
    "prefix",
    "postfix",
    "attribute",
    "deriving",
    "variable",
    "run_tac",
    "run_cmd",
    "run_elab",
    "by_elab",
    "native_decide",
    "ofReduceBool",
    "trustCompiler",
    "instance",
    "class",
    "initialize",
    "builtin_initialize",
    "unif_hint",
    "declare_syntax_cat",
    "simproc",
    "namespace",
    "section",
    "end",
];

pub const FORBIDDEN_SHARED_SUBSTRINGS: &[&str] =
    &["@[", "#eval", "#check", "#print", "#reduce", "#synth"];

pub const FORBIDDEN_PROOF_WORD_TOKENS: &[&str] = &["local", "scoped", "open", "export"];

fn is_lean_ident_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '\'' | '?' | '»' | '₊' | '₋')
}

fn match_lean_char_literal(chars: &[(usize, char)], idx: usize) -> Option<usize> {
    if idx + 2 < chars.len() {
        let c1 = chars[idx + 1].1;
        if c1 != '\'' && c1 != '\\' && c1 != '\n' && c1 != '\r' && chars[idx + 2].1 == '\'' {
            return Some(3);
        }
    }
    if idx + 3 < chars.len() && chars[idx + 1].1 == '\\' {
        let esc = chars[idx + 2].1;
        if esc != '\n' && esc != '\r' && chars[idx + 3].1 == '\'' {
            return Some(4);
        }
        if esc == 'x'
            && idx + 5 < chars.len()
            && chars[idx + 3].1.is_ascii_hexdigit()
            && chars[idx + 4].1.is_ascii_hexdigit()
            && chars[idx + 5].1 == '\''
        {
            return Some(6);
        }
        if esc == 'u'
            && idx + 7 < chars.len()
            && (3..=6).all(|k| chars[idx + k].1.is_ascii_hexdigit())
            && chars[idx + 7].1 == '\''
        {
            return Some(8);
        }
    }
    None
}

/// Strip Lean `--` line comments and nested `/- ... -/` block comments while blanking
/// string and character literal interiors so `"--"` / `"/-"` / `'-'` / `'/'` never spoof comments.
/// Fails closed on unclosed `/-`, unclosed string/char literals, or interpolated strings `s!"..."`.
pub fn strip_lean_comments(source: &str) -> Result<String, RegistryError> {
    let mut out = String::with_capacity(source.len());
    let mut block_depth = 0usize;
    let mut prev_is_ident = false;
    let mut prev_was_ident_bang = false;
    let chars: Vec<(usize, char)> = source.char_indices().collect();
    let mut idx = 0usize;
    while idx < chars.len() {
        let (_, ch) = chars[idx];
        let next_ch = chars.get(idx + 1).map(|(_, c)| *c);
        if block_depth > 0 {
            if ch == '/' && next_ch == Some('-') {
                block_depth += 1;
                out.push_str("  ");
                idx += 2;
                continue;
            }
            if ch == '-' && next_ch == Some('/') {
                block_depth -= 1;
                out.push_str("  ");
                idx += 2;
                if block_depth == 0 {
                    prev_is_ident = false;
                    prev_was_ident_bang = false;
                }
                continue;
            }
            out.push(if ch == '\n' { '\n' } else { ' ' });
            idx += 1;
            continue;
        }
        if ch == '/' && next_ch == Some('-') {
            block_depth = 1;
            out.push_str("  ");
            prev_is_ident = false;
            prev_was_ident_bang = false;
            idx += 2;
            continue;
        }
        if ch == '-' && next_ch == Some('-') {
            while idx < chars.len() && chars[idx].1 != '\n' {
                out.push(' ');
                idx += 1;
            }
            prev_is_ident = false;
            prev_was_ident_bang = false;
            continue;
        }
        // Raw string literal `r#+"..."#+` when `r` is not part of an identifier:
        if ch == 'r' && !prev_is_ident {
            let mut hash_count = 0usize;
            while chars.get(idx + 1 + hash_count).map(|(_, c)| *c) == Some('#') {
                hash_count += 1;
            }
            if hash_count > 0 && chars.get(idx + 1 + hash_count).map(|(_, c)| *c) == Some('"') {
                out.push('r');
                for _ in 0..hash_count {
                    out.push('#');
                }
                out.push('"');
                idx += 2 + hash_count;
                let mut closed = false;
                while idx < chars.len() {
                    let c = chars[idx].1;
                    if c == '"'
                        && (1..=hash_count)
                            .all(|k| chars.get(idx + k).map(|(_, hc)| *hc) == Some('#'))
                    {
                        out.push('"');
                        for _ in 0..hash_count {
                            out.push('#');
                        }
                        idx += 1 + hash_count;
                        closed = true;
                        break;
                    }
                    out.push(if c == '\n' { '\n' } else { ' ' });
                    idx += 1;
                }
                if !closed {
                    return Err(RegistryError::Serialization(
                        "unclosed Lean raw string literal in promotion source".into(),
                    ));
                }
                prev_is_ident = false;
                prev_was_ident_bang = false;
                continue;
            }
        }
        if ch == '"' {
            if prev_was_ident_bang {
                return Err(RegistryError::Serialization(
                    "interpolated Lean string literal is not permitted in promotion source".into(),
                ));
            }
            out.push('"');
            idx += 1;
            let mut closed = false;
            while idx < chars.len() {
                let c = chars[idx].1;
                if c == '\\' {
                    out.push(' ');
                    idx += 1;
                    if idx >= chars.len() {
                        return Err(RegistryError::Serialization(
                            "unclosed Lean string literal in promotion source".into(),
                        ));
                    }
                    let esc = chars[idx].1;
                    out.push(if esc == '\n' { '\n' } else { ' ' });
                    idx += 1;
                    continue;
                }
                if c == '"' {
                    out.push('"');
                    idx += 1;
                    closed = true;
                    break;
                }
                out.push(if c == '\n' { '\n' } else { ' ' });
                idx += 1;
            }
            if !closed {
                return Err(RegistryError::Serialization(
                    "unclosed Lean string literal in promotion source".into(),
                ));
            }
            prev_is_ident = false;
            prev_was_ident_bang = false;
            continue;
        }
        if ch == '\'' && !prev_is_ident {
            let Some(lit_len) = match_lean_char_literal(&chars, idx) else {
                return Err(RegistryError::Serialization(
                    "unclosed or malformed Lean character literal in promotion source".into(),
                ));
            };
            out.push('\'');
            for _ in 0..lit_len.saturating_sub(2) {
                out.push(' ');
            }
            out.push('\'');
            idx += lit_len;
            prev_is_ident = false;
            prev_was_ident_bang = false;
            continue;
        }
        out.push(ch);
        prev_was_ident_bang = prev_is_ident && ch == '!';
        prev_is_ident = is_lean_ident_char(ch) || (prev_is_ident && ch == '!');
        idx += 1;
    }
    if block_depth != 0 {
        return Err(RegistryError::Serialization(
            "unclosed Lean block comment `/-` in promotion source".into(),
        ));
    }
    Ok(out)
}

fn has_lean_word_token(stripped_code: &str, forbidden: &[&str]) -> bool {
    stripped_code
        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '\'')
        .filter(|tok| !tok.is_empty())
        .any(|tok| forbidden.contains(&tok))
}

fn has_forbidden_promotion_tokens(stripped_combined: &str, stripped_proof: &str) -> bool {
    FORBIDDEN_SHARED_SUBSTRINGS
        .iter()
        .any(|sub| stripped_combined.contains(sub))
        || has_lean_word_token(stripped_combined, FORBIDDEN_SHARED_WORD_TOKENS)
        || has_lean_word_token(stripped_proof, FORBIDDEN_PROOF_WORD_TOKENS)
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
    let combined = format!("{}\n{}", candidate.statement, candidate.proof_source);
    let stripped_stmt_code = strip_lean_comments(&candidate.statement)?;
    let stripped_proof_code = strip_lean_comments(&candidate.proof_source)?;
    let stripped_combined_code = format!("{stripped_stmt_code}\n{stripped_proof_code}");
    if has_forbidden_promotion_tokens(&stripped_combined_code, &stripped_proof_code) {
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
    let mut swarm_library_imports = BTreeSet::new();
    let mut collect_and_strip_imports = |source: &str| -> Result<String, RegistryError> {
        let mut lines = Vec::new();
        for line in source.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("import ") || trimmed.starts_with("import\t") {
                let without_comment = trimmed.split("--").next().unwrap_or(trimmed);
                for mod_name in without_comment
                    .trim_start_matches("import")
                    .split_whitespace()
                {
                    if mod_name.starts_with(GENERATED_MODULE_PREFIX) {
                        if !imports.iter().any(|allowed| allowed == mod_name) {
                            return Err(RegistryError::Serialization(format!(
                                "undeclared generated dependency import `{mod_name}`"
                            )));
                        }
                    } else if mod_name.starts_with("SwarmProofs") {
                        if !ALLOWED_SWARM_LIBRARY_IMPORTS.contains(&mod_name) {
                            return Err(RegistryError::Serialization(format!(
                                "noncanonical SwarmProofs import `{mod_name}`"
                            )));
                        }
                        swarm_library_imports.insert(mod_name.to_string());
                    }
                }
                continue;
            }
            lines.push(line);
        }
        Ok(lines.join("\n"))
    };
    let stripped_statement = collect_and_strip_imports(&candidate.statement)?;
    let stripped_proof = collect_and_strip_imports(&candidate.proof_source)?;
    let mut source = String::from(
        "/-\nCopyright 2026 Swarm Tips contributors\nSPDX-License-Identifier: Apache-2.0\n-/\n\nimport Mathlib\nimport Zeta23\n",
    );
    for lib_import in &swarm_library_imports {
        source.push_str(&format!("import {lib_import}\n"));
    }
    for import in imports {
        source.push_str(&format!("import {import}\n"));
    }
    source.push_str(&format!(
        "\nnamespace {namespace}\n\n{stripped_statement}\n\n{stripped_proof}\n\nend {namespace}\n"
    ));
    let declares_disproof = stripped_proof_code.lines().any(|line| {
        let t = line.trim_start();
        t.starts_with("theorem disproof")
            || t.starts_with("lemma disproof")
            || t.starts_with("def disproof")
    });
    let is_negated_proof = declares_disproof
        || stripped_proof_code.lines().any(|line| {
            let t = line.trim_start();
            (t.starts_with("theorem proof")
                || t.starts_with("lemma proof")
                || t.starts_with("def proof"))
                && (t.contains("¬")
                    || t.contains("Not statementProp")
                    || t.contains("statementProp → False")
                    || t.contains("statementProp -> False"))
        });
    let entry_name = if declares_disproof {
        "disproof"
    } else {
        "proof"
    };
    let theorem_signature = if is_negated_proof {
        format!("theorem {namespace}.{entry_name} : ¬ {namespace}.statementProp")
    } else {
        format!("theorem {namespace}.{entry_name} : {namespace}.statementProp")
    };
    Ok(GeneratedModule {
        source,
        theorem_name: format!("{namespace}.{entry_name}"),
        theorem_signature,
    })
}

/// Automatically promote a verified promotion candidate into a target registry.
///
/// Validates the candidate's hashes, schemas, and axioms, resolves its dependency closure,
/// generates the namespaced Lean module file, inserts the manifest into the registry,
/// sorts the registry modules deterministically, and validates the entire registry.
///
/// Idempotent: if the module is already present with identical identity, the existing
/// record is returned without re-activating yanked/revoked modules.
pub fn preview_autopromote_manifest(
    candidate: &PromotionCandidate,
    registry: &RegistryManifest,
    public_base_url: &str,
) -> Result<(ProofModuleManifest, GeneratedModule), RegistryError> {
    if candidate.schema != "swarm.lean-promotion-candidate/v1"
        || candidate.status != "verified_pending_finalization"
        || candidate.task_id.is_empty()
        || candidate.campaign_id.is_empty()
        || candidate.network.is_empty()
        || candidate.author_wallet.is_empty()
        || sha256_hex(candidate.proof_source.as_bytes()) != candidate.artifact_sha256
        || sha256_hex(candidate.statement.as_bytes()) != candidate.challenge.statement_sha256
        || candidate.challenge.schema != CHALLENGE_SCHEMA_VERSION
        || candidate.challenge.policy_version != 4
        || !candidate.challenge.reusable
        || candidate.challenge.reuse_license != APACHE_2_0
        || candidate
            .verification_axioms
            .iter()
            .any(|axiom| !ALLOWED_AXIOMS.contains(&axiom.as_str()))
    {
        return Err(RegistryError::InvalidRegistry(
            "candidate hashes, schema, status, or unallowed axioms do not match".into(),
        ));
    }
    if !registry
        .environments
        .iter()
        .any(|environment| environment.environment_id == candidate.challenge.environment_id)
    {
        return Err(RegistryError::EnvironmentMismatch(
            candidate.challenge.environment_id.clone(),
        ));
    }
    let mut direct_dependencies = candidate.direct_dependencies.clone();
    direct_dependencies.sort();
    direct_dependencies.dedup();
    let bundle = resolve_bundle(
        registry,
        &candidate.challenge.environment_id,
        &direct_dependencies,
    )?;
    if bundle.modules != candidate.dependency_closure
        || bundle_id(&bundle)? != candidate.challenge.dependency_bundle_id
        || candidate.exact_imports
            != bundle
                .modules
                .iter()
                .map(|id| generated_module_name(id))
                .collect::<Result<Vec<_>, _>>()?
    {
        return Err(RegistryError::InvalidRegistry(
            "candidate dependency closure or challenge does not match".into(),
        ));
    }

    let generated = compose_generated_module(candidate)?;
    let source_sha256 = sha256_hex(generated.source.as_bytes());
    let mut manifest = ProofModuleManifest {
        schema: SCHEMA_VERSION.into(),
        module_id: String::new(),
        module_name: String::new(),
        theorem_name: generated.theorem_name.clone(),
        theorem_signature: generated.theorem_signature.clone(),
        environment_id: candidate.challenge.environment_id.clone(),
        source_sha256,
        source_url: String::new(),
        source_bytes: generated.source.len() as u64,
        direct_dependencies,
        originating_task_id: candidate.task_id.clone(),
        originating_network: candidate.network.clone(),
        author_wallet: candidate.author_wallet.clone(),
        license: APACHE_2_0.into(),
        verification: ModuleVerification {
            lean_kernel: true,
            axiom_audit: true,
            independent_kernel: true,
            axioms: candidate.verification_axioms.clone(),
        },
        status: ModuleStatus::Active,
        replacement: None,
        title: format!("Reusable proof from task {}", candidate.task_id),
        summary: "Kernel checked, not mathematical peer review.".into(),
    };
    manifest.module_id = module_id(&manifest)?;
    manifest.module_name = generated_module_name(&manifest.module_id)?;
    let id_hex = manifest
        .module_id
        .strip_prefix("sha256:")
        .ok_or_else(|| RegistryError::InvalidIdentifier(manifest.module_id.clone()))?;
    manifest.source_url = format!(
        "{}/modules/{id_hex}.lean",
        public_base_url.trim_end_matches('/')
    );

    Ok((manifest, generated))
}

pub fn autopromote_candidate(
    candidate: &PromotionCandidate,
    registry: &mut RegistryManifest,
    package_root: &Path,
    public_base_url: &str,
) -> Result<ProofModuleManifest, RegistryError> {
    let (manifest, generated) = preview_autopromote_manifest(candidate, registry, public_base_url)?;

    if let Some(existing) = registry
        .modules
        .iter()
        .find(|existing| existing.module_id == manifest.module_id)
    {
        if module_id(existing)? != manifest.module_id {
            return Err(RegistryError::InvalidRegistry(
                "module id collision with different manifest".into(),
            ));
        }
        let existing_path =
            package_root.join(format!("{}.lean", existing.module_name.replace('.', "/")));
        if let Ok(bytes) = std::fs::read(&existing_path) {
            if bytes != generated.source.as_bytes() {
                return Err(RegistryError::InvalidRegistry(
                    "existing immutable module bytes differ".into(),
                ));
            }
        }
        return Ok(existing.clone());
    }

    let relative = format!("{}.lean", manifest.module_name.replace('.', "/"));
    let output = package_root.join(PathBuf::from(relative));
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|e| RegistryError::Serialization(e.to_string()))?;
    }
    std::fs::write(&output, &generated.source)
        .map_err(|e| RegistryError::Serialization(e.to_string()))?;

    registry.modules.push(manifest.clone());
    registry
        .modules
        .sort_by(|left, right| left.module_id.cmp(&right.module_id));
    validate_registry(registry)?;
    Ok(manifest)
}

fn validate_id(value: &str) -> Result<&str, RegistryError> {
    let Some(hex) = value.strip_prefix(MODULE_ID_PREFIX) else {
        return Err(RegistryError::InvalidIdentifier(value.to_string()));
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
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

    fn environment_lock() -> EnvironmentLock {
        EnvironmentLock {
            schema: "swarm.lean-environment/v1".into(),
            name: "test".into(),
            toolchain: "leanprover/lean4:v4.33.0-rc2".into(),
            mathlib_revision: "1".repeat(40),
            policy_version: 4,
            registry_schema: SCHEMA_VERSION.into(),
            allowed_import_roots: vec!["Mathlib".into(), "SwarmProofs".into()],
            external_packages: Vec::new(),
        }
    }

    fn valid_registry() -> RegistryManifest {
        let lock = environment_lock();
        let environment = environment_id(&lock).unwrap();
        let mut item = module(&id('a'), &[], ModuleStatus::Active);
        item.environment_id = environment.clone();
        item.module_id = module_id(&item).unwrap();
        item.module_name = generated_module_name(&item.module_id).unwrap();
        RegistryManifest {
            schema: SCHEMA_VERSION.into(),
            environments: vec![EnvironmentRecord {
                environment_id: environment,
                lock,
                frozen_library_modules: BTreeMap::new(),
            }],
            modules: vec![item],
            aliases: BTreeMap::new(),
        }
    }

    #[test]
    fn shared_canonical_vectors_and_integer_limits() {
        let vectors: Value =
            serde_json::from_str(include_str!("../canonical-vectors.json")).unwrap();
        for vector in vectors.as_array().unwrap() {
            assert_eq!(
                String::from_utf8(canonical_json(&vector["input"]).unwrap()).unwrap(),
                vector["canonical"].as_str().unwrap()
            );
        }
        for number in [
            serde_json::json!(9_007_199_254_740_992_u64),
            serde_json::json!(-9_007_199_254_740_992_i64),
            serde_json::json!(u64::MAX),
        ] {
            assert_eq!(canonical_json(&number), Err(RegistryError::UnsafeInteger));
        }
        assert_eq!(
            canonical_json(&serde_json::json!(1.5)),
            Err(RegistryError::FloatingPoint)
        );
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
            "sha256:cd039f443a3237dcafa04d0ee2135f62590f7b8fc0ae65f6c729177df5473f80"
        );
        first.module_id = identity.clone();
        first.module_name = generated_module_name(&identity).unwrap();
        first.source_url = "https://example.invalid/other.lean".into();
        first.status = ModuleStatus::Deprecated;
        first.replacement = Some(id('b'));
        first.title = "new title".into();
        first.summary = "new summary".into();
        first.verification.axioms.push("propext".into());
        first.verification.independent_kernel = false;
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

    #[test]
    fn registry_validation_rejects_forged_identity_and_dangling_alias() {
        let registry = valid_registry();
        validate_registry(&registry).unwrap();

        let mut forged = registry.clone();
        forged.modules[0].source_sha256 = "b".repeat(64);
        assert!(matches!(
            validate_registry(&forged),
            Err(RegistryError::InvalidRegistry(_))
        ));

        let mut dangling = registry;
        dangling.aliases.insert("latest".into(), id('f'));
        assert!(matches!(
            validate_registry(&dangling),
            Err(RegistryError::InvalidRegistry(_))
        ));
    }

    #[test]
    fn autopromote_and_revocation_lifecycle_test() {
        let mut registry = valid_registry();
        let env_id = registry.environments[0].environment_id.clone();
        let statement = "def statementProp : Prop := True";
        let proof =
            "-- SPDX-License-Identifier: Apache-2.0\ntheorem proof : statementProp := trivial";
        let bundle = DependencyBundle {
            schema: SCHEMA_VERSION.into(),
            environment_id: env_id.clone(),
            modules: Vec::new(),
        };
        let candidate = PromotionCandidate {
            schema: "swarm.lean-promotion-candidate/v1".into(),
            task_id: "task-auto-1".into(),
            campaign_id: "camp-auto-1".into(),
            network: "devnet".into(),
            author_wallet: "wallet123".into(),
            artifact_sha256: sha256_hex(proof.as_bytes()),
            statement: statement.into(),
            proof_source: proof.into(),
            challenge: ChallengeManifest {
                schema: CHALLENGE_SCHEMA_VERSION.into(),
                statement_sha256: sha256_hex(statement.as_bytes()),
                policy_version: 4,
                environment_id: env_id.clone(),
                dependency_bundle_id: bundle_id(&bundle).unwrap(),
                reuse_license: APACHE_2_0.into(),
                reusable: true,
            },
            direct_dependencies: Vec::new(),
            dependency_closure: Vec::new(),
            exact_imports: Vec::new(),
            verification_axioms: vec!["propext".into()],
            status: "verified_pending_finalization".into(),
        };
        let tmp = std::env::temp_dir().join(format!("autopromote-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let manifest =
            autopromote_candidate(&candidate, &mut registry, &tmp, "https://proofs.swarm.tips")
                .unwrap();
        assert!(manifest.module_id.starts_with("sha256:"));
        assert_eq!(manifest.status, ModuleStatus::Active);

        // Next task can resolve bundle containing this module:
        let resolved = resolve_bundle(
            &registry,
            &env_id,
            std::slice::from_ref(&manifest.module_id),
        )
        .unwrap();
        assert_eq!(resolved.modules, vec![manifest.module_id.clone()]);

        // Revocation (two-way door):
        let target = registry
            .modules
            .iter_mut()
            .find(|m| m.module_id == manifest.module_id)
            .unwrap();
        target.status = ModuleStatus::Revoked;

        // Future resolution of the revoked module MUST fail closed:
        assert!(matches!(
            resolve_bundle(
                &registry,
                &env_id,
                std::slice::from_ref(&manifest.module_id)
            ),
            Err(RegistryError::InactiveModule(_))
        ));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn compose_generated_module_preserves_allowed_swarm_chain_imports_and_rejects_unknown() {
        let proof =
            "-- SPDX-License-Identifier: Apache-2.0\ntheorem proof : statementProp := trivial";
        let statement = "import SwarmProofs.Chains.BSD.Defs.V1\ndef statementProp : Prop := True";
        let make_cand = |stmt: &str| PromotionCandidate {
            schema: "swarm.lean-promotion-candidate/v1".into(),
            task_id: "task-chain".into(),
            campaign_id: "camp-chain".into(),
            network: "devnet".into(),
            author_wallet: "wallet".into(),
            artifact_sha256: sha256_hex(proof.as_bytes()),
            statement: stmt.into(),
            proof_source: proof.into(),
            challenge: ChallengeManifest {
                schema: CHALLENGE_SCHEMA_VERSION.into(),
                statement_sha256: sha256_hex(stmt.as_bytes()),
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
        let generated = compose_generated_module(&make_cand(statement)).unwrap();
        assert!(generated
            .source
            .contains("import Zeta23\nimport SwarmProofs.Chains.BSD.Defs.V1\n"));

        for disallowed in [
            "import SwarmProofs\ndef statementProp : Prop := True",
            "import SwarmProofs.Registry\ndef statementProp : Prop := True",
            "import SwarmProofs.Chains.BSD.Defs\ndef statementProp : Prop := True",
            "import SwarmProofs.Unknown.Module\ndef statementProp : Prop := True",
            "instance bad : Zero Nat := ⟨0⟩\ndef statementProp : Prop := True",
            "class Bad (α : Type) where x : α\ndef statementProp : Prop := True",
        ] {
            assert!(
                compose_generated_module(&make_cand(disallowed)).is_err(),
                "expected compose_generated_module to reject: {disallowed}"
            );
        }

        let undeclared_gen =
            "import SwarmProofs.Generated.Hdeadbeef\ndef statementProp : Prop := True";
        assert!(compose_generated_module(&make_cand(undeclared_gen)).is_err());
    }

    #[test]
    fn golden_swarm_proofs_frozen_library_modules_pass_and_fail_closed_on_1_byte_mutation() {
        let pkg_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .unwrap()
            .join("proofs")
            .join("SwarmProofs");
        let reg_bytes = std::fs::read(pkg_root.join("registry.json")).unwrap();
        let reg: RegistryManifest = serde_json::from_slice(&reg_bytes).unwrap();
        validate_registry(&reg).unwrap();
        validate_frozen_library_modules(&pkg_root, &reg).unwrap();

        // Copy to a temp package root and verify 1-byte mutation & unlisted V2 fail closed
        let tmp = std::env::temp_dir().join(format!(
            "frozen-defs-golden-{}-{}",
            std::process::id(),
            &sha256_hex(&reg_bytes)[..12]
        ));
        let _ = std::fs::remove_dir_all(&tmp);
        for rel in [
            "SwarmProofs/Registry.lean",
            "SwarmProofs/Chains/BSD/Defs/V1.lean",
            "SwarmProofs/Chains/BoolFourier/Defs/V1.lean",
            "SwarmProofs/Chains/Circuits/Defs/V1.lean",
            "SwarmProofs/Chains/NavierStokes/Defs/V1.lean",
            "SwarmProofs/Chains/ProtocolMath/Defs/V1.lean",
            "SwarmProofs/Chains/QuantumInfo/Defs/V1.lean",
            "SwarmProofs/Chains/RHAnalytic/Defs/V1.lean",
            "SwarmProofs/Chains/RHLagarias/Defs/V1.lean",
            "environment-lock.json",
            "registry.json",
        ] {
            let dst = tmp.join(rel);
            std::fs::create_dir_all(dst.parent().unwrap()).unwrap();
            std::fs::copy(pkg_root.join(rel), &dst).unwrap();
        }
        validate_frozen_library_modules(&tmp, &reg).unwrap();

        // Mutate 1 byte in BSD/Defs/V1.lean -> MUST fail closed!
        let bsd_path = tmp.join("SwarmProofs/Chains/BSD/Defs/V1.lean");
        let orig_bsd = std::fs::read(&bsd_path).unwrap();
        let mut mutated = orig_bsd.clone();
        mutated.push(b'\n');
        std::fs::write(&bsd_path, &mutated).unwrap();
        assert!(validate_frozen_library_modules(&tmp, &reg).is_err());

        // Restore BSD/Defs/V1.lean, then add an unlisted V2.lean -> MUST fail closed!
        std::fs::write(&bsd_path, &orig_bsd).unwrap();
        validate_frozen_library_modules(&tmp, &reg).unwrap();
        let unlisted_v2 = tmp.join("SwarmProofs/Chains/BSD/Defs/V2.lean");
        std::fs::write(&unlisted_v2, b"def sneaky : Nat := 0\n").unwrap();
        assert!(validate_frozen_library_modules(&tmp, &reg).is_err());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn registry_v4_environment_matches_pinned_attester_policy_v4() {
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .unwrap();
        let policy_v4: Value = serde_json::from_slice(
            &std::fs::read(repo_root.join("policies/lean-attester-policy-v4.json")).unwrap(),
        )
        .unwrap();
        let expected_env_sha = policy_v4["environment_lock_sha256"].as_str().unwrap();
        assert_eq!(
            expected_env_sha,
            "sha256:2dbb38af494fbd820a4920b5ed36fb7834ab6e3c551f77f535a6f0a73fdb2a43"
        );

        let pkg_root = repo_root.join("proofs/SwarmProofs");
        let lock: EnvironmentLock =
            serde_json::from_slice(&std::fs::read(pkg_root.join("environment-lock.json")).unwrap())
                .unwrap();
        assert_eq!(environment_id(&lock).unwrap(), expected_env_sha);

        let reg: RegistryManifest =
            serde_json::from_slice(&std::fs::read(pkg_root.join("registry.json")).unwrap())
                .unwrap();
        validate_registry(&reg).unwrap();
        validate_frozen_library_modules(&pkg_root, &reg).unwrap();

        let registry_v4 = reg
            .environments
            .iter()
            .find(|e| e.lock.policy_version == 4)
            .unwrap();
        assert_eq!(registry_v4.environment_id, expected_env_sha);
        assert_eq!(registry_v4.frozen_library_modules.len(), 8);

        // Verify weil-correlation.challenge.json matches restored environment_id and empty bundle_id
        let weil: ChallengeManifest = serde_json::from_slice(
            &std::fs::read(repo_root.join("proofs/gram-ladder/weil-correlation.challenge.json"))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(weil.environment_id, expected_env_sha);
        let empty_bundle = resolve_bundle(&reg, expected_env_sha, &[]).unwrap();
        assert_eq!(weil.dependency_bundle_id, bundle_id(&empty_bundle).unwrap());

        // Verify SBOM.spdx.json namespace and repo links match SwarmProofs + environment_id
        let sbom: Value =
            serde_json::from_slice(&std::fs::read(pkg_root.join("SBOM.spdx.json")).unwrap())
                .unwrap();
        let doc_ns = sbom["documentNamespace"].as_str().unwrap();
        let env_hex = expected_env_sha.strip_prefix("sha256:").unwrap();
        assert!(
            doc_ns.starts_with("https://github.com/corsur/swarm-tips/proofs/SwarmProofs/")
                && doc_ns.ends_with(env_hex),
            "unexpected SBOM documentNamespace: {doc_ns}"
        );
    }

    #[test]
    fn compose_generated_module_source_policy_parity_and_comment_stripping() {
        let base_stmt =
            "-- meta constant sorry instance in line comment\n/- nested /- @[simp] class -/ block comment -/\ndef statementProp : Prop := True";
        let make_cand = |stmt: &str, proof_body: &str| {
            let full_proof = format!("-- SPDX-License-Identifier: Apache-2.0\n{proof_body}");
            PromotionCandidate {
                schema: "swarm.lean-promotion-candidate/v1".into(),
                task_id: "task-parity".into(),
                campaign_id: "camp-parity".into(),
                network: "mainnet".into(),
                author_wallet: "wallet".into(),
                artifact_sha256: sha256_hex(full_proof.as_bytes()),
                statement: stmt.into(),
                proof_source: full_proof,
                challenge: ChallengeManifest {
                    schema: CHALLENGE_SCHEMA_VERSION.into(),
                    statement_sha256: sha256_hex(stmt.as_bytes()),
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
            }
        };

        // Comments containing banned words do not false-trigger:
        assert!(compose_generated_module(&make_cand(
            base_stmt,
            "-- instance sorry @[simp]\ntheorem proof : statementProp := trivial"
        ))
        .is_ok());

        // Exact S-a required negative cases:
        for bad_proof in [
            "@[simp] theorem helper : True := trivial\ntheorem proof : statementProp := trivial",
            "instance:\n  Inhabited Nat := ⟨0⟩\ntheorem proof : statementProp := trivial",
            "inductive Sneaky where | mk\n  deriving DecidableEq\ntheorem proof : statementProp := trivial",
            "meta def sneakyMeta : Nat := 0\ntheorem proof : statementProp := trivial",
            "opaque sneakyOpaque : Nat := 0\ntheorem proof : statementProp := trivial",
            "constant sneakyConst : Nat\ntheorem proof : statementProp := trivial",
            "theorem proof : statementProp := sorryAx statementProp false",
            "theorem proof : statementProp := Lean.ofReduceBool true rfl",
            "open Classical in\ntheorem proof : statementProp := trivial",
            "/- unclosed comment instance",
        ] {
            assert!(
                compose_generated_module(&make_cand(base_stmt, bad_proof)).is_err(),
                "expected compose_generated_module to reject proof:\n{bad_proof}"
            );
        }

        // SF3 mandatory negative cases: string & char literals must NEVER spoof line/block comments!
        for spoof_proof in [
            "theorem proof : statementProp := by\n  have h : \"--\" = \"--\" := rfl; native_decide",
            "theorem proof : statementProp := by\n  have a : String := \"/-\"; native_decide; have b : String := \"-/\"",
            "theorem proof : statementProp := by\n  have c : String := \"\\\"--\"; sorry",
            "theorem proof : statementProp := by\n  have c1 : Char := '/'; have c2 : Char := '-'; sorry",
            "theorem proof : statementProp := by\n  let f' : Char → Char := id; let _ := f' '-'; sorry",
            "theorem proof : statementProp := by\n  have s : String := s!\"{sorry}\"; exact trivial",
            "theorem proof : statementProp := by\n  have r : String := r#\"\\\"#; sorry; have q : String := \"#\"",
            "theorem proof : statementProp := by\n  have u : String := \"unclosed",
        ] {
            assert!(
                compose_generated_module(&make_cand(base_stmt, spoof_proof)).is_err(),
                "SF3 expected compose_generated_module to reject spoofed proof:\n{spoof_proof}"
            );
        }

        // SF3 positive cases: identifier primes (`local'`, `f' x`) and benign char/string literals work cleanly:
        for ok_proof in [
            "def local' : Prop := True\ntheorem proof : statementProp := by\n  let f' : Nat → Nat := fun x => x\n  have _ : f' 0 = 0 := rfl\n  exact trivial",
            "theorem proof : statementProp := by\n  have _ : Char := '-'\n  have _ : Char := '\\''\n  have _ : String := \"\\\\\"\n  exact trivial",
        ] {
            assert!(
                compose_generated_module(&make_cand(base_stmt, ok_proof)).is_ok(),
                "SF3 expected compose_generated_module to accept valid proof:\n{ok_proof}"
            );
        }

        // P3-3 regression: block comments or line comments mentioning `theorem disproof`
        // or negated `theorem proof` MUST NOT classify a normal proof as a disproof!
        for commented_proof in [
            "/-\ntheorem disproof : ¬ statementProp := by\n  exact False.elim\n-/\ntheorem proof : statementProp := trivial",
            "theorem proof : statementProp := by\n  -- theorem proof : ¬ statementProp\n  exact trivial",
        ] {
            let generated = compose_generated_module(&make_cand(base_stmt, commented_proof))
                .expect("commented disproof mention in valid proof must compose");
            assert!(
                generated.theorem_name.ends_with(".proof"),
                "commented disproof must keep .proof entry_name, got {}",
                generated.theorem_name
            );
            assert!(
                !generated.theorem_signature.contains("¬"),
                "commented disproof must not negate theorem_signature, got {}",
                generated.theorem_signature
            );
        }

        // Real uncommented `theorem disproof` is still detected as disproof.
        let real_disproof = compose_generated_module(&make_cand(
            "def statementProp : Prop := False",
            "theorem disproof : ¬ statementProp := fun h => h",
        ))
        .expect("real disproof must compose");
        assert!(real_disproof.theorem_name.ends_with(".disproof"));
        assert!(real_disproof.theorem_signature.contains("¬"));
    }

    #[test]
    fn frozen_v1_defs_hashes_are_immutable_and_append_only() {
        let pkg_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .unwrap()
            .join("proofs")
            .join("SwarmProofs");
        let reg: RegistryManifest =
            serde_json::from_slice(&std::fs::read(pkg_root.join("registry.json")).unwrap())
                .unwrap();
        validate_registry(&reg).unwrap();
        validate_frozen_library_modules(&pkg_root, &reg).unwrap();

        let v4_env = reg
            .environments
            .iter()
            .find(|env| env.lock.policy_version == 4)
            .expect("missing policy_version=4 environment in registry.json");

        // Exact hard-coded golden SHA-256 hashes for all 8 V1 Defs modules:
        let expected_v1: [(&str, &str, &str); 8] = [
            (
                "SwarmProofs.Chains.BSD.Defs.V1",
                "SwarmProofs/Chains/BSD/Defs/V1.lean",
                "sha256:c2c3edf31528a5287c992f26b010140c99dd10536ec1cd7527041545222e704d",
            ),
            (
                "SwarmProofs.Chains.BoolFourier.Defs.V1",
                "SwarmProofs/Chains/BoolFourier/Defs/V1.lean",
                "sha256:c2dc1cb8e6f53729d1a47bb6bac2156d1c411c0b2ddb19526347e685bf467c09",
            ),
            (
                "SwarmProofs.Chains.Circuits.Defs.V1",
                "SwarmProofs/Chains/Circuits/Defs/V1.lean",
                "sha256:00cd43b77425913085d1a1d8ab6d2664c5857ee7495a5c1b5b89e798ceef667d",
            ),
            (
                "SwarmProofs.Chains.NavierStokes.Defs.V1",
                "SwarmProofs/Chains/NavierStokes/Defs/V1.lean",
                "sha256:74ee21968d313a9d67b8900f478a0da591c0ad38565eab01915637e8d17489ed",
            ),
            (
                "SwarmProofs.Chains.ProtocolMath.Defs.V1",
                "SwarmProofs/Chains/ProtocolMath/Defs/V1.lean",
                "sha256:b262371196d872e244cd60b44dabfd3654f10a247ad75226e85bc9d55bb40e5c",
            ),
            (
                "SwarmProofs.Chains.QuantumInfo.Defs.V1",
                "SwarmProofs/Chains/QuantumInfo/Defs/V1.lean",
                "sha256:9b0d768bdd78bdc3e84efa900022b1c94e6e0a8aef3f84b57e7d19dc471ee8b4",
            ),
            (
                "SwarmProofs.Chains.RHAnalytic.Defs.V1",
                "SwarmProofs/Chains/RHAnalytic/Defs/V1.lean",
                "sha256:5777771ee0a643240cb1f6d84994658c0008dd04ea32ae665c560a48329dfbe6",
            ),
            (
                "SwarmProofs.Chains.RHLagarias.Defs.V1",
                "SwarmProofs/Chains/RHLagarias/Defs/V1.lean",
                "sha256:c590bd97d7b40f604c9a258c6f2c9197ab64d24036cea0ecee34b91efa873300",
            ),
        ];
        assert_eq!(FROZEN_V1_DEFS_GOLDEN_HASHES, &expected_v1);

        for (mod_name, rel_file, pinned_sha) in expected_v1 {
            assert_eq!(
                v4_env.frozen_library_modules.get(mod_name).map(String::as_str),
                Some(pinned_sha),
                "registry.json frozen_library_modules entry for {mod_name} drifted from pinned V1 hash"
            );
            let disk_bytes = std::fs::read(pkg_root.join(rel_file)).unwrap();
            let disk_sha = format!("sha256:{}", sha256_hex(&disk_bytes));
            assert_eq!(
                disk_sha, pinned_sha,
                "disk file {rel_file} drifted from pinned V1 hash"
            );
        }

        // Append-only checks: unchanged -> ok; adding V2 -> ok; mutating or deleting V1 -> error!
        validate_frozen_defs_append_only(&reg, &reg).unwrap();

        let mut with_v2 = reg.clone();
        with_v2.environments[0]
            .frozen_library_modules
            .insert("SwarmProofs.Chains.BSD.Defs.V2".into(), id('2'));
        validate_frozen_defs_append_only(&reg, &with_v2).unwrap();

        let mut mutated_v1 = reg.clone();
        mutated_v1.environments[0]
            .frozen_library_modules
            .insert("SwarmProofs.Chains.BSD.Defs.V1".into(), id('9'));
        assert!(validate_frozen_defs_append_only(&reg, &mutated_v1).is_err());

        let mut deleted_v1 = reg.clone();
        deleted_v1.environments[0]
            .frozen_library_modules
            .remove("SwarmProofs.Chains.BSD.Defs.V1");
        assert!(validate_frozen_defs_append_only(&reg, &deleted_v1).is_err());
    }
}
