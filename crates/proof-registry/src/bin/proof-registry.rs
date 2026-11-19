use proof_registry::{
    generated_module_name, preview_autopromote_manifest, validate_frozen_library_modules,
    ModuleStatus, PromotionCandidate, RegistryManifest,
};
use std::path::Path;

const ALLOWED_AXIOMS: &[&str] = &["propext", "Classical.choice", "Quot.sound"];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    match args.get(1).map(String::as_str) {
        Some("validate") if args.len() == 3 => validate(Path::new(&args[2])),
        Some("prepare" | "autopromote") if (6..=8).contains(&args.len()) => {
            let alias = parse_optional_alias(&args[6..])?;
            prepare(
                Path::new(&args[2]),
                Path::new(&args[3]),
                Path::new(&args[4]),
                &args[5],
                alias.as_deref(),
            )
        }
        Some("yank") if (5..=6).contains(&args.len()) => yank(
            Path::new(&args[2]),
            &args[3],
            &args[4],
            args.get(5).map(String::as_str),
        ),
        _ => Err(
            "usage: proof-registry validate <registry> | prepare|autopromote <candidate> <registry> <package-root> <public-base-url> [--alias <alias>] | yank <registry> <module-id> <deprecated|revoked> [replacement]"
                .into(),
        ),
    }
}

fn read_registry(path: &Path) -> Result<RegistryManifest, Box<dyn std::error::Error>> {
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}

fn write_registry(
    path: &Path,
    registry: &RegistryManifest,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut bytes = serde_json::to_vec_pretty(registry)?;
    bytes.push(b'\n');
    std::fs::write(path, bytes)?;
    Ok(())
}

fn validate(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let registry = read_registry(path)?;
    proof_registry::validate_registry(&registry)?;
    for module in &registry.modules {
        if module
            .verification
            .axioms
            .iter()
            .any(|axiom| !ALLOWED_AXIOMS.contains(&axiom.as_str()))
        {
            return Err(format!("invalid module manifest: {}", module.module_id).into());
        }
    }
    let pkg_root = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    if pkg_root.join("SwarmProofs").join("Chains").is_dir()
        || pkg_root.join("environment-lock.json").is_file()
        || registry
            .environments
            .iter()
            .any(|env| !env.frozen_library_modules.is_empty())
    {
        validate_frozen_library_modules(pkg_root, &registry)?;
    }
    println!("validated {} module(s)", registry.modules.len());
    Ok(())
}

fn parse_optional_alias(extra: &[String]) -> Result<Option<String>, Box<dyn std::error::Error>> {
    match extra {
        [] => Ok(None),
        [flag, value] if flag == "--alias" && !value.trim().is_empty() => Ok(Some(value.clone())),
        [combined] if combined.starts_with("--alias=") => {
            let val = combined.trim_start_matches("--alias=").trim();
            if val.is_empty() {
                return Err("--alias requires a non-empty value".into());
            }
            Ok(Some(val.to_string()))
        }
        _ => Err(
            "invalid extra arguments for prepare/autopromote; expected [--alias <alias>]".into(),
        ),
    }
}

fn prepare(
    candidate_path: &Path,
    registry_path: &Path,
    package_root: &Path,
    public_base_url: &str,
    alias: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let candidate: PromotionCandidate = serde_json::from_slice(&std::fs::read(candidate_path)?)?;
    let mut registry = read_registry(registry_path)?;
    if let Some(alias_key) = alias {
        if alias_key.trim().is_empty() {
            return Err("alias must be non-empty".into());
        }
        let (preview_manifest, _) =
            preview_autopromote_manifest(&candidate, &registry, public_base_url)?;
        if let Some(existing_id) = registry.aliases.get(alias_key) {
            if existing_id != &preview_manifest.module_id {
                return Err(format!(
                    "refusing to overwrite existing alias `{alias_key}`: mapped to {existing_id}, candidate is {}",
                    preview_manifest.module_id
                )
                .into());
            }
        }
    }
    let manifest = proof_registry::autopromote_candidate(
        &candidate,
        &mut registry,
        package_root,
        public_base_url,
    )?;
    if let Some(alias_key) = alias {
        registry
            .aliases
            .insert(alias_key.to_string(), manifest.module_id.clone());
    }
    write_registry(registry_path, &registry)?;
    validate(registry_path)?;
    println!("{}", manifest.module_id);
    Ok(())
}

fn yank(
    registry_path: &Path,
    id: &str,
    status: &str,
    replacement: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut registry = read_registry(registry_path)?;
    let status = match status {
        "deprecated" => ModuleStatus::Deprecated,
        "revoked" => ModuleStatus::Revoked,
        _ => return Err("yank status must be deprecated or revoked".into()),
    };
    if let Some(replacement) = replacement {
        generated_module_name(replacement)?;
        if !registry
            .modules
            .iter()
            .any(|module| module.module_id == replacement)
        {
            return Err("replacement module is unknown".into());
        }
    }
    let module = registry
        .modules
        .iter_mut()
        .find(|module| module.module_id == id)
        .ok_or("module is unknown")?;
    module.status = status;
    module.replacement = replacement.map(ToOwned::to_owned);
    write_registry(registry_path, &registry)?;
    validate(registry_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proof_registry::{
        bundle_id, environment_id, sha256_hex, ChallengeManifest, DependencyBundle,
        EnvironmentLock, EnvironmentRecord, ExternalPackage, APACHE_2_0, CHALLENGE_SCHEMA_VERSION,
        SCHEMA_VERSION,
    };
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temporary_directory() -> PathBuf {
        let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "proof-registry-test-{}-{nonce}-{seq}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn environment() -> (String, EnvironmentLock) {
        let lock = EnvironmentLock {
            schema: "swarm.lean-environment/v1".into(),
            name: "test".into(),
            toolchain: "leanprover/lean4:v4.33.0-rc2".into(),
            mathlib_revision: "1".repeat(40),
            policy_version: 4,
            registry_schema: SCHEMA_VERSION.into(),
            allowed_import_roots: vec!["Mathlib".into(), "Zeta23".into(), "SwarmProofs".into()],
            external_packages: vec![ExternalPackage {
                name: "Zeta23".into(),
                repository: "https://example.invalid/formal-math".into(),
                revision: "2".repeat(40),
                subdirectory: None,
                license: APACHE_2_0.into(),
            }],
        };
        (environment_id(&lock).unwrap(), lock)
    }

    fn candidate(environment_id: &str) -> PromotionCandidate {
        let statement = "def statementProp : Prop := True";
        let proof =
            "-- SPDX-License-Identifier: Apache-2.0\ntheorem proof : statementProp := trivial";
        let bundle = DependencyBundle {
            schema: SCHEMA_VERSION.into(),
            environment_id: environment_id.into(),
            modules: Vec::new(),
        };
        PromotionCandidate {
            schema: "swarm.lean-promotion-candidate/v1".into(),
            task_id: "task-1".into(),
            campaign_id: "campaign-1".into(),
            network: "devnet".into(),
            author_wallet: "wallet".into(),
            artifact_sha256: sha256_hex(proof.as_bytes()),
            statement: statement.into(),
            proof_source: proof.into(),
            challenge: ChallengeManifest {
                schema: CHALLENGE_SCHEMA_VERSION.into(),
                statement_sha256: sha256_hex(statement.as_bytes()),
                policy_version: 4,
                environment_id: environment_id.into(),
                dependency_bundle_id: bundle_id(&bundle).unwrap(),
                reuse_license: APACHE_2_0.into(),
                reusable: true,
            },
            direct_dependencies: Vec::new(),
            dependency_closure: Vec::new(),
            exact_imports: Vec::new(),
            verification_axioms: vec!["propext".into()],
            status: "verified_pending_finalization".into(),
        }
    }

    #[test]
    fn promotion_is_idempotent_and_rejects_tampered_artifacts() {
        let root = temporary_directory();
        let registry_path = root.join("registry.json");
        let candidate_path = root.join("candidate.json");
        let (environment_id, lock) = environment();
        let registry = RegistryManifest {
            schema: SCHEMA_VERSION.into(),
            environments: vec![EnvironmentRecord {
                environment_id: environment_id.clone(),
                lock,
                frozen_library_modules: BTreeMap::new(),
            }],
            modules: Vec::new(),
            aliases: BTreeMap::new(),
        };
        write_registry(&registry_path, &registry).unwrap();
        let candidate = candidate(&environment_id);
        std::fs::write(&candidate_path, serde_json::to_vec(&candidate).unwrap()).unwrap();

        prepare(
            &candidate_path,
            &registry_path,
            &root,
            "https://storage.googleapis.com/example",
            None,
        )
        .unwrap();
        let after_first = std::fs::read(&registry_path).unwrap();
        prepare(
            &candidate_path,
            &registry_path,
            &root,
            "https://storage.googleapis.com/example",
            None,
        )
        .unwrap();
        assert_eq!(std::fs::read(&registry_path).unwrap(), after_first);
        assert_eq!(read_registry(&registry_path).unwrap().modules.len(), 1);

        let promoted_id = read_registry(&registry_path).unwrap().modules[0]
            .module_id
            .clone();
        yank(&registry_path, &promoted_id, "revoked", None).unwrap();
        let after_yank = std::fs::read(&registry_path).unwrap();
        prepare(
            &candidate_path,
            &registry_path,
            &root,
            "https://storage.googleapis.com/example",
            None,
        )
        .unwrap();
        assert_eq!(std::fs::read(&registry_path).unwrap(), after_yank);

        let mut tampered = candidate;
        tampered.proof_source.push_str("\n-- altered");
        std::fs::write(&candidate_path, serde_json::to_vec(&tampered).unwrap()).unwrap();
        assert!(prepare(
            &candidate_path,
            &registry_path,
            &root,
            "https://storage.googleapis.com/example",
            None,
        )
        .is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn autopromote_alias_flag_records_and_validates_alias() {
        let root = temporary_directory();
        let registry_path = root.join("registry.json");
        let candidate_path = root.join("candidate.json");
        let (environment_id, lock) = environment();
        let registry = RegistryManifest {
            schema: SCHEMA_VERSION.into(),
            environments: vec![EnvironmentRecord {
                environment_id: environment_id.clone(),
                lock,
                frozen_library_modules: BTreeMap::new(),
            }],
            modules: Vec::new(),
            aliases: BTreeMap::new(),
        };
        write_registry(&registry_path, &registry).unwrap();
        let cand = candidate(&environment_id);
        std::fs::write(&candidate_path, serde_json::to_vec(&cand).unwrap()).unwrap();

        assert_eq!(
            parse_optional_alias(&["--alias".into(), "catalog:chain-bsd-1".into()]).unwrap(),
            Some("catalog:chain-bsd-1".into())
        );
        assert!(parse_optional_alias(&["--alias=".into()]).is_err());

        prepare(
            &candidate_path,
            &registry_path,
            &root,
            "https://storage.googleapis.com/example",
            Some("catalog:chain-bsd-1"),
        )
        .unwrap();
        let updated = read_registry(&registry_path).unwrap();
        assert_eq!(
            updated.aliases.get("catalog:chain-bsd-1"),
            Some(&updated.modules[0].module_id)
        );

        // Idempotent re-run with the SAME candidate and SAME alias succeeds:
        prepare(
            &candidate_path,
            &registry_path,
            &root,
            "https://storage.googleapis.com/example",
            Some("catalog:chain-bsd-1"),
        )
        .unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn alias_hijack_overwrite_is_refused() {
        let root = temporary_directory();
        let registry_path = root.join("registry.json");
        let cand1_path = root.join("candidate1.json");
        let cand2_path = root.join("candidate2.json");
        let (environment_id, lock) = environment();
        let registry = RegistryManifest {
            schema: SCHEMA_VERSION.into(),
            environments: vec![EnvironmentRecord {
                environment_id: environment_id.clone(),
                lock,
                frozen_library_modules: BTreeMap::new(),
            }],
            modules: Vec::new(),
            aliases: BTreeMap::new(),
        };
        write_registry(&registry_path, &registry).unwrap();
        let cand1 = candidate(&environment_id);
        std::fs::write(&cand1_path, serde_json::to_vec(&cand1).unwrap()).unwrap();

        prepare(
            &cand1_path,
            &registry_path,
            &root,
            "https://storage.googleapis.com/example",
            Some("catalog:chain-bsd-1-weierstrass-bijection"),
        )
        .unwrap();
        let snapshot_reg = std::fs::read(&registry_path).unwrap();
        let gen_dir = root.join("SwarmProofs").join("Generated");
        let files_before = std::fs::read_dir(&gen_dir).unwrap().count();
        assert_eq!(files_before, 1);

        // Second candidate with different statement/task trying to overwrite the alias:
        let hijack_stmt = "def statementProp : Prop := 1 = 1";
        let hijack_proof =
            "-- SPDX-License-Identifier: Apache-2.0\ntheorem proof : statementProp := rfl";
        let mut cand2 = candidate(&environment_id);
        cand2.task_id = "task-hijack".into();
        cand2.statement = hijack_stmt.into();
        cand2.proof_source = hijack_proof.into();
        cand2.artifact_sha256 = sha256_hex(hijack_proof.as_bytes());
        cand2.challenge.statement_sha256 = sha256_hex(hijack_stmt.as_bytes());
        std::fs::write(&cand2_path, serde_json::to_vec(&cand2).unwrap()).unwrap();

        let err = prepare(
            &cand2_path,
            &registry_path,
            &root,
            "https://storage.googleapis.com/example",
            Some("catalog:chain-bsd-1-weierstrass-bijection"),
        )
        .unwrap_err()
        .to_string();
        assert!(
            err.contains("refusing to overwrite existing alias"),
            "unexpected error: {err}"
        );

        // Verify zero files or registry bytes were mutated on disk!
        assert_eq!(std::fs::read(&registry_path).unwrap(), snapshot_reg);
        assert_eq!(std::fs::read_dir(&gen_dir).unwrap().count(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }
}
