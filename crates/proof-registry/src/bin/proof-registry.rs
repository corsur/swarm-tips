use proof_registry::{
    bundle_id, compose_generated_module, generated_module_name, module_id, resolve_bundle,
    sha256_hex, ModuleStatus, ModuleVerification, PromotionCandidate, ProofModuleManifest,
    RegistryManifest, APACHE_2_0, SCHEMA_VERSION,
};
use std::path::{Path, PathBuf};

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
    if candidate.schema != "swarm.lean-promotion-candidate/v1"
        || candidate.status != "verified_pending_finalization"
        || candidate.task_id.is_empty()
        || candidate.campaign_id.is_empty()
        || candidate.network.is_empty()
        || candidate.author_wallet.is_empty()
        || sha256_hex(candidate.proof_source.as_bytes()) != candidate.artifact_sha256
        || sha256_hex(candidate.statement.as_bytes()) != candidate.challenge.statement_sha256
        || candidate.challenge.schema != proof_registry::CHALLENGE_SCHEMA_VERSION
        || candidate.challenge.policy_version != 4
        || !candidate.challenge.reusable
        || candidate.challenge.reuse_license != APACHE_2_0
        || candidate
            .verification_axioms
            .iter()
            .any(|axiom| !ALLOWED_AXIOMS.contains(&axiom.as_str()))
    {
        return Err("candidate hashes, schema, or status do not match".into());
    }
    let mut registry = read_registry(registry_path)?;
    if !registry
        .environments
        .iter()
        .any(|environment| environment.environment_id == candidate.challenge.environment_id)
    {
        return Err("candidate uses an unknown environment".into());
    }
    let mut direct_dependencies = candidate.direct_dependencies.clone();
    direct_dependencies.sort();
    direct_dependencies.dedup();
    let bundle = resolve_bundle(
        &registry,
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
        return Err("candidate dependency closure or challenge does not match".into());
    }

    let generated = compose_generated_module(&candidate)?;
    let source_sha256 = sha256_hex(generated.source.as_bytes());
    let mut manifest = ProofModuleManifest {
        schema: SCHEMA_VERSION.into(),
        module_id: String::new(),
        module_name: String::new(),
        theorem_name: generated.theorem_name,
        theorem_signature: generated.theorem_signature,
        environment_id: candidate.challenge.environment_id,
        source_sha256: source_sha256.clone(),
        source_url: String::new(),
        source_bytes: generated.source.len() as u64,
        direct_dependencies,
        originating_task_id: candidate.task_id.clone(),
        originating_network: candidate.network,
        author_wallet: candidate.author_wallet,
        license: APACHE_2_0.into(),
        verification: ModuleVerification {
            lean_kernel: true,
            axiom_audit: true,
            independent_kernel: true,
            axioms: candidate.verification_axioms,
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
        .ok_or("invalid generated module id")?;
    manifest.source_url = format!(
        "{}/modules/{id_hex}.lean",
        public_base_url.trim_end_matches('/')
    );
    if let Some(existing) = registry
        .modules
        .iter()
        .find(|existing| existing.module_id == manifest.module_id)
    {
        if proof_registry::module_id(existing)? != manifest.module_id {
            return Err("module id collision with different manifest".into());
        }
        let existing_path =
            package_root.join(format!("{}.lean", existing.module_name.replace('.', "/")));
        if std::fs::read(&existing_path)? != generated.source.as_bytes() {
            return Err("existing immutable module bytes differ".into());
        }
        // Retrying must not reactivate a yanked module or overwrite later audit
        // evidence and reviewed catalog edits.
        println!("{}", manifest.module_id);
        return Ok(());
    }

    let relative = format!("{}.lean", manifest.module_name.replace('.', "/"));
    let output = package_root.join(PathBuf::from(relative));
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&output, generated.source)?;
    registry.modules.push(manifest.clone());
    registry
        .modules
        .sort_by(|left, right| left.module_id.cmp(&right.module_id));
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
        bundle_id, environment_id, ChallengeManifest, DependencyBundle, EnvironmentLock,
        EnvironmentRecord, ExternalPackage, CHALLENGE_SCHEMA_VERSION,
    };
    use std::collections::BTreeMap;
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
