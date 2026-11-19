use crate::http::{
    gcloud_access_token, percent_encode, CurlTransport, HttpFailure, HttpRequest, HttpTransport,
};
use anyhow::{bail, ensure, Context, Result};
use proof_registry::{
    bundle_id, compose_generated_module, generated_module_name, preview_autopromote_manifest,
    resolve_bundle, sha256_hex, validate_registry, ChallengeManifest, PromotionCandidate,
    RegistryManifest, ALLOWED_AXIOMS, APACHE_2_0, CHALLENGE_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub const DEFAULT_SHILLBOT_API: &str = "https://api.shillbot.org";
pub const DEFAULT_NETWORK: &str = "mainnet";
pub const DEFAULT_ARTIFACT_BUCKET: &str = "coordination-game-prod-swarm-tips";
pub const DEFAULT_GCP_PROJECT: &str = "coordination-game-prod";
pub const DEFAULT_PUBLIC_REGISTRY_BASE_URL: &str =
    "https://storage.googleapis.com/coordination-game-prod-swarm-proof-registry-public";
pub const PINNED_POSTER_WALLETS: &[&str] = &["CKsZ7ZMLLUzbHUeu2Vm5mjuB8QQi3vfvqvXFdFxT7xmY"];
const LEAN_PROOF_PLATFORM: u64 = 10;
const PASSING_COMPOSITE_SCORE: u64 = 1_000_000;
const MAX_FIRESTORE_SCAN_PAGES: usize = 20;
const MAX_TASK_LIST_PAGES: usize = 50;
const TASK_LIST_PAGE_LIMIT: usize = 100;

pub const REJECTED_CANDIDATES_SCHEMA: &str = "swarm.lean-rejected-candidates/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedRejectionEntry {
    pub artifact_sha256: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RejectedCandidatesManifest {
    pub schema: String,
    #[serde(default)]
    pub rejected: BTreeMap<String, PersistedRejectionEntry>,
}

impl Default for RejectedCandidatesManifest {
    fn default() -> Self {
        Self {
            schema: REJECTED_CANDIDATES_SCHEMA.to_string(),
            rejected: BTreeMap::new(),
        }
    }
}

pub fn load_rejected_candidates_manifest(
    path: Option<&Path>,
) -> Result<RejectedCandidatesManifest> {
    let Some(p) = path else {
        return Ok(RejectedCandidatesManifest::default());
    };
    if !p.is_file() {
        return Ok(RejectedCandidatesManifest::default());
    }
    let bytes = std::fs::read(p)
        .with_context(|| format!("failed reading rejected candidates file {}", p.display()))?;
    let parsed: RejectedCandidatesManifest =
        serde_json::from_slice(&bytes).context("invalid rejected-candidates.json")?;
    ensure!(
        parsed.schema == REJECTED_CANDIDATES_SCHEMA,
        "unexpected rejected-candidates.json schema '{}'",
        parsed.schema
    );
    Ok(parsed)
}

pub fn save_rejected_candidates_manifest(
    path: &Path,
    manifest: &RejectedCandidatesManifest,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let mut bytes = serde_json::to_vec_pretty(manifest)?;
    bytes.push(b'\n');
    std::fs::write(path, bytes).with_context(|| format!("failed writing {}", path.display()))?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct FetchCandidatesArgs {
    pub api_base: String,
    pub network: String,
    pub bucket: String,
    pub project: String,
    pub registry_path: PathBuf,
    pub catalog_dir: PathBuf,
    pub allowed_posters: Vec<String>,
    pub out_dir: PathBuf,
    pub tasks_file: Option<PathBuf>,
    pub firestore_fallback: bool,
    pub rejected_file: Option<PathBuf>,
    pub dry_run_registry: Option<PathBuf>,
    pub only_task_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FetchedCandidateSummary {
    pub task_id: String,
    pub candidate_file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    pub artifact_sha256: String,
    pub statement_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RejectedCandidate {
    pub task_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FetchReport {
    pub candidates: Vec<FetchedCandidateSummary>,
    pub skipped: usize,
    pub already_promoted: usize,
    pub rejected: Vec<RejectedCandidate>,
}

#[derive(Debug, Clone)]
pub struct RawCandidateInputs {
    pub task_id: String,
    pub campaign_id: String,
    pub network: String,
    pub author_wallet: String,
    pub campaign_topic: Option<String>,
    pub artifact_sha256: String,
    pub proof_bytes: Vec<u8>,
    pub expected_statement_sha256: Option<String>,
    pub statement_bytes: Vec<u8>,
    pub direct_dependencies: Vec<String>,
    pub verification_axioms: Vec<String>,
}

pub fn run_fetch_promotion_candidates(args: &FetchCandidatesArgs) -> Result<()> {
    let token = gcloud_access_token().ok();
    let transport = CurlTransport;
    let report = fetch_promotion_candidates_with(&transport, args, token.as_deref())?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

pub fn parse_depends_on_header(statement: &str) -> Vec<String> {
    let mut deps = Vec::new();
    for line in statement.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("--") {
            let meta = rest.trim();
            if let Some(raw_dep) = meta.strip_prefix("@depends-on:") {
                for item in raw_dep.split(',') {
                    let clean = item.trim();
                    if !clean.is_empty() {
                        deps.push(clean.to_string());
                    }
                }
            }
        } else if !trimmed.is_empty() {
            break;
        }
    }
    deps
}

pub fn has_apache_spdx_header(proof_source: &str) -> bool {
    proof_source
        .lines()
        .take(5)
        .any(|line| line.contains("SPDX-License-Identifier: Apache-2.0"))
}

fn is_lower_hex_64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

pub fn sanitize_task_filename(task_id: &str) -> Result<String> {
    ensure!(!task_id.trim().is_empty(), "task_id must be non-empty");
    let mut out = String::with_capacity(task_id.len());
    for ch in task_id.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    let trimmed = out.trim_matches('.');
    ensure!(
        !trimmed.is_empty() && !trimmed.contains(".."),
        "invalid sanitized task filename for `{task_id}`"
    );
    Ok(trimmed.to_string())
}

/// Validate hashes, SPDX license header, dependency closure, and source policy
/// to construct a canonical [`PromotionCandidate`].
pub fn validate_and_build_candidate(
    registry: &RegistryManifest,
    environment_id: &str,
    inputs: RawCandidateInputs,
) -> Result<(PromotionCandidate, Option<String>)> {
    ensure!(
        is_lower_hex_64(&inputs.artifact_sha256),
        "invalid artifact_sha256 `{}` for task {}",
        inputs.artifact_sha256,
        inputs.task_id
    );
    let actual_proof_sha = sha256_hex(&inputs.proof_bytes);
    ensure!(
        actual_proof_sha == inputs.artifact_sha256,
        "proof SHA-256 mismatch for task {}: expected {}, got {}",
        inputs.task_id,
        inputs.artifact_sha256,
        actual_proof_sha
    );

    let actual_stmt_sha = sha256_hex(&inputs.statement_bytes);
    if let Some(expected_stmt_sha) = inputs.expected_statement_sha256.as_deref() {
        ensure!(
            is_lower_hex_64(expected_stmt_sha),
            "invalid statement_sha256 `{expected_stmt_sha}` for task {}",
            inputs.task_id
        );
        ensure!(
            actual_stmt_sha == expected_stmt_sha,
            "statement SHA-256 mismatch for task {}: expected {}, got {}",
            inputs.task_id,
            expected_stmt_sha,
            actual_stmt_sha
        );
    }

    let proof_source = String::from_utf8(inputs.proof_bytes).with_context(|| {
        format!(
            "proof bytes for task {} are not valid UTF-8",
            inputs.task_id
        )
    })?;
    let statement = String::from_utf8(inputs.statement_bytes).with_context(|| {
        format!(
            "statement bytes for task {} are not valid UTF-8",
            inputs.task_id
        )
    })?;

    ensure!(
        has_apache_spdx_header(&proof_source),
        "missing `-- SPDX-License-Identifier: Apache-2.0` header in first 5 lines for task {}",
        inputs.task_id
    );

    let mut direct_dependencies = inputs.direct_dependencies;
    for dep_rung in parse_depends_on_header(&statement) {
        let alias_key = if dep_rung.starts_with("catalog:") {
            dep_rung.clone()
        } else {
            format!("catalog:{dep_rung}")
        };
        let resolved = registry
            .aliases
            .get(&alias_key)
            .or_else(|| registry.aliases.get(&dep_rung))
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "task {} depends on unpromoted rung `{dep_rung}` (missing alias `{alias_key}`)",
                    inputs.task_id
                )
            })?;
        direct_dependencies.push(resolved.clone());
    }
    direct_dependencies.sort();
    direct_dependencies.dedup();

    let bundle = resolve_bundle(registry, environment_id, &direct_dependencies)
        .map_err(|e| anyhow::anyhow!("dependency resolution failed for {}: {e}", inputs.task_id))?;
    let dependency_bundle_id = bundle_id(&bundle)
        .map_err(|e| anyhow::anyhow!("bundle_id failed for {}: {e}", inputs.task_id))?;
    let dependency_closure = bundle.modules.clone();
    let exact_imports = bundle
        .modules
        .iter()
        .map(|id| generated_module_name(id))
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| anyhow::anyhow!("generated_module_name failed for {}: {e}", inputs.task_id))?;

    let mut axioms = inputs.verification_axioms;
    for ax in &axioms {
        ensure!(
            ALLOWED_AXIOMS.contains(&ax.as_str()),
            "disallowed verification axiom `{ax}` on task {}",
            inputs.task_id
        );
    }
    axioms.dedup();

    let candidate = PromotionCandidate {
        schema: "swarm.lean-promotion-candidate/v1".to_string(),
        task_id: inputs.task_id,
        campaign_id: inputs.campaign_id,
        network: inputs.network,
        author_wallet: inputs.author_wallet,
        artifact_sha256: inputs.artifact_sha256,
        statement,
        proof_source,
        challenge: ChallengeManifest {
            schema: CHALLENGE_SCHEMA_VERSION.to_string(),
            statement_sha256: actual_stmt_sha,
            policy_version: 4,
            environment_id: environment_id.to_string(),
            dependency_bundle_id,
            reuse_license: APACHE_2_0.to_string(),
            reusable: true,
        },
        direct_dependencies,
        dependency_closure,
        exact_imports,
        verification_axioms: axioms,
        status: "verified_pending_finalization".to_string(),
    };

    compose_generated_module(&candidate).map_err(|e| {
        anyhow::anyhow!(
            "candidate {} failed compose_generated_module: {e}",
            candidate.task_id
        )
    })?;

    let alias = inputs
        .campaign_topic
        .map(|t| t.trim().to_string())
        .filter(|t| t.starts_with("catalog:") && t.len() > "catalog:".len());

    Ok((candidate, alias))
}

fn verify_catalog_statement_bytes(
    catalog_dir: &Path,
    topic: &str,
    actual_stmt_sha: &str,
) -> Result<()> {
    let Some(raw_stem) = topic.strip_prefix("catalog:") else {
        return Ok(());
    };
    let stem = raw_stem.trim();
    ensure!(
        !stem.is_empty() && !stem.contains('/') && !stem.contains('\\') && !stem.contains(".."),
        "invalid catalog topic `{topic}`"
    );
    let canonical_path = catalog_dir.join("open").join(format!("{stem}.lean"));
    ensure!(
        canonical_path.is_file(),
        "catalog statement hash mismatch: missing canonical catalog file `{}` for topic `{topic}`",
        canonical_path.display()
    );
    let canonical_bytes = std::fs::read(&canonical_path).with_context(|| {
        format!(
            "failed reading canonical catalog file `{}`",
            canonical_path.display()
        )
    })?;
    let canonical_sha = sha256_hex(&canonical_bytes);
    ensure!(
        actual_stmt_sha == canonical_sha,
        "catalog statement hash mismatch for `{topic}`: expected canonical {canonical_sha}, got {actual_stmt_sha}"
    );
    Ok(())
}

enum SingleTaskOutcome {
    Skipped,
    PreviouslyRejected,
    Ready {
        candidate: Box<PromotionCandidate>,
        alias: Option<String>,
        module_id: String,
        stem: String,
    },
}

#[allow(clippy::too_many_arguments)]
fn evaluate_single_task<T: HttpTransport>(
    transport: &T,
    args: &FetchCandidatesArgs,
    gcp_token: Option<&str>,
    registry: &RegistryManifest,
    environment_id: &str,
    batch_aliases: &BTreeMap<String, String>,
    persisted_rejected: &RejectedCandidatesManifest,
    mut task_val: Value,
    task_id: &str,
) -> Result<SingleTaskOutcome> {
    if args.tasks_file.is_none()
        && (task_val.get("lean_policy").is_none()
            || task_val.get("statement_lean").is_none()
            || extract_client_wallet(&task_val).is_none()
            || extract_campaign_topic(&task_val).is_none())
    {
        if let Some(hydrated) =
            fetch_task_detail(transport, &args.api_base, &args.network, task_id)?
        {
            merge_missing_fields(&mut task_val, hydrated);
        }
    }

    let lean_policy = json_u64(&task_val, "lean_policy");
    if lean_policy != Some(4) {
        return Ok(SingleTaskOutcome::Skipped);
    }

    let mut score = json_u64(&task_val, "composite_score");
    if score.is_none() && args.tasks_file.is_none() {
        if let Some(att) =
            fetch_task_attestation(transport, &args.api_base, &args.network, task_id)?
        {
            score = json_u64(&att, "composite_score");
            merge_missing_fields(&mut task_val, att);
        }
    }
    if score != Some(PASSING_COMPOSITE_SCORE) {
        return Ok(SingleTaskOutcome::Skipped);
    }

    let campaign_id = json_str(&task_val, "campaign_id")
        .or_else(|| task_id.split_once(':').map(|(c, _)| c.to_string()))
        .unwrap_or_default();

    if args.tasks_file.is_none()
        && !campaign_id.is_empty()
        && (extract_client_wallet(&task_val).is_none()
            || extract_campaign_topic(&task_val).is_none())
    {
        if let Some(camp) =
            fetch_campaign_detail(transport, &args.api_base, &args.network, &campaign_id)?
        {
            merge_campaign_fields(&mut task_val, camp);
        }
    }

    let client_wallet = extract_client_wallet(&task_val).unwrap_or_default();
    ensure!(
        !client_wallet.is_empty()
            && args
                .allowed_posters
                .iter()
                .any(|allowed| allowed == &client_wallet),
        "untrusted or missing poster wallet `{client_wallet}` for task {task_id}"
    );

    let author_wallet = json_str(&task_val, "agent").unwrap_or_default();
    ensure!(
        !campaign_id.is_empty() && !author_wallet.is_empty(),
        "missing campaign_id or agent wallet"
    );

    let artifact_sha256 = extract_artifact_sha256(&task_val)
        .ok_or_else(|| anyhow::anyhow!("missing artifact_sha256 on finalized policy-4 task"))?;
    ensure!(
        is_lower_hex_64(&artifact_sha256),
        "invalid artifact_sha256 `{artifact_sha256}` on task {task_id}"
    );
    let rej_key = format!("{}:{task_id}", args.network);
    if persisted_rejected
        .rejected
        .get(&rej_key)
        .is_some_and(|entry| entry.artifact_sha256 == artifact_sha256)
    {
        return Ok(SingleTaskOutcome::PreviouslyRejected);
    }

    let proof_bytes = fetch_gcs_object(
        transport,
        &args.bucket,
        &format!("lean-proofs/{artifact_sha256}.lean"),
        gcp_token,
    )
    .with_context(|| format!("failed fetching proof bytes for task {task_id}"))?;

    let actual_proof_sha = sha256_hex(&proof_bytes);
    ensure!(
        actual_proof_sha == artifact_sha256,
        "proof SHA-256 mismatch for task {task_id}: expected {artifact_sha256}, got {actual_proof_sha}"
    );

    let expected_stmt_sha = extract_statement_sha256(&task_val);
    let inline_stmt = json_str(&task_val, "statement_lean");
    let statement_bytes = match (&expected_stmt_sha, &inline_stmt) {
        (Some(stmt_sha), _) => {
            ensure!(
                is_lower_hex_64(stmt_sha),
                "invalid statement_sha256 `{stmt_sha}` on task {task_id}"
            );
            let gcs_res = fetch_gcs_object(
                transport,
                &args.bucket,
                &format!("lean-statements/{stmt_sha}.lean"),
                gcp_token,
            );
            let bytes = match (gcs_res, inline_stmt.as_ref()) {
                (Ok(b), Some(inline)) => {
                    let inline_sha = sha256_hex(inline.as_bytes());
                    ensure!(
                        inline_sha == *stmt_sha,
                        "statement SHA-256 mismatch between inline statement_lean and statement_sha256 for task {task_id}"
                    );
                    b
                }
                (Ok(b), None) => b,
                (Err(_), Some(inline)) => inline.as_bytes().to_vec(),
                (Err(err), None) => {
                    return Err(err).with_context(|| {
                        format!("failed fetching statement bytes for task {task_id}")
                    })
                }
            };
            let got_sha = sha256_hex(&bytes);
            ensure!(
                got_sha == *stmt_sha,
                "statement SHA-256 mismatch for task {task_id}: expected {stmt_sha}, got {got_sha}"
            );
            bytes
        }
        (None, Some(inline)) => inline.as_bytes().to_vec(),
        (None, None) => bail!("missing statement_sha256 and statement_lean"),
    };

    let campaign_topic = extract_campaign_topic(&task_val);
    let actual_stmt_sha = sha256_hex(&statement_bytes);
    if let Some(ref topic) = campaign_topic {
        let trimmed = topic.trim();
        if trimmed.starts_with("catalog:") {
            verify_catalog_statement_bytes(&args.catalog_dir, trimmed, &actual_stmt_sha)?;
        }
    }

    let direct_dependencies = extract_string_array(&task_val, "direct_dependencies")
        .or_else(|| extract_string_array(&task_val, "lean_dependencies"))
        .unwrap_or_default();
    let verification_axioms = extract_axioms(&task_val);

    let (candidate, alias) = validate_and_build_candidate(
        registry,
        environment_id,
        RawCandidateInputs {
            task_id: task_id.to_string(),
            campaign_id,
            network: args.network.clone(),
            author_wallet,
            campaign_topic,
            artifact_sha256,
            proof_bytes,
            expected_statement_sha256: expected_stmt_sha,
            statement_bytes,
            direct_dependencies,
            verification_axioms,
        },
    )?;

    let (preview_manifest, _) =
        preview_autopromote_manifest(&candidate, registry, DEFAULT_PUBLIC_REGISTRY_BASE_URL)
            .map_err(|e| anyhow::anyhow!("preview_autopromote_manifest failed: {e}"))?;

    if let Some(ref alias_key) = alias {
        if let Some(existing_id) = registry
            .aliases
            .get(alias_key)
            .or_else(|| batch_aliases.get(alias_key))
        {
            ensure!(
                existing_id == &preview_manifest.module_id,
                "refusing to overwrite existing alias `{alias_key}`: mapped to {existing_id}, candidate is {}",
                preview_manifest.module_id
            );
        }
    }

    let stem = sanitize_task_filename(task_id)?;
    Ok(SingleTaskOutcome::Ready {
        candidate: Box::new(candidate),
        alias,
        module_id: preview_manifest.module_id,
        stem,
    })
}

pub fn fetch_promotion_candidates_with<T: HttpTransport>(
    transport: &T,
    args: &FetchCandidatesArgs,
    gcp_token: Option<&str>,
) -> Result<FetchReport> {
    ensure!(!args.network.is_empty(), "--network must be non-empty");
    ensure!(!args.bucket.is_empty(), "--bucket must be non-empty");
    ensure!(
        !args.allowed_posters.is_empty(),
        "allowed_posters must be non-empty"
    );
    let public_swarm_proofs_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(|r| r.join("proofs").join("SwarmProofs"))
        .ok_or_else(|| {
            anyhow::anyhow!("cannot resolve proofs/SwarmProofs from CARGO_MANIFEST_DIR")
        })?;
    let effective_registry_path = if let Some(ref dry_root) = args.dry_run_registry {
        crate::promote_pipeline::assert_dry_run_isolation_for_network(
            &args.network,
            Some(dry_root),
            &dry_root.join("SwarmProofs").join("registry.json"),
            &public_swarm_proofs_dir,
        )?;
        let staged_pkg = crate::promote_pipeline::stage_dry_run_swarm_proofs_copy(
            &public_swarm_proofs_dir,
            dry_root,
        )?;
        staged_pkg.join("registry.json")
    } else {
        crate::promote_pipeline::assert_dry_run_isolation_for_network(
            &args.network,
            None,
            &args.registry_path,
            &public_swarm_proofs_dir,
        )?;
        args.registry_path.clone()
    };
    let persisted_rejected = load_rejected_candidates_manifest(args.rejected_file.as_deref())?;
    let registry_bytes = std::fs::read(&effective_registry_path).with_context(|| {
        format!(
            "failed reading registry {}",
            effective_registry_path.display()
        )
    })?;
    let registry: RegistryManifest =
        serde_json::from_slice(&registry_bytes).context("invalid registry.json")?;
    validate_registry(&registry).map_err(|e| anyhow::anyhow!("registry validation failed: {e}"))?;

    let environment_id = registry
        .environments
        .iter()
        .rfind(|env| env.lock.policy_version == 4)
        .map(|env| env.environment_id.clone())
        .ok_or_else(|| anyhow::anyhow!("registry.json contains no policy_version=4 environment"))?;

    let already_in_reg: BTreeSet<(String, String)> = registry
        .modules
        .iter()
        .map(|m| (m.originating_network.clone(), m.originating_task_id.clone()))
        .collect();

    std::fs::create_dir_all(&args.out_dir)
        .with_context(|| format!("failed creating out-dir {}", args.out_dir.display()))?;

    let mut raw_tasks = load_initial_tasks(transport, args)?;
    let mut finalized_lean: Vec<Value> =
        raw_tasks.drain(..).filter(is_finalized_lean_task).collect();

    if finalized_lean.is_empty() && args.tasks_file.is_none() && args.firestore_fallback {
        if let Some(token) = gcp_token {
            match fetch_firestore_finalized_lean_tasks(transport, args, token) {
                Ok(fallback_tasks) => finalized_lean = fallback_tasks,
                Err(err) => {
                    eprintln!(
                        "[WARN] fetch-promotion-candidates Firestore fallback error: {err:#}"
                    );
                }
            }
        }
    }

    let mut report = FetchReport::default();
    let mut plan_lines = Vec::new();
    let mut batch_aliases: BTreeMap<String, String> = BTreeMap::new();

    for task_val in finalized_lean {
        let Some(task_id) = json_str(&task_val, "task_id") else {
            continue;
        };
        if let Some(ref wanted) = args.only_task_id {
            if &task_id != wanted {
                continue;
            }
        }
        if already_in_reg.contains(&(args.network.clone(), task_id.clone())) {
            report.already_promoted += 1;
            continue;
        }
        if let Some(art_sha) = extract_artifact_sha256(&task_val) {
            let rej_key = format!("{}:{task_id}", args.network);
            if persisted_rejected
                .rejected
                .get(&rej_key)
                .is_some_and(|entry| entry.artifact_sha256 == art_sha)
            {
                report.skipped += 1;
                continue;
            }
        }

        let outcome = evaluate_single_task(
            transport,
            args,
            gcp_token,
            &registry,
            &environment_id,
            &batch_aliases,
            &persisted_rejected,
            task_val,
            &task_id,
        );

        let (candidate, alias, module_id, stem) = match outcome {
            Ok(SingleTaskOutcome::Skipped | SingleTaskOutcome::PreviouslyRejected) => {
                report.skipped += 1;
                continue;
            }
            Ok(SingleTaskOutcome::Ready {
                candidate,
                alias,
                module_id,
                stem,
            }) => (*candidate, alias, module_id, stem),
            Err(err) => {
                report.rejected.push(RejectedCandidate {
                    task_id,
                    reason: format!("{err:#}"),
                });
                continue;
            }
        };

        if let Some(ref alias_key) = alias {
            batch_aliases.insert(alias_key.clone(), module_id);
        }

        let out_file = args.out_dir.join(format!("{stem}.json"));
        let mut cand_json = serde_json::to_vec_pretty(&candidate)?;
        cand_json.push(b'\n');
        std::fs::write(&out_file, cand_json)
            .with_context(|| format!("failed writing {}", out_file.display()))?;

        let out_file_str = out_file.to_string_lossy().to_string();
        plan_lines.push(format!(
            "{out_file_str}\t{}",
            alias.as_deref().unwrap_or("")
        ));
        let artifact_sha256 = candidate.artifact_sha256.clone();
        report.candidates.push(FetchedCandidateSummary {
            task_id,
            candidate_file: out_file_str,
            alias,
            artifact_sha256,
            statement_sha256: candidate.challenge.statement_sha256,
        });
    }

    let mut plan_payload = plan_lines.join("\n");
    if !plan_payload.is_empty() {
        plan_payload.push('\n');
    }
    std::fs::write(args.out_dir.join("autopromote-plan.tsv"), plan_payload)
        .context("failed writing autopromote-plan.tsv")?;

    let mut manifest_bytes = serde_json::to_vec_pretty(&report)?;
    manifest_bytes.push(b'\n');
    std::fs::write(args.out_dir.join("manifest.json"), manifest_bytes)
        .context("failed writing manifest.json")?;

    Ok(report)
}

fn parse_tasks_json_page(raw_bytes: &[u8]) -> Result<Vec<Value>> {
    let parsed: Value = serde_json::from_slice(raw_bytes).context("invalid tasks JSON")?;
    if let Some(arr) = parsed.as_array() {
        return Ok(arr.clone());
    }
    if let Some(arr) = parsed.get("tasks").and_then(Value::as_array) {
        return Ok(arr.clone());
    }
    bail!("expected JSON array or `{{\"tasks\": [...]}}` from tasks source")
}

fn load_initial_tasks<T: HttpTransport>(
    transport: &T,
    args: &FetchCandidatesArgs,
) -> Result<Vec<Value>> {
    if let Some(path) = &args.tasks_file {
        let raw_bytes =
            std::fs::read(path).with_context(|| format!("failed reading {}", path.display()))?;
        return parse_tasks_json_page(&raw_bytes);
    }

    let mut all_tasks = Vec::new();
    let mut seen_ids = BTreeSet::new();
    let base = args.api_base.trim_end_matches('/');
    let encoded_net = percent_encode(&args.network);

    for page_idx in 0..MAX_TASK_LIST_PAGES {
        let offset = page_idx * TASK_LIST_PAGE_LIMIT;
        let url = format!(
            "{base}/tasks?state=finalized&platform={LEAN_PROOF_PLATFORM}&network={encoded_net}&limit={TASK_LIST_PAGE_LIMIT}&offset={offset}"
        );
        let raw_bytes = transport
            .send(&HttpRequest {
                method: "GET",
                url: &url,
                bearer_token: None,
                content_type: None,
                body: None,
            })
            .with_context(|| format!("failed fetching tasks from {url}"))?;
        let page_items = parse_tasks_json_page(&raw_bytes)?;
        let page_len = page_items.len();
        if page_len == 0 {
            break;
        }
        let mut new_in_page = 0usize;
        for item in page_items {
            if let Some(tid) = json_str(&item, "task_id") {
                if !seen_ids.insert(tid) {
                    continue;
                }
            }
            new_in_page += 1;
            all_tasks.push(item);
        }
        if new_in_page == 0 || page_len < TASK_LIST_PAGE_LIMIT {
            break;
        }
    }
    Ok(all_tasks)
}

fn is_finalized_lean_task(item: &Value) -> bool {
    let Some(state) = json_str(item, "state") else {
        return false;
    };
    if !state.eq_ignore_ascii_case("finalized") {
        return false;
    }
    if let Some(p) = json_u64(item, "platform") {
        return p == LEAN_PROOF_PLATFORM;
    }
    if let Some(ps) = json_str(item, "platform") {
        return ps.eq_ignore_ascii_case("leanproof") || ps.eq_ignore_ascii_case("lean_proof");
    }
    false
}

fn fetch_task_detail<T: HttpTransport>(
    transport: &T,
    api_base: &str,
    network: &str,
    task_id: &str,
) -> Result<Option<Value>> {
    let url = format!(
        "{}/tasks/{}?network={}",
        api_base.trim_end_matches('/'),
        percent_encode(task_id),
        percent_encode(network)
    );
    match transport.send(&HttpRequest {
        method: "GET",
        url: &url,
        bearer_token: None,
        content_type: None,
        body: None,
    }) {
        Ok(bytes) => Ok(Some(
            serde_json::from_slice(&bytes).context("invalid task detail JSON")?,
        )),
        Err(HttpFailure::Status { status: 404, .. }) => Ok(None),
        Err(err) => Err(err.into()),
    }
}

fn fetch_campaign_detail<T: HttpTransport>(
    transport: &T,
    api_base: &str,
    network: &str,
    campaign_id: &str,
) -> Result<Option<Value>> {
    let url = format!(
        "{}/campaigns/{}?network={}",
        api_base.trim_end_matches('/'),
        percent_encode(campaign_id),
        percent_encode(network)
    );
    match transport.send(&HttpRequest {
        method: "GET",
        url: &url,
        bearer_token: None,
        content_type: None,
        body: None,
    }) {
        Ok(bytes) => Ok(Some(
            serde_json::from_slice(&bytes).context("invalid campaign detail JSON")?,
        )),
        Err(HttpFailure::Status { status: 404, .. }) => Ok(None),
        Err(err) => Err(err.into()),
    }
}

fn fetch_task_attestation<T: HttpTransport>(
    transport: &T,
    api_base: &str,
    network: &str,
    task_id: &str,
) -> Result<Option<Value>> {
    let url = format!(
        "{}/tasks/{}/attestation?network={}",
        api_base.trim_end_matches('/'),
        percent_encode(task_id),
        percent_encode(network)
    );
    match transport.send(&HttpRequest {
        method: "GET",
        url: &url,
        bearer_token: None,
        content_type: None,
        body: None,
    }) {
        Ok(bytes) => Ok(Some(
            serde_json::from_slice(&bytes).context("invalid attestation JSON")?,
        )),
        Err(HttpFailure::Status {
            status: 404 | 409, ..
        }) => Ok(None),
        Err(err) => Err(err.into()),
    }
}

fn fetch_gcs_object<T: HttpTransport>(
    transport: &T,
    bucket: &str,
    object_name: &str,
    gcp_token: Option<&str>,
) -> Result<Vec<u8>> {
    let url = format!(
        "https://storage.googleapis.com/storage/v1/b/{}/o/{}?alt=media",
        percent_encode(bucket),
        percent_encode(object_name)
    );
    let bytes = transport.send(&HttpRequest {
        method: "GET",
        url: &url,
        bearer_token: gcp_token,
        content_type: None,
        body: None,
    })?;
    Ok(bytes)
}

fn fetch_firestore_finalized_lean_tasks<T: HttpTransport>(
    transport: &T,
    args: &FetchCandidatesArgs,
    gcp_token: &str,
) -> Result<Vec<Value>> {
    let encoded_project = percent_encode(&args.project);
    let mut results = Vec::new();
    let mut page_token: Option<String> = None;

    for _ in 0..MAX_FIRESTORE_SCAN_PAGES {
        let mut url = format!(
            "https://firestore.googleapis.com/v1/projects/{encoded_project}/databases/(default)/documents/tasks?pageSize=300"
        );
        if let Some(tok) = &page_token {
            url.push_str("&pageToken=");
            url.push_str(&percent_encode(tok));
        }
        let body = match transport.send(&HttpRequest {
            method: "GET",
            url: &url,
            bearer_token: Some(gcp_token),
            content_type: None,
            body: None,
        }) {
            Ok(bytes) => bytes,
            Err(err) => {
                eprintln!("[WARN] fetch-promotion-candidates Firestore fallback error: {err:#}");
                return Ok(results);
            }
        };
        let page: Value =
            serde_json::from_slice(&body).context("invalid Firestore tasks list JSON")?;
        if let Some(docs) = page.get("documents").and_then(Value::as_array) {
            for doc in docs {
                if let Some(mut item) = parse_firestore_task_doc(doc, &args.network) {
                    if let Some(tid) = json_str(&item, "task_id") {
                        match fetch_firestore_verification_doc(
                            transport,
                            &args.project,
                            &tid,
                            gcp_token,
                        ) {
                            Ok(Some(verif)) => merge_missing_fields(&mut item, verif),
                            Ok(None) => {}
                            Err(err) => {
                                eprintln!(
                                    "[WARN] fetch-promotion-candidates Firestore fallback error: {err:#}"
                                );
                            }
                        }
                    }
                    results.push(item);
                }
            }
        }
        match page.get("nextPageToken").and_then(Value::as_str) {
            Some(next) if !next.is_empty() => page_token = Some(next.to_string()),
            _ => break,
        }
    }
    Ok(results)
}

fn fetch_firestore_verification_doc<T: HttpTransport>(
    transport: &T,
    project: &str,
    task_id: &str,
    gcp_token: &str,
) -> Result<Option<Value>> {
    let url = format!(
        "https://firestore.googleapis.com/v1/projects/{}/databases/(default)/documents/verifications/{}",
        percent_encode(project),
        percent_encode(task_id)
    );
    let bytes = match transport.send(&HttpRequest {
        method: "GET",
        url: &url,
        bearer_token: Some(gcp_token),
        content_type: None,
        body: None,
    }) {
        Ok(b) => b,
        Err(HttpFailure::Status { status: 404, .. }) => return Ok(None),
        Err(err) => {
            eprintln!("[WARN] fetch-promotion-candidates Firestore fallback error: {err:#}");
            return Ok(None);
        }
    };
    let doc: Value = serde_json::from_slice(&bytes).context("invalid verification doc JSON")?;
    let Some(fields) = doc.get("fields") else {
        return Ok(None);
    };
    let mut map = serde_json::Map::new();
    if let Some(sha) = fs_str(fields, "artifact_sha256").or_else(|| fs_str(fields, "artifact_hash"))
    {
        map.insert("artifact_sha256".into(), Value::String(sha));
    }
    if let Some(ssha) = fs_str(fields, "statement_sha256") {
        map.insert("statement_sha256".into(), Value::String(ssha));
    }
    if let Some(score) = fs_u64(fields, "composite_score").or_else(|| fs_u64(fields, "score")) {
        map.insert("composite_score".into(), Value::Number(score.into()));
    }
    if let Some(axioms) = fs_str_array(fields, "axioms") {
        map.insert(
            "axioms".into(),
            Value::Array(axioms.into_iter().map(Value::String).collect()),
        );
    }
    Ok(Some(Value::Object(map)))
}

fn parse_firestore_task_doc(doc: &Value, expected_network: &str) -> Option<Value> {
    let doc_name = doc.get("name")?.as_str()?;
    let task_id = doc_name.rsplit('/').next()?.to_string();
    let fields = doc.get("fields")?;
    let network = fs_str(fields, "network")?;
    let state = fs_str(fields, "state")?;
    if network != expected_network || !state.eq_ignore_ascii_case("finalized") {
        return None;
    }
    let is_lean = fs_u64(fields, "platform") == Some(LEAN_PROOF_PLATFORM)
        || fs_str(fields, "platform").is_some_and(|p| {
            p == "10" || p.eq_ignore_ascii_case("leanproof") || p.eq_ignore_ascii_case("lean_proof")
        });
    if !is_lean {
        return None;
    }
    let mut obj = serde_json::Map::new();
    obj.insert("task_id".into(), Value::String(task_id));
    obj.insert("state".into(), Value::String("finalized".into()));
    obj.insert("platform".into(), Value::Number(LEAN_PROOF_PLATFORM.into()));
    if let Some(cid) = fs_str(fields, "campaign_id") {
        obj.insert("campaign_id".into(), Value::String(cid));
    }
    if let Some(client) = fs_str(fields, "client") {
        obj.insert("client".into(), Value::String(client));
    }
    if let Some(agent) = fs_str(fields, "agent") {
        obj.insert("agent".into(), Value::String(agent));
    }
    if let Some(topic) = fs_str(fields, "campaign_topic") {
        obj.insert("campaign_topic".into(), Value::String(topic));
    }
    if let Some(content_id) = fs_str(fields, "content_id") {
        obj.insert("content_id".into(), Value::String(content_id));
    }
    if let Some(pol) = fs_u64(fields, "lean_policy") {
        obj.insert("lean_policy".into(), Value::Number(pol.into()));
    }
    if let Some(score) = fs_u64(fields, "composite_score") {
        obj.insert("composite_score".into(), Value::Number(score.into()));
    }
    if let Some(asha) = fs_str(fields, "artifact_sha256") {
        obj.insert("artifact_sha256".into(), Value::String(asha));
    }
    if let Some(ssha) = fs_str(fields, "statement_sha256") {
        obj.insert("statement_sha256".into(), Value::String(ssha));
    }
    if let Some(stmt) = fs_str(fields, "statement_lean") {
        obj.insert("statement_lean".into(), Value::String(stmt));
    }
    Some(Value::Object(obj))
}

fn fs_str(fields: &Value, key: &str) -> Option<String> {
    fields
        .get(key)?
        .get("stringValue")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn fs_u64(fields: &Value, key: &str) -> Option<u64> {
    let field = fields.get(key)?;
    if let Some(s) = field.get("integerValue").and_then(Value::as_str) {
        return s.parse::<u64>().ok();
    }
    field.get("integerValue").and_then(Value::as_u64)
}

fn fs_str_array(fields: &Value, key: &str) -> Option<Vec<String>> {
    let values = fields
        .get(key)?
        .get("arrayValue")?
        .get("values")?
        .as_array()?;
    let mut out = Vec::with_capacity(values.len());
    for v in values {
        if let Some(s) = v.get("stringValue").and_then(Value::as_str) {
            out.push(s.to_string());
        }
    }
    Some(out)
}

fn merge_missing_fields(target: &mut Value, extra: Value) {
    let (Some(dst), Some(src)) = (target.as_object_mut(), extra.as_object()) else {
        return;
    };
    for (k, v) in src {
        let should_fill = matches!(dst.get(k), None | Some(Value::Null));
        if should_fill && !v.is_null() {
            dst.insert(k.clone(), v.clone());
        }
    }
}

fn merge_campaign_fields(target: &mut Value, campaign: Value) {
    if let Some(client) = json_str(&campaign, "client") {
        if extract_client_wallet(target).is_none() {
            if let Some(dst) = target.as_object_mut() {
                dst.insert("client".into(), Value::String(client));
            }
        }
    }
    if let Some(topic) = extract_campaign_topic(&campaign) {
        if extract_campaign_topic(target).is_none() {
            if let Some(dst) = target.as_object_mut() {
                dst.insert("campaign_topic".into(), Value::String(topic));
            }
        }
    }
    merge_missing_fields(target, campaign);
}

fn extract_client_wallet(val: &Value) -> Option<String> {
    json_str(val, "client").or_else(|| val.get("campaign").and_then(|c| json_str(c, "client")))
}

fn extract_campaign_topic(val: &Value) -> Option<String> {
    json_str(val, "campaign_topic")
        .or_else(|| val.get("brief").and_then(|b| json_str(b, "topic")))
        .or_else(|| {
            val.get("campaign")
                .and_then(|c| json_str(c, "campaign_topic"))
        })
        .or_else(|| {
            val.get("campaign")
                .and_then(|c| c.get("brief"))
                .and_then(|b| json_str(b, "topic"))
        })
}

fn json_str(val: &Value, key: &str) -> Option<String> {
    val.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

fn json_u64(val: &Value, key: &str) -> Option<u64> {
    let field = val.get(key)?;
    if let Some(n) = field.as_u64() {
        return Some(n);
    }
    field.as_str().and_then(|s| s.trim().parse::<u64>().ok())
}

fn extract_string_array(val: &Value, key: &str) -> Option<Vec<String>> {
    let arr = val.get(key)?.as_array()?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        if let Some(s) = item.as_str() {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                out.push(trimmed.to_string());
            }
        }
    }
    Some(out)
}

fn extract_artifact_sha256(val: &Value) -> Option<String> {
    if let Some(direct) = json_str(val, "artifact_sha256") {
        return Some(direct);
    }
    if let Some(ext) = val.get("extensions") {
        if let Some(s) = json_str(ext, "artifact_sha256") {
            return Some(s);
        }
        if let Some(lean_ext) = ext.get("lean") {
            if let Some(s) = json_str(lean_ext, "artifact_sha256") {
                return Some(s);
            }
        }
    }
    if let Some(cid) = json_str(val, "content_id") {
        if let Some((_, tail)) = cid.rsplit_once("/lean-proofs/") {
            if let Some(stem) = tail.strip_suffix(".lean") {
                if is_lower_hex_64(stem) {
                    return Some(stem.to_string());
                }
            }
        }
    }
    None
}

fn extract_statement_sha256(val: &Value) -> Option<String> {
    if let Some(direct) = json_str(val, "statement_sha256") {
        return Some(direct);
    }
    if let Some(ext) = val.get("extensions") {
        if let Some(s) = json_str(ext, "statement_sha256") {
            return Some(s);
        }
        if let Some(lean_ext) = ext.get("lean") {
            if let Some(s) = json_str(lean_ext, "statement_sha256") {
                return Some(s);
            }
        }
    }
    None
}

fn extract_axioms(val: &Value) -> Vec<String> {
    if let Some(arr) = extract_string_array(val, "axioms")
        .or_else(|| extract_string_array(val, "verification_axioms"))
        .or_else(|| {
            val.get("extensions")
                .and_then(|e| extract_string_array(e, "axioms"))
        })
    {
        return arr;
    }
    ALLOWED_AXIOMS.iter().map(|s| (*s).to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proof_registry::autopromote_candidate;
    use serde_json::json;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::path::Path;

    struct MapTransport {
        routes: HashMap<String, std::result::Result<Vec<u8>, HttpFailure>>,
        calls: RefCell<Vec<String>>,
    }

    impl MapTransport {
        fn new() -> Self {
            Self {
                routes: HashMap::new(),
                calls: RefCell::new(Vec::new()),
            }
        }

        fn insert_ok(&mut self, url: impl Into<String>, body: impl Into<Vec<u8>>) {
            self.routes.insert(url.into(), Ok(body.into()));
        }
    }

    impl HttpTransport for MapTransport {
        fn send(&self, request: &HttpRequest<'_>) -> std::result::Result<Vec<u8>, HttpFailure> {
            self.calls.borrow_mut().push(request.url.to_string());
            self.routes.get(request.url).cloned().unwrap_or_else(|| {
                Err(HttpFailure::Status {
                    status: 404,
                    body: format!("unmocked URL: {}", request.url).into_bytes(),
                })
            })
        }
    }

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .unwrap()
            .to_path_buf()
    }

    fn default_args(
        reg_path: PathBuf,
        out_dir: PathBuf,
        tasks_file: Option<PathBuf>,
    ) -> FetchCandidatesArgs {
        FetchCandidatesArgs {
            api_base: DEFAULT_SHILLBOT_API.to_string(),
            network: "mainnet".to_string(),
            bucket: DEFAULT_ARTIFACT_BUCKET.to_string(),
            project: DEFAULT_GCP_PROJECT.to_string(),
            registry_path: reg_path,
            catalog_dir: repo_root().join("tests/fixtures/lean/catalog"),
            allowed_posters: PINNED_POSTER_WALLETS
                .iter()
                .map(|w| (*w).to_string())
                .collect(),
            out_dir,
            tasks_file,
            firestore_fallback: false,
            rejected_file: None,
            dry_run_registry: None,
            only_task_id: None,
        }
    }

    fn setup_temp_workspace(label: &str) -> (PathBuf, PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "xtask-fetch-promo-{label}-{}",
            uuid::Uuid::new_v4()
        ));
        let pkg_dir = dir.join("SwarmProofs");
        let out_dir = dir.join("out");
        std::fs::create_dir_all(&pkg_dir).unwrap();
        std::fs::create_dir_all(&out_dir).unwrap();
        let real_reg = repo_root().join("proofs/SwarmProofs/registry.json");
        let reg_dst = pkg_dir.join("registry.json");
        std::fs::copy(&real_reg, &reg_dst).unwrap();
        (dir, pkg_dir, out_dir)
    }

    fn gcs_media_url(bucket: &str, object_name: &str) -> String {
        format!(
            "https://storage.googleapis.com/storage/v1/b/{}/o/{}?alt=media",
            percent_encode(bucket),
            percent_encode(object_name)
        )
    }

    #[test]
    fn golden_api_and_gcs_fixture_autopromotes_and_validates() {
        let (dir, pkg_dir, out_dir) = setup_temp_workspace("golden");
        let reg_path = pkg_dir.join("registry.json");
        let statement = std::fs::read(
            repo_root()
                .join("tests/fixtures/lean/catalog/open/chain-bsd-1-weierstrass-bijection.lean"),
        )
        .unwrap();
        let proof =
            b"-- SPDX-License-Identifier: Apache-2.0\ntheorem proof : statementProp := rfl\n";
        let stmt_sha = sha256_hex(&statement);
        let proof_sha = sha256_hex(proof);

        let api_url =
            "https://api.shillbot.org/tasks?state=finalized&platform=10&network=mainnet&limit=100&offset=0";
        let mut transport = MapTransport::new();
        transport.insert_ok(
            api_url,
            serde_json::to_vec(&json!({
                "tasks": [{
                    "task_id": "camp-golden:task-golden-1",
                    "campaign_id": "camp-golden",
                    "client": PINNED_POSTER_WALLETS[0],
                    "campaign_topic": "catalog:chain-bsd-1-weierstrass-bijection",
                    "state": "finalized",
                    "platform": 10,
                    "lean_policy": 4,
                    "composite_score": 1_000_000,
                    "agent": "AgentWallet11111111111111111111111111111111",
                    "artifact_sha256": proof_sha,
                    "statement_sha256": stmt_sha,
                    "axioms": ["propext"]
                }]
            }))
            .unwrap(),
        );
        transport.insert_ok(
            gcs_media_url(
                DEFAULT_ARTIFACT_BUCKET,
                &format!("lean-proofs/{proof_sha}.lean"),
            ),
            proof.to_vec(),
        );
        transport.insert_ok(
            gcs_media_url(
                DEFAULT_ARTIFACT_BUCKET,
                &format!("lean-statements/{stmt_sha}.lean"),
            ),
            statement,
        );

        let args = default_args(reg_path.clone(), out_dir, None);
        let report = fetch_promotion_candidates_with(&transport, &args, Some("tok")).unwrap();
        assert_eq!(report.candidates.len(), 1);
        assert_eq!(report.skipped, 0);
        assert!(report.rejected.is_empty());
        assert_eq!(
            report.candidates[0].alias.as_deref(),
            Some("catalog:chain-bsd-1-weierstrass-bijection")
        );

        let cand: PromotionCandidate =
            serde_json::from_slice(&std::fs::read(&report.candidates[0].candidate_file).unwrap())
                .unwrap();
        let mut reg: RegistryManifest =
            serde_json::from_slice(&std::fs::read(&reg_path).unwrap()).unwrap();
        let manifest =
            autopromote_candidate(&cand, &mut reg, &pkg_dir, DEFAULT_PUBLIC_REGISTRY_BASE_URL)
                .unwrap();
        reg.aliases.insert(
            report.candidates[0].alias.clone().unwrap(),
            manifest.module_id.clone(),
        );
        validate_registry(&reg).unwrap();
        assert_eq!(reg.modules.len(), 1);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn golden_chain_bsd_2_statement_with_promoted_rung_1_autopromotes() {
        let (dir, pkg_dir, out_dir) = setup_temp_workspace("chain-bsd-2");
        let reg_path = pkg_dir.join("registry.json");

        let b1_stmt = std::fs::read_to_string(
            repo_root()
                .join("tests/fixtures/lean/catalog/open/chain-bsd-1-weierstrass-bijection.lean"),
        )
        .unwrap();
        let b1_proof =
            "-- SPDX-License-Identifier: Apache-2.0\ntheorem proof : statementProp := by exact True.intro\n";
        let mut reg: RegistryManifest =
            serde_json::from_slice(&std::fs::read(&reg_path).unwrap()).unwrap();
        let env_id = reg.environments[0].environment_id.clone();
        let (b1_cand, b1_alias) = validate_and_build_candidate(
            &reg,
            &env_id,
            RawCandidateInputs {
                task_id: "camp-b1:task-b1".into(),
                campaign_id: "camp-b1".into(),
                network: "mainnet".into(),
                author_wallet: "SolverB1".into(),
                campaign_topic: Some("catalog:chain-bsd-1-weierstrass-bijection".into()),
                artifact_sha256: sha256_hex(b1_proof.as_bytes()),
                proof_bytes: b1_proof.as_bytes().to_vec(),
                expected_statement_sha256: Some(sha256_hex(b1_stmt.as_bytes())),
                statement_bytes: b1_stmt.as_bytes().to_vec(),
                direct_dependencies: Vec::new(),
                verification_axioms: vec!["propext".into(), "Classical.choice".into()],
            },
        )
        .unwrap();
        let b1_manifest = autopromote_candidate(
            &b1_cand,
            &mut reg,
            &pkg_dir,
            DEFAULT_PUBLIC_REGISTRY_BASE_URL,
        )
        .unwrap();
        reg.aliases
            .insert(b1_alias.unwrap(), b1_manifest.module_id.clone());
        validate_registry(&reg).unwrap();
        std::fs::write(&reg_path, serde_json::to_vec_pretty(&reg).unwrap()).unwrap();

        let b2_stmt = std::fs::read(
            repo_root()
                .join("tests/fixtures/lean/catalog/open/chain-bsd-2-fermat-noncongruent-1.lean"),
        )
        .unwrap();
        let b2_proof =
            b"-- SPDX-License-Identifier: Apache-2.0\ntheorem proof : statementProp := by intro h; exact False.elim (by trivial)\n";
        let b2_stmt_sha = sha256_hex(&b2_stmt);
        let b2_proof_sha = sha256_hex(b2_proof);

        let tasks_json_path = dir.join("tasks.json");
        std::fs::write(
            &tasks_json_path,
            serde_json::to_vec(&json!({
                "tasks": [{
                    "task_id": "camp-b2:task-b2",
                    "campaign_id": "camp-b2",
                    "client": PINNED_POSTER_WALLETS[0],
                    "campaign_topic": "catalog:chain-bsd-2-fermat-noncongruent-1",
                    "state": "finalized",
                    "platform": 10,
                    "lean_policy": 4,
                    "composite_score": 1_000_000,
                    "agent": "SolverB2",
                    "artifact_sha256": b2_proof_sha,
                    "statement_sha256": b2_stmt_sha,
                    "axioms": ["propext", "Classical.choice"]
                }]
            }))
            .unwrap(),
        )
        .unwrap();

        let mut transport = MapTransport::new();
        transport.insert_ok(
            gcs_media_url(
                DEFAULT_ARTIFACT_BUCKET,
                &format!("lean-proofs/{b2_proof_sha}.lean"),
            ),
            b2_proof.to_vec(),
        );
        transport.insert_ok(
            gcs_media_url(
                DEFAULT_ARTIFACT_BUCKET,
                &format!("lean-statements/{b2_stmt_sha}.lean"),
            ),
            b2_stmt,
        );

        let args = default_args(reg_path, out_dir, Some(tasks_json_path));
        let report = fetch_promotion_candidates_with(&transport, &args, Some("tok")).unwrap();
        assert_eq!(report.candidates.len(), 1);
        let b2_cand: PromotionCandidate =
            serde_json::from_slice(&std::fs::read(&report.candidates[0].candidate_file).unwrap())
                .unwrap();
        assert_eq!(b2_cand.direct_dependencies, vec![b1_manifest.module_id]);

        let b2_manifest = autopromote_candidate(
            &b2_cand,
            &mut reg,
            &pkg_dir,
            DEFAULT_PUBLIC_REGISTRY_BASE_URL,
        )
        .unwrap();
        let generated_src = std::fs::read_to_string(pkg_dir.join(format!(
            "{}.lean",
            b2_manifest.module_name.replace('.', "/")
        )))
        .unwrap();
        assert!(generated_src.contains("import SwarmProofs.Chains.BSD.Defs.V1"));
        assert!(generated_src.contains(&format!("import {}", b1_manifest.module_name)));
        validate_registry(&reg).unwrap();

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn every_open_chain_statement_passes_compose_generated_module() {
        let open_dir = repo_root().join("tests/fixtures/lean/catalog/open");
        let reg_bytes =
            std::fs::read(repo_root().join("proofs/SwarmProofs/registry.json")).unwrap();
        let reg: RegistryManifest = serde_json::from_slice(&reg_bytes).unwrap();
        let env_id = reg.environments[0].environment_id.clone();
        let mock_proof =
            "-- SPDX-License-Identifier: Apache-2.0\ntheorem proof : statementProp := trivial\n";
        let empty_bundle = resolve_bundle(&reg, &env_id, &[]).unwrap();
        let empty_bundle_id = bundle_id(&empty_bundle).unwrap();

        let mut count = 0usize;
        for entry in std::fs::read_dir(&open_dir).unwrap() {
            let path = entry.unwrap().path();
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if !name.starts_with("chain-") || !name.ends_with(".lean") {
                continue;
            }
            let stmt = std::fs::read_to_string(&path).unwrap();
            let cand = PromotionCandidate {
                schema: "swarm.lean-promotion-candidate/v1".into(),
                task_id: format!("task-{name}"),
                campaign_id: "camp".into(),
                network: "mainnet".into(),
                author_wallet: "wallet".into(),
                artifact_sha256: sha256_hex(mock_proof.as_bytes()),
                statement: stmt.clone(),
                proof_source: mock_proof.into(),
                challenge: ChallengeManifest {
                    schema: CHALLENGE_SCHEMA_VERSION.into(),
                    statement_sha256: sha256_hex(stmt.as_bytes()),
                    policy_version: 4,
                    environment_id: env_id.clone(),
                    dependency_bundle_id: empty_bundle_id.clone(),
                    reuse_license: APACHE_2_0.into(),
                    reusable: true,
                },
                direct_dependencies: Vec::new(),
                dependency_closure: Vec::new(),
                exact_imports: Vec::new(),
                verification_axioms: vec!["propext".into()],
                status: "verified_pending_finalization".into(),
            };
            let generated = compose_generated_module(&cand)
                .unwrap_or_else(|e| panic!("{name} failed compose_generated_module: {e}"));
            assert!(
                generated.source.contains(".Defs.V1"),
                "{name} must import a versioned SwarmProofs.Chains.*.Defs.V1 module"
            );
            count += 1;
        }
        assert_eq!(count, 18, "expected all 18 open chain-*.lean statements");
    }

    #[test]
    fn proof_sha256_mismatch_is_rejected() {
        let (dir, pkg_dir, out_dir) = setup_temp_workspace("bad-proof-sha");
        let reg_path = pkg_dir.join("registry.json");
        let statement = b"def statementProp : Prop := True\n";
        let expected_proof =
            b"-- SPDX-License-Identifier: Apache-2.0\ntheorem proof : statementProp := trivial\n";
        let tampered_proof =
            b"-- SPDX-License-Identifier: Apache-2.0\ntheorem proof : statementProp := True.intro\n";
        let proof_sha = sha256_hex(expected_proof);
        let stmt_sha = sha256_hex(statement);

        let reg: RegistryManifest =
            serde_json::from_slice(&std::fs::read(&reg_path).unwrap()).unwrap();
        let direct_err = validate_and_build_candidate(
            &reg,
            &reg.environments[0].environment_id,
            RawCandidateInputs {
                task_id: "camp:bad-proof".into(),
                campaign_id: "camp".into(),
                network: "mainnet".into(),
                author_wallet: "Wallet1".into(),
                campaign_topic: None,
                artifact_sha256: proof_sha.clone(),
                proof_bytes: tampered_proof.to_vec(),
                expected_statement_sha256: Some(stmt_sha.clone()),
                statement_bytes: statement.to_vec(),
                direct_dependencies: Vec::new(),
                verification_axioms: vec!["propext".into()],
            },
        )
        .unwrap_err()
        .to_string();
        assert!(direct_err.contains("proof SHA-256 mismatch"));

        let tasks_path = dir.join("tasks.json");
        std::fs::write(
            &tasks_path,
            serde_json::to_vec(&json!([{
                "task_id": "camp:bad-proof",
                "campaign_id": "camp",
                "client": PINNED_POSTER_WALLETS[0],
                "state": "finalized",
                "platform": 10,
                "lean_policy": 4,
                "composite_score": 1_000_000,
                "agent": "Wallet1",
                "artifact_sha256": proof_sha,
                "statement_sha256": stmt_sha
            }]))
            .unwrap(),
        )
        .unwrap();

        let mut transport = MapTransport::new();
        transport.insert_ok(
            gcs_media_url(
                DEFAULT_ARTIFACT_BUCKET,
                &format!("lean-proofs/{proof_sha}.lean"),
            ),
            tampered_proof.to_vec(),
        );
        transport.insert_ok(
            gcs_media_url(
                DEFAULT_ARTIFACT_BUCKET,
                &format!("lean-statements/{stmt_sha}.lean"),
            ),
            statement.to_vec(),
        );

        let report = fetch_promotion_candidates_with(
            &transport,
            &default_args(reg_path, out_dir, Some(tasks_path)),
            Some("tok"),
        )
        .unwrap();
        assert!(report.candidates.is_empty());
        assert_eq!(report.rejected.len(), 1);
        assert!(
            report.rejected[0].reason.contains("proof SHA-256 mismatch"),
            "unexpected reason: {}",
            report.rejected[0].reason
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn statement_sha256_mismatch_is_rejected() {
        let (dir, pkg_dir, out_dir) = setup_temp_workspace("bad-stmt-sha");
        let reg_path = pkg_dir.join("registry.json");
        let expected_stmt = b"def statementProp : Prop := True\n";
        let tampered_stmt = b"def statementProp : Prop := False\n";
        let proof =
            b"-- SPDX-License-Identifier: Apache-2.0\ntheorem proof : statementProp := trivial\n";
        let proof_sha = sha256_hex(proof);
        let stmt_sha = sha256_hex(expected_stmt);

        let reg: RegistryManifest =
            serde_json::from_slice(&std::fs::read(&reg_path).unwrap()).unwrap();
        let direct_err = validate_and_build_candidate(
            &reg,
            &reg.environments[0].environment_id,
            RawCandidateInputs {
                task_id: "camp:bad-stmt".into(),
                campaign_id: "camp".into(),
                network: "mainnet".into(),
                author_wallet: "Wallet1".into(),
                campaign_topic: None,
                artifact_sha256: proof_sha.clone(),
                proof_bytes: proof.to_vec(),
                expected_statement_sha256: Some(stmt_sha.clone()),
                statement_bytes: tampered_stmt.to_vec(),
                direct_dependencies: Vec::new(),
                verification_axioms: vec!["propext".into()],
            },
        )
        .unwrap_err()
        .to_string();
        assert!(direct_err.contains("statement SHA-256 mismatch"));

        let tasks_path = dir.join("tasks.json");
        std::fs::write(
            &tasks_path,
            serde_json::to_vec(&json!([{
                "task_id": "camp:bad-stmt",
                "campaign_id": "camp",
                "client": PINNED_POSTER_WALLETS[0],
                "state": "finalized",
                "platform": 10,
                "lean_policy": 4,
                "composite_score": 1_000_000,
                "agent": "Wallet1",
                "artifact_sha256": proof_sha,
                "statement_sha256": stmt_sha
            }]))
            .unwrap(),
        )
        .unwrap();

        let mut transport = MapTransport::new();
        transport.insert_ok(
            gcs_media_url(
                DEFAULT_ARTIFACT_BUCKET,
                &format!("lean-proofs/{proof_sha}.lean"),
            ),
            proof.to_vec(),
        );
        transport.insert_ok(
            gcs_media_url(
                DEFAULT_ARTIFACT_BUCKET,
                &format!("lean-statements/{stmt_sha}.lean"),
            ),
            tampered_stmt.to_vec(),
        );

        let report = fetch_promotion_candidates_with(
            &transport,
            &default_args(reg_path, out_dir, Some(tasks_path)),
            Some("tok"),
        )
        .unwrap();
        assert!(report.candidates.is_empty());
        assert_eq!(report.rejected.len(), 1);
        assert!(
            report.rejected[0]
                .reason
                .contains("statement SHA-256 mismatch"),
            "unexpected reason: {}",
            report.rejected[0].reason
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_spdx_header_is_rejected() {
        let (dir, pkg_dir, out_dir) = setup_temp_workspace("no-spdx");
        let reg_path = pkg_dir.join("registry.json");
        let statement = b"def statementProp : Prop := True\n";
        let unlicensed_proof = b"theorem proof : statementProp := trivial\n";
        let proof_sha = sha256_hex(unlicensed_proof);
        let stmt_sha = sha256_hex(statement);

        let reg: RegistryManifest =
            serde_json::from_slice(&std::fs::read(&reg_path).unwrap()).unwrap();
        let direct_err = validate_and_build_candidate(
            &reg,
            &reg.environments[0].environment_id,
            RawCandidateInputs {
                task_id: "camp:no-spdx".into(),
                campaign_id: "camp".into(),
                network: "mainnet".into(),
                author_wallet: "Wallet1".into(),
                campaign_topic: None,
                artifact_sha256: proof_sha.clone(),
                proof_bytes: unlicensed_proof.to_vec(),
                expected_statement_sha256: Some(stmt_sha.clone()),
                statement_bytes: statement.to_vec(),
                direct_dependencies: Vec::new(),
                verification_axioms: vec!["propext".into()],
            },
        )
        .unwrap_err()
        .to_string();
        assert!(direct_err.contains("SPDX-License-Identifier"));

        let tasks_path = dir.join("tasks.json");
        std::fs::write(
            &tasks_path,
            serde_json::to_vec(&json!([{
                "task_id": "camp:no-spdx",
                "campaign_id": "camp",
                "client": PINNED_POSTER_WALLETS[0],
                "state": "finalized",
                "platform": 10,
                "lean_policy": 4,
                "composite_score": 1_000_000,
                "agent": "Wallet1",
                "artifact_sha256": proof_sha,
                "statement_sha256": stmt_sha
            }]))
            .unwrap(),
        )
        .unwrap();

        let mut transport = MapTransport::new();
        transport.insert_ok(
            gcs_media_url(
                DEFAULT_ARTIFACT_BUCKET,
                &format!("lean-proofs/{proof_sha}.lean"),
            ),
            unlicensed_proof.to_vec(),
        );
        transport.insert_ok(
            gcs_media_url(
                DEFAULT_ARTIFACT_BUCKET,
                &format!("lean-statements/{stmt_sha}.lean"),
            ),
            statement.to_vec(),
        );

        let report = fetch_promotion_candidates_with(
            &transport,
            &default_args(reg_path, out_dir, Some(tasks_path)),
            Some("tok"),
        )
        .unwrap();
        assert!(report.candidates.is_empty());
        assert_eq!(report.rejected.len(), 1);
        assert!(report.rejected[0]
            .reason
            .contains("SPDX-License-Identifier"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn pre_policy_4_tasks_are_skipped() {
        let (dir, pkg_dir, out_dir) = setup_temp_workspace("pre-policy-4");
        let reg_path = pkg_dir.join("registry.json");

        for policy in [json!(2), json!(3), json!(null)] {
            let tasks_path = dir.join("tasks.json");
            std::fs::write(
                &tasks_path,
                serde_json::to_vec(&json!([{
                    "task_id": "camp:warmup-1",
                    "campaign_id": "camp",
                    "client": PINNED_POSTER_WALLETS[0],
                    "state": "finalized",
                    "platform": 10,
                    "lean_policy": policy,
                    "composite_score": 1_000_000,
                    "agent": "WalletWarmup",
                    "statement_lean": "def statementProp : Prop := True"
                }]))
                .unwrap(),
            )
            .unwrap();

            let transport = MapTransport::new();
            let report = fetch_promotion_candidates_with(
                &transport,
                &default_args(reg_path.clone(), out_dir.clone(), Some(tasks_path)),
                None,
            )
            .unwrap();
            assert!(report.candidates.is_empty());
            assert!(report.rejected.is_empty());
            assert_eq!(report.skipped, 1);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn foreign_poster_wallet_is_rejected() {
        let (dir, pkg_dir, out_dir) = setup_temp_workspace("foreign-poster");
        let reg_path = pkg_dir.join("registry.json");
        let statement = std::fs::read(
            repo_root()
                .join("tests/fixtures/lean/catalog/open/chain-bsd-1-weierstrass-bijection.lean"),
        )
        .unwrap();
        let proof =
            b"-- SPDX-License-Identifier: Apache-2.0\ntheorem proof : statementProp := rfl\n";
        let stmt_sha = sha256_hex(&statement);
        let proof_sha = sha256_hex(proof);

        let tasks_path = dir.join("tasks.json");
        std::fs::write(
            &tasks_path,
            serde_json::to_vec(&json!([
                {
                    "task_id": "camp-attacker:task-1",
                    "campaign_id": "camp-attacker",
                    "client": "AttackerWallet999999999999999999999999999999",
                    "campaign_topic": "catalog:chain-bsd-1-weierstrass-bijection",
                    "state": "finalized",
                    "platform": 10,
                    "lean_policy": 4,
                    "composite_score": 1_000_000,
                    "agent": "AttackerSolver",
                    "artifact_sha256": proof_sha,
                    "statement_sha256": stmt_sha
                },
                {
                    "task_id": "camp-missing-client:task-2",
                    "campaign_id": "camp-missing-client",
                    "state": "finalized",
                    "platform": 10,
                    "lean_policy": 4,
                    "composite_score": 1_000_000,
                    "agent": "AttackerSolver",
                    "artifact_sha256": proof_sha,
                    "statement_sha256": stmt_sha
                }
            ]))
            .unwrap(),
        )
        .unwrap();

        let transport = MapTransport::new();
        let report = fetch_promotion_candidates_with(
            &transport,
            &default_args(reg_path, out_dir, Some(tasks_path)),
            Some("tok"),
        )
        .unwrap();
        assert!(report.candidates.is_empty());
        assert_eq!(report.rejected.len(), 2);
        assert!(report.rejected[0]
            .reason
            .contains("untrusted or missing poster wallet"));
        assert!(report.rejected[1]
            .reason
            .contains("untrusted or missing poster wallet"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn catalog_alias_statement_mismatch_is_rejected() {
        let (dir, pkg_dir, out_dir) = setup_temp_workspace("catalog-mismatch");
        let reg_path = pkg_dir.join("registry.json");
        // Fake trivial statement claiming canonical catalog:chain-bsd-1-weierstrass-bijection topic!
        let fake_stmt = b"def statementProp : Prop := True\n";
        let proof =
            b"-- SPDX-License-Identifier: Apache-2.0\ntheorem proof : statementProp := trivial\n";
        let stmt_sha = sha256_hex(fake_stmt);
        let proof_sha = sha256_hex(proof);

        let tasks_path = dir.join("tasks.json");
        std::fs::write(
            &tasks_path,
            serde_json::to_vec(&json!([
                {
                    "task_id": "camp-fake:task-1",
                    "campaign_id": "camp-fake",
                    "client": PINNED_POSTER_WALLETS[0],
                    "campaign_topic": "catalog:chain-bsd-1-weierstrass-bijection",
                    "state": "finalized",
                    "platform": 10,
                    "lean_policy": 4,
                    "composite_score": 1_000_000,
                    "agent": "Solver1",
                    "artifact_sha256": proof_sha,
                    "statement_sha256": stmt_sha
                },
                {
                    "task_id": "camp-nonexistent:task-2",
                    "campaign_id": "camp-nonexistent",
                    "client": PINNED_POSTER_WALLETS[0],
                    "campaign_topic": "catalog:chain-nonexistent-rung",
                    "state": "finalized",
                    "platform": 10,
                    "lean_policy": 4,
                    "composite_score": 1_000_000,
                    "agent": "Solver1",
                    "artifact_sha256": proof_sha,
                    "statement_sha256": stmt_sha
                }
            ]))
            .unwrap(),
        )
        .unwrap();

        let mut transport = MapTransport::new();
        transport.insert_ok(
            gcs_media_url(
                DEFAULT_ARTIFACT_BUCKET,
                &format!("lean-proofs/{proof_sha}.lean"),
            ),
            proof.to_vec(),
        );
        transport.insert_ok(
            gcs_media_url(
                DEFAULT_ARTIFACT_BUCKET,
                &format!("lean-statements/{stmt_sha}.lean"),
            ),
            fake_stmt.to_vec(),
        );

        let report = fetch_promotion_candidates_with(
            &transport,
            &default_args(reg_path, out_dir, Some(tasks_path)),
            Some("tok"),
        )
        .unwrap();
        assert!(report.candidates.is_empty());
        assert_eq!(report.rejected.len(), 2);
        assert!(report.rejected[0]
            .reason
            .contains("catalog statement hash mismatch"));
        assert!(report.rejected[1]
            .reason
            .contains("catalog statement hash mismatch"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn alias_hijack_overwrite_is_refused() {
        let (dir, pkg_dir, out_dir) = setup_temp_workspace("alias-hijack");
        let reg_path = pkg_dir.join("registry.json");

        // Pre-populate registry with an active module mapped to `catalog:chain-bsd-1-weierstrass-bijection`:
        let b1_stmt = std::fs::read(
            repo_root()
                .join("tests/fixtures/lean/catalog/open/chain-bsd-1-weierstrass-bijection.lean"),
        )
        .unwrap();
        let first_proof =
            b"-- SPDX-License-Identifier: Apache-2.0\ntheorem proof : statementProp := rfl\n";
        let mut reg: RegistryManifest =
            serde_json::from_slice(&std::fs::read(&reg_path).unwrap()).unwrap();
        let env_id = reg.environments[0].environment_id.clone();
        let (first_cand, first_alias) = validate_and_build_candidate(
            &reg,
            &env_id,
            RawCandidateInputs {
                task_id: "camp-b1:original-task".into(),
                campaign_id: "camp-b1".into(),
                network: "mainnet".into(),
                author_wallet: "OriginalSolver".into(),
                campaign_topic: Some("catalog:chain-bsd-1-weierstrass-bijection".into()),
                artifact_sha256: sha256_hex(first_proof),
                proof_bytes: first_proof.to_vec(),
                expected_statement_sha256: Some(sha256_hex(&b1_stmt)),
                statement_bytes: b1_stmt.clone(),
                direct_dependencies: Vec::new(),
                verification_axioms: vec!["propext".into()],
            },
        )
        .unwrap();
        let first_manifest = autopromote_candidate(
            &first_cand,
            &mut reg,
            &pkg_dir,
            DEFAULT_PUBLIC_REGISTRY_BASE_URL,
        )
        .unwrap();
        reg.aliases
            .insert(first_alias.unwrap(), first_manifest.module_id);
        std::fs::write(&reg_path, serde_json::to_vec_pretty(&reg).unwrap()).unwrap();

        // Now a second task attempts to promote a DIFFERENT module under the same alias:
        let second_proof =
            b"-- SPDX-License-Identifier: Apache-2.0\n-- distinct proof\ntheorem proof : statementProp := rfl\n";
        let stmt_sha = sha256_hex(&b1_stmt);
        let second_proof_sha = sha256_hex(second_proof);

        let tasks_path = dir.join("tasks.json");
        std::fs::write(
            &tasks_path,
            serde_json::to_vec(&json!([{
                "task_id": "camp-b1:second-task",
                "campaign_id": "camp-b1",
                "client": PINNED_POSTER_WALLETS[0],
                "campaign_topic": "catalog:chain-bsd-1-weierstrass-bijection",
                "state": "finalized",
                "platform": 10,
                "lean_policy": 4,
                "composite_score": 1_000_000,
                "agent": "SecondSolver",
                "artifact_sha256": second_proof_sha,
                "statement_sha256": stmt_sha
            }]))
            .unwrap(),
        )
        .unwrap();

        let mut transport = MapTransport::new();
        transport.insert_ok(
            gcs_media_url(
                DEFAULT_ARTIFACT_BUCKET,
                &format!("lean-proofs/{second_proof_sha}.lean"),
            ),
            second_proof.to_vec(),
        );
        transport.insert_ok(
            gcs_media_url(
                DEFAULT_ARTIFACT_BUCKET,
                &format!("lean-statements/{stmt_sha}.lean"),
            ),
            b1_stmt,
        );

        let report = fetch_promotion_candidates_with(
            &transport,
            &default_args(reg_path, out_dir, Some(tasks_path)),
            Some("tok"),
        )
        .unwrap();
        assert!(report.candidates.is_empty());
        assert_eq!(report.rejected.len(), 1);
        assert!(
            report.rejected[0]
                .reason
                .contains("refusing to overwrite existing alias"),
            "unexpected reason: {}",
            report.rejected[0].reason
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn gcs_404_on_one_task_records_rejected_and_continues_promoting_next_task() {
        let (dir, pkg_dir, out_dir) = setup_temp_workspace("gcs-404-resilience");
        let reg_path = pkg_dir.join("registry.json");
        let good_stmt = std::fs::read(
            repo_root()
                .join("tests/fixtures/lean/catalog/open/chain-bsd-1-weierstrass-bijection.lean"),
        )
        .unwrap();
        let good_proof =
            b"-- SPDX-License-Identifier: Apache-2.0\ntheorem proof : statementProp := rfl\n";
        let good_stmt_sha = sha256_hex(&good_stmt);
        let good_proof_sha = sha256_hex(good_proof);
        let missing_proof_sha = "b".repeat(64);

        let tasks_path = dir.join("tasks.json");
        std::fs::write(
            &tasks_path,
            serde_json::to_vec(&json!([
                {
                    "task_id": "camp-404:broken-task",
                    "campaign_id": "camp-404",
                    "client": PINNED_POSTER_WALLETS[0],
                    "state": "finalized",
                    "platform": 10,
                    "lean_policy": 4,
                    "composite_score": 1_000_000,
                    "agent": "SolverBroken",
                    "artifact_sha256": missing_proof_sha,
                    "statement_sha256": good_stmt_sha
                },
                {
                    "task_id": "camp-ok:healthy-task",
                    "campaign_id": "camp-ok",
                    "client": PINNED_POSTER_WALLETS[0],
                    "campaign_topic": "catalog:chain-bsd-1-weierstrass-bijection",
                    "state": "finalized",
                    "platform": 10,
                    "lean_policy": 4,
                    "composite_score": 1_000_000,
                    "agent": "SolverHealthy",
                    "artifact_sha256": good_proof_sha,
                    "statement_sha256": good_stmt_sha
                }
            ]))
            .unwrap(),
        )
        .unwrap();

        let mut transport = MapTransport::new();
        // Intentionally do NOT mock `lean-proofs/{missing_proof_sha}.lean` so it 404s!
        transport.insert_ok(
            gcs_media_url(
                DEFAULT_ARTIFACT_BUCKET,
                &format!("lean-proofs/{good_proof_sha}.lean"),
            ),
            good_proof.to_vec(),
        );
        transport.insert_ok(
            gcs_media_url(
                DEFAULT_ARTIFACT_BUCKET,
                &format!("lean-statements/{good_stmt_sha}.lean"),
            ),
            good_stmt,
        );

        let report = fetch_promotion_candidates_with(
            &transport,
            &default_args(reg_path, out_dir, Some(tasks_path)),
            Some("tok"),
        )
        .unwrap();
        assert_eq!(report.rejected.len(), 1);
        assert_eq!(report.rejected[0].task_id, "camp-404:broken-task");
        assert_eq!(report.candidates.len(), 1);
        assert_eq!(report.candidates[0].task_id, "camp-ok:healthy-task");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn already_promoted_task_increments_already_promoted_and_skips() {
        let (dir, pkg_dir, out_dir) = setup_temp_workspace("already-promoted");
        let reg_path = pkg_dir.join("registry.json");
        let stmt = std::fs::read(
            repo_root()
                .join("tests/fixtures/lean/catalog/open/chain-bsd-1-weierstrass-bijection.lean"),
        )
        .unwrap();
        let proof =
            b"-- SPDX-License-Identifier: Apache-2.0\ntheorem proof : statementProp := rfl\n";

        let mut reg: RegistryManifest =
            serde_json::from_slice(&std::fs::read(&reg_path).unwrap()).unwrap();
        let env_id = reg.environments[0].environment_id.clone();
        let (cand, _) = validate_and_build_candidate(
            &reg,
            &env_id,
            RawCandidateInputs {
                task_id: "camp-done:task-done-1".into(),
                campaign_id: "camp-done".into(),
                network: "mainnet".into(),
                author_wallet: "SolverDone".into(),
                campaign_topic: None,
                artifact_sha256: sha256_hex(proof),
                proof_bytes: proof.to_vec(),
                expected_statement_sha256: Some(sha256_hex(&stmt)),
                statement_bytes: stmt,
                direct_dependencies: Vec::new(),
                verification_axioms: vec!["propext".into()],
            },
        )
        .unwrap();
        autopromote_candidate(&cand, &mut reg, &pkg_dir, DEFAULT_PUBLIC_REGISTRY_BASE_URL).unwrap();
        std::fs::write(&reg_path, serde_json::to_vec_pretty(&reg).unwrap()).unwrap();

        let tasks_path = dir.join("tasks.json");
        std::fs::write(
            &tasks_path,
            serde_json::to_vec(&json!([{
                "task_id": "camp-done:task-done-1",
                "campaign_id": "camp-done",
                "client": PINNED_POSTER_WALLETS[0],
                "state": "finalized",
                "platform": 10,
                "lean_policy": 4,
                "composite_score": 1_000_000
            }]))
            .unwrap(),
        )
        .unwrap();

        let transport = MapTransport::new();
        let report = fetch_promotion_candidates_with(
            &transport,
            &default_args(reg_path, out_dir, Some(tasks_path)),
            None,
        )
        .unwrap();
        assert_eq!(report.already_promoted, 1);
        assert!(report.candidates.is_empty());
        assert!(report.rejected.is_empty());
        assert!(transport.calls.borrow().is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn paginates_finalized_task_listing_across_multiple_pages() {
        let (dir, pkg_dir, out_dir) = setup_temp_workspace("pagination");
        let reg_path = pkg_dir.join("registry.json");
        let stmt = std::fs::read(
            repo_root()
                .join("tests/fixtures/lean/catalog/open/chain-bsd-1-weierstrass-bijection.lean"),
        )
        .unwrap();
        let proof =
            b"-- SPDX-License-Identifier: Apache-2.0\ntheorem proof : statementProp := rfl\n";
        let stmt_sha = sha256_hex(&stmt);
        let proof_sha = sha256_hex(proof);

        // Page 0: 100 warmup policy-2 tasks (forces fetching offset=100!)
        let page0: Vec<Value> = (0..100)
            .map(|i| {
                json!({
                    "task_id": format!("camp-warm:task-{i}"),
                    "campaign_id": "camp-warm",
                    "client": PINNED_POSTER_WALLETS[0],
                    "state": "finalized",
                    "platform": 10,
                    "lean_policy": 2,
                    "composite_score": 1_000_000,
                    "statement_lean": "def statementProp : Prop := True"
                })
            })
            .collect();
        // Page 1 (offset=100): 1 policy-4 candidate (< 100 items, terminates pagination!)
        let page1 = vec![json!({
            "task_id": "camp-page2:task-101",
            "campaign_id": "camp-page2",
            "client": PINNED_POSTER_WALLETS[0],
            "campaign_topic": "catalog:chain-bsd-1-weierstrass-bijection",
            "state": "finalized",
            "platform": 10,
            "lean_policy": 4,
            "composite_score": 1_000_000,
            "agent": "SolverPage2",
            "artifact_sha256": proof_sha,
            "statement_sha256": stmt_sha
        })];

        let mut transport = MapTransport::new();
        transport.insert_ok(
            "https://api.shillbot.org/tasks?state=finalized&platform=10&network=mainnet&limit=100&offset=0",
            serde_json::to_vec(&json!({ "tasks": page0 })).unwrap(),
        );
        transport.insert_ok(
            "https://api.shillbot.org/tasks?state=finalized&platform=10&network=mainnet&limit=100&offset=100",
            serde_json::to_vec(&json!({ "tasks": page1 })).unwrap(),
        );
        transport.insert_ok(
            gcs_media_url(
                DEFAULT_ARTIFACT_BUCKET,
                &format!("lean-proofs/{proof_sha}.lean"),
            ),
            proof.to_vec(),
        );
        transport.insert_ok(
            gcs_media_url(
                DEFAULT_ARTIFACT_BUCKET,
                &format!("lean-statements/{stmt_sha}.lean"),
            ),
            stmt,
        );

        let report = fetch_promotion_candidates_with(
            &transport,
            &default_args(reg_path, out_dir, None),
            Some("tok"),
        )
        .unwrap();
        assert_eq!(report.skipped, 100);
        assert_eq!(report.candidates.len(), 1);
        assert_eq!(report.candidates[0].task_id, "camp-page2:task-101");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn source_policy_parity_rejects_simp_instance_deriving_and_meta_at_fetch_stage() {
        let (dir, pkg_dir, out_dir) = setup_temp_workspace("sa-parity");
        let reg_path = pkg_dir.join("registry.json");
        let stmt = std::fs::read(
            repo_root()
                .join("tests/fixtures/lean/catalog/open/chain-bsd-1-weierstrass-bijection.lean"),
        )
        .unwrap();
        let stmt_sha = sha256_hex(&stmt);

        let bad_proofs: [(&str, &str); 4] = [
            (
                "camp-sa:simp",
                "-- SPDX-License-Identifier: Apache-2.0\n@[simp] theorem helper : True := trivial\ntheorem proof : statementProp := rfl\n",
            ),
            (
                "camp-sa:instance-colon",
                "-- SPDX-License-Identifier: Apache-2.0\ninstance:\n  Inhabited Nat := ⟨0⟩\ntheorem proof : statementProp := rfl\n",
            ),
            (
                "camp-sa:deriving",
                "-- SPDX-License-Identifier: Apache-2.0\ninductive Foo where | mk\n  deriving DecidableEq\ntheorem proof : statementProp := rfl\n",
            ),
            (
                "camp-sa:meta",
                "-- SPDX-License-Identifier: Apache-2.0\nmeta def helper : Nat := 0\ntheorem proof : statementProp := rfl\n",
            ),
        ];

        let mut transport = MapTransport::new();
        transport.insert_ok(
            gcs_media_url(
                DEFAULT_ARTIFACT_BUCKET,
                &format!("lean-statements/{stmt_sha}.lean"),
            ),
            stmt,
        );
        let mut task_items = Vec::new();
        for (tid, proof_str) in bad_proofs {
            let proof_bytes = proof_str.as_bytes();
            let p_sha = sha256_hex(proof_bytes);
            transport.insert_ok(
                gcs_media_url(
                    DEFAULT_ARTIFACT_BUCKET,
                    &format!("lean-proofs/{p_sha}.lean"),
                ),
                proof_bytes.to_vec(),
            );
            task_items.push(json!({
                "task_id": tid,
                "campaign_id": "camp-sa",
                "client": PINNED_POSTER_WALLETS[0],
                "campaign_topic": "catalog:chain-bsd-1-weierstrass-bijection",
                "state": "finalized",
                "platform": 10,
                "lean_policy": 4,
                "composite_score": 1_000_000,
                "agent": "SolverBad",
                "artifact_sha256": p_sha,
                "statement_sha256": stmt_sha
            }));
        }
        let tasks_path = dir.join("tasks.json");
        std::fs::write(&tasks_path, serde_json::to_vec(&task_items).unwrap()).unwrap();

        let report = fetch_promotion_candidates_with(
            &transport,
            &default_args(reg_path, out_dir, Some(tasks_path)),
            Some("tok"),
        )
        .unwrap();
        assert!(report.candidates.is_empty());
        assert_eq!(report.rejected.len(), 4);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn per_candidate_audit_isolation_rolls_back_bad_proof_persists_rejection_and_reconstructs_cleanly(
    ) {
        use crate::promote_pipeline::{
            apply_verified_promotions, audit_staged_candidates_with, ApplyVerifiedArgs,
            AuditStagedArgs,
        };
        let (dir, pkg_dir, out_dir) = setup_temp_workspace("sa-isolation");
        let real_pkg = repo_root().join("proofs/SwarmProofs");
        for rel in [
            "environment-lock.json",
            "SwarmProofs/Registry.lean",
            "SwarmProofs/Chains/BSD/Defs/V1.lean",
            "SwarmProofs/Chains/Circuits/Defs/V1.lean",
            "SwarmProofs/Chains/RHAnalytic/Defs/V1.lean",
            "SwarmProofs/Chains/RHLagarias/Defs/V1.lean",
        ] {
            let dst = pkg_dir.join(rel);
            std::fs::create_dir_all(dst.parent().unwrap()).unwrap();
            std::fs::copy(real_pkg.join(rel), dst).unwrap();
        }
        let reg_path = pkg_dir.join("registry.json");
        let rejected_path = pkg_dir.join("rejected-candidates.json");

        let b1_stmt = std::fs::read(
            repo_root()
                .join("tests/fixtures/lean/catalog/open/chain-bsd-1-weierstrass-bijection.lean"),
        )
        .unwrap();
        let c1_stmt = std::fs::read(
            repo_root().join(
                "tests/fixtures/lean/catalog/open/chain-rh-analytic-1-mertens-first-prime-log-bound.lean",
            ),
        )
        .unwrap();
        let bad_proof =
            b"-- SPDX-License-Identifier: Apache-2.0\ntheorem proof : statementProp := True.intro\n";
        let good_proof =
            b"-- SPDX-License-Identifier: Apache-2.0\ntheorem proof : statementProp := rfl\n";
        let b1_sha = sha256_hex(&b1_stmt);
        let c1_sha = sha256_hex(&c1_stmt);
        let bad_proof_sha = sha256_hex(bad_proof);
        let good_proof_sha = sha256_hex(good_proof);

        let tasks_path = dir.join("tasks.json");
        std::fs::write(
            &tasks_path,
            serde_json::to_vec(&json!([
                {
                    "task_id": "camp-bad:task-1",
                    "campaign_id": "camp-bad",
                    "client": PINNED_POSTER_WALLETS[0],
                    "campaign_topic": "catalog:chain-rh-analytic-1-mertens-first-prime-log-bound",
                    "state": "finalized",
                    "platform": 10,
                    "lean_policy": 4,
                    "composite_score": 1_000_000,
                    "agent": "SolverBad",
                    "artifact_sha256": bad_proof_sha,
                    "statement_sha256": c1_sha
                },
                {
                    "task_id": "camp-good:task-2",
                    "campaign_id": "camp-good",
                    "client": PINNED_POSTER_WALLETS[0],
                    "campaign_topic": "catalog:chain-bsd-1-weierstrass-bijection",
                    "state": "finalized",
                    "platform": 10,
                    "lean_policy": 4,
                    "composite_score": 1_000_000,
                    "agent": "SolverGood",
                    "artifact_sha256": good_proof_sha,
                    "statement_sha256": b1_sha
                }
            ]))
            .unwrap(),
        )
        .unwrap();

        let mut transport = MapTransport::new();
        transport.insert_ok(
            gcs_media_url(
                DEFAULT_ARTIFACT_BUCKET,
                &format!("lean-statements/{b1_sha}.lean"),
            ),
            b1_stmt,
        );
        transport.insert_ok(
            gcs_media_url(
                DEFAULT_ARTIFACT_BUCKET,
                &format!("lean-statements/{c1_sha}.lean"),
            ),
            c1_stmt,
        );
        transport.insert_ok(
            gcs_media_url(
                DEFAULT_ARTIFACT_BUCKET,
                &format!("lean-proofs/{bad_proof_sha}.lean"),
            ),
            bad_proof.to_vec(),
        );
        transport.insert_ok(
            gcs_media_url(
                DEFAULT_ARTIFACT_BUCKET,
                &format!("lean-proofs/{good_proof_sha}.lean"),
            ),
            good_proof.to_vec(),
        );

        let mut fetch_args = default_args(reg_path.clone(), out_dir.clone(), Some(tasks_path));
        fetch_args.rejected_file = Some(rejected_path.clone());
        let report1 =
            fetch_promotion_candidates_with(&transport, &fetch_args, Some("tok")).unwrap();
        assert_eq!(report1.candidates.len(), 2);

        // Audit in an untrusted workspace copy: task-1 fails Lean audit, task-2 passes!
        let audit_pkg = dir.join("untrusted-audit-pkg");
        crate::promote_pipeline::stage_dry_run_swarm_proofs_copy(&pkg_dir, &audit_pkg).unwrap();
        let verdicts_path = dir.join("audit-verdicts.json");
        let verdicts = audit_staged_candidates_with(
            &repo_root(),
            &AuditStagedArgs {
                staged_candidates_dir: out_dir.clone(),
                package_root: audit_pkg.join("SwarmProofs"),
                verdicts_out: verdicts_path.clone(),
                network: "mainnet".into(),
                skip_nanoda: true,
            },
            |_root, _pkg, cand, _skip| {
                if cand.task_id == "camp-bad:task-1" {
                    anyhow::bail!("simulated Lean type mismatch in bad proof")
                } else {
                    Ok(())
                }
            },
        )
        .unwrap();
        assert_eq!(verdicts.passed_task_ids, vec!["camp-good:task-2"]);
        assert_eq!(verdicts.rejected.len(), 1);
        assert_eq!(verdicts.rejected[0].task_id, "camp-bad:task-1");

        // Reconstruct in open-pr purely from trusted out_dir + verdicts_path (zero files from untrusted audit_pkg!)
        let promoted_count = apply_verified_promotions(&ApplyVerifiedArgs {
            staged_candidates_dir: out_dir.clone(),
            verdicts_path: verdicts_path.clone(),
            registry_path: reg_path.clone(),
            package_root: pkg_dir.clone(),
            rejected_file: rejected_path.clone(),
            public_base_url: DEFAULT_PUBLIC_REGISTRY_BASE_URL.into(),
        })
        .unwrap();
        assert_eq!(promoted_count, 1);

        let final_reg: RegistryManifest =
            serde_json::from_slice(&std::fs::read(&reg_path).unwrap()).unwrap();
        assert_eq!(final_reg.modules.len(), 1);
        assert_eq!(final_reg.modules[0].originating_task_id, "camp-good:task-2");

        // Second cron run: camp-good:task-2 is already_promoted AND camp-bad:task-1 is skipped via rejected-candidates.json!
        let out_dir2 = dir.join("out2");
        fetch_args.out_dir = out_dir2;
        let report2 =
            fetch_promotion_candidates_with(&transport, &fetch_args, Some("tok")).unwrap();
        assert!(report2.candidates.is_empty());
        assert_eq!(report2.already_promoted, 1);
        assert_eq!(report2.skipped, 1);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn devnet_requires_isolated_dry_run_registry_and_never_touches_public_registry() {
        use crate::promote_pipeline::{run_promote_pipeline_with, PromotePipelineArgs};
        let public_pkg = repo_root().join("proofs/SwarmProofs");
        let public_reg_path = public_pkg.join("registry.json");
        let public_reg_before = std::fs::read(&public_reg_path).unwrap();

        let tmp = std::env::temp_dir().join(format!("w7-devnet-dryrun-{}", uuid::Uuid::new_v4()));
        let out_dir = tmp.join("out");
        std::fs::create_dir_all(&out_dir).unwrap();

        let b1_stmt = std::fs::read(
            repo_root()
                .join("tests/fixtures/lean/catalog/open/chain-bsd-1-weierstrass-bijection.lean"),
        )
        .unwrap();
        let proof =
            b"-- SPDX-License-Identifier: Apache-2.0\ntheorem proof : statementProp := rfl\n";
        let stmt_sha = sha256_hex(&b1_stmt);
        let proof_sha = sha256_hex(proof);

        let tasks_path = tmp.join("devnet-tasks.json");
        std::fs::write(
            &tasks_path,
            serde_json::to_vec(&json!([{
                "task_id": "dev-camp:dev-task-1",
                "campaign_id": "dev-camp",
                "client": PINNED_POSTER_WALLETS[0],
                "campaign_topic": "catalog:chain-bsd-1-weierstrass-bijection",
                "state": "finalized",
                "platform": 10,
                "lean_policy": 4,
                "composite_score": 1_000_000,
                "agent": "DevnetSolver",
                "artifact_sha256": proof_sha,
                "statement_sha256": stmt_sha
            }]))
            .unwrap(),
        )
        .unwrap();

        let mut transport = MapTransport::new();
        transport.insert_ok(
            gcs_media_url(
                DEFAULT_ARTIFACT_BUCKET,
                &format!("lean-statements/{stmt_sha}.lean"),
            ),
            b1_stmt,
        );
        transport.insert_ok(
            gcs_media_url(
                DEFAULT_ARTIFACT_BUCKET,
                &format!("lean-proofs/{proof_sha}.lean"),
            ),
            proof.to_vec(),
        );

        // 1. --network devnet WITHOUT --dry-run-registry MUST be rejected!
        let mut missing_dry = default_args(
            public_reg_path.clone(),
            out_dir.clone(),
            Some(tasks_path.clone()),
        );
        missing_dry.network = "devnet".into();
        let err1 = fetch_promotion_candidates_with(&transport, &missing_dry, Some("tok"))
            .unwrap_err()
            .to_string();
        assert!(
            err1.contains("devnet proofs never enter the public registry"),
            "unexpected error: {err1}"
        );

        // 2. --network devnet WITH --dry-run-registry pointing inside repo proofs/SwarmProofs MUST be rejected!
        let mut inside_public = missing_dry.clone();
        inside_public.dry_run_registry = Some(public_pkg.clone());
        let err2 = fetch_promotion_candidates_with(&transport, &inside_public, Some("tok"))
            .unwrap_err()
            .to_string();
        assert!(
            err2.contains("devnet proofs never enter the public registry"),
            "unexpected error: {err2}"
        );

        // 3. --network devnet WITH --dry-run-registry <tmp_isolated> builds & validates cleanly while public registry stays 100% untouched!
        let dry_isolated = tmp.join("dry-run-reg");
        let mut valid_devnet = missing_dry;
        valid_devnet.dry_run_registry = Some(dry_isolated.clone());
        let verdicts = run_promote_pipeline_with(
            &repo_root(),
            &transport,
            &PromotePipelineArgs {
                fetch: valid_devnet,
                skip_nanoda: true,
            },
            Some("tok"),
            |_root, staged_pkg, _cand, _skip| {
                let staged_reg: RegistryManifest = serde_json::from_slice(
                    &std::fs::read(staged_pkg.join("registry.json")).unwrap(),
                )
                .unwrap();
                proof_registry::validate_registry(&staged_reg)?;
                proof_registry::validate_frozen_library_modules(staged_pkg, &staged_reg)?;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(verdicts.passed_task_ids, vec!["dev-camp:dev-task-1"]);
        assert!(verdicts.rejected.is_empty());

        let staged_reg: RegistryManifest = serde_json::from_slice(
            &std::fs::read(dry_isolated.join("SwarmProofs/registry.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(staged_reg.modules.len(), 1);
        assert_eq!(staged_reg.modules[0].originating_network, "devnet");

        // Public registry in repo MUST be 100% byte-for-byte unchanged!
        assert_eq!(std::fs::read(&public_reg_path).unwrap(), public_reg_before);

        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
