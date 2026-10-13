use proof_registry::{generated_module_name, ModuleStatus, PromotionCandidate, RegistryManifest};
use std::path::Path;

const ALLOWED_AXIOMS: &[&str] = &["propext", "Classical.choice", "Quot.sound"];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    match args.get(1).map(String::as_str) {
        Some("validate") if args.len() == 3 => validate(Path::new(&args[2])),
        Some("prepare") if args.len() == 6 => prepare(
            Path::new(&args[2]),
            Path::new(&args[3]),
            Path::new(&args[4]),
            &args[5],
        ),
        Some("yank") if (5..=6).contains(&args.len()) => yank(
            Path::new(&args[2]),
            &args[3],
            &args[4],
            args.get(5).map(String::as_str),
        ),
        _ => Err(
            "usage: proof-registry validate <registry> | prepare <candidate> <registry> <package-root> <public-base-url> | yank <registry> <module-id> <deprecated|revoked> [replacement]"
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
    println!("validated {} module(s)", registry.modules.len());
    Ok(())
}

fn prepare(
    candidate_path: &Path,
    registry_path: &Path,
    package_root: &Path,
    public_base_url: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let candidate: PromotionCandidate = serde_json::from_slice(&std::fs::read(candidate_path)?)?;
    let mut registry = read_registry(registry_path)?;
    let manifest = proof_registry::autopromote_candidate(
        &candidate,
        &mut registry,
        package_root,
        public_base_url,
    )?;
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
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_directory() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "proof-registry-test-{}-{nonce}",
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
        )
        .unwrap();
        let after_first = std::fs::read(&registry_path).unwrap();
        prepare(
            &candidate_path,
            &registry_path,
            &root,
            "https://storage.googleapis.com/example",
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
        )
        .is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
