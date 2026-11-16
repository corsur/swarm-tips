use crate::http::{
    gcloud_access_token, git_head_commit, git_is_ancestor, percent_encode, CurlTransport,
    HttpFailure, HttpRequest, HttpTransport,
};
use anyhow::{bail, ensure, Context, Result};
use proof_registry::{canonical_json, sha256_hex};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct MirrorRegistry {
    #[serde(default)]
    modules: Vec<MirrorModule>,
}

#[derive(Debug, Deserialize)]
struct MirrorModule {
    module_id: String,
    module_name: String,
    source_sha256: String,
    source_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PublishedObject {
    generation: String,
    module_id: String,
    name: String,
    source_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PublicationReceipt<'a> {
    objects: Vec<PublishedObject>,
    registry_sha256: &'a str,
    schema: &'static str,
    source_commit: &'a str,
}

#[derive(Debug, Serialize)]
struct MultipartObjectMetadata<'a> {
    metadata: RegistryCustomMetadata<'a>,
    name: &'static str,
}

#[derive(Debug, Serialize)]
struct RegistryCustomMetadata<'a> {
    registry_sha256: &'a str,
    source_commit: &'a str,
}

pub fn mirror_proof_registry(project: &str, package: &Path) -> Result<()> {
    ensure!(!project.is_empty(), "--project must be non-empty");
    ensure!(
        package.is_dir(),
        "--package directory must exist: {}",
        package.display()
    );
    let access_token = gcloud_access_token()?;
    let source_commit = git_head_commit(package)?;
    let transport = CurlTransport;
    let hash = publish_registry_with(
        &transport,
        git_is_ancestor,
        project,
        package,
        &access_token,
        &source_commit,
    )?;
    println!("mirrored registry sha256={hash}");
    Ok(())
}

pub fn catalog_precondition<T, F>(
    transport: &T,
    mut is_ancestor: F,
    bucket: &str,
    access_token: &str,
    source_commit: &str,
) -> Result<String>
where
    T: HttpTransport,
    F: FnMut(&str, &str) -> Result<bool>,
{
    ensure!(!bucket.is_empty(), "bucket must be non-empty");
    let url = format!(
        "https://storage.googleapis.com/storage/v1/b/{}/o/registry.json",
        percent_encode(bucket)
    );
    let response = transport.send(&HttpRequest {
        method: "GET",
        url: &url,
        bearer_token: Some(access_token),
        content_type: None,
        body: None,
    });
    let body = match response {
        Ok(bytes) => bytes,
        Err(HttpFailure::Status { status: 404, .. }) => return Ok("0".to_string()),
        Err(err) => return Err(err.into()),
    };

    let metadata: Value = serde_json::from_slice(&body).context("invalid GCS metadata JSON")?;
    let previous = metadata
        .get("metadata")
        .and_then(|m| m.get("source_commit"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if previous.len() != 40
        || !previous
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    {
        bail!("existing catalog has no trustworthy source commit; refusing overwrite");
    }
    if !is_ancestor(previous, source_commit)? {
        bail!("refusing stale or divergent catalog publication");
    }
    let generation = metadata
        .get("generation")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("GCS catalog metadata missing generation"))?;
    ensure!(
        !generation.is_empty(),
        "GCS catalog generation must be non-empty"
    );
    Ok(generation.to_string())
}

pub fn immutable_upload<T: HttpTransport>(
    transport: &T,
    bucket: &str,
    name: &str,
    payload: &[u8],
    access_token: &str,
) -> Result<String> {
    ensure!(
        !bucket.is_empty() && !name.is_empty(),
        "bucket and object name required"
    );
    let encoded_bucket = percent_encode(bucket);
    let encoded_name = percent_encode(name);
    let upload_url = format!(
        "https://storage.googleapis.com/upload/storage/v1/b/{encoded_bucket}/o?uploadType=media&ifGenerationMatch=0&name={encoded_name}"
    );
    let upload_res = transport.send(&HttpRequest {
        method: "POST",
        url: &upload_url,
        bearer_token: Some(access_token),
        content_type: Some("application/octet-stream"),
        body: Some(payload),
    });
    match upload_res {
        Ok(resp_bytes) => return extract_generation(&resp_bytes),
        Err(HttpFailure::Status { status: 412, .. }) => {}
        Err(err) => return Err(err.into()),
    }

    let media_url = format!(
        "https://storage.googleapis.com/storage/v1/b/{encoded_bucket}/o/{encoded_name}?alt=media"
    );
    let existing = transport.send(&HttpRequest {
        method: "GET",
        url: &media_url,
        bearer_token: Some(access_token),
        content_type: None,
        body: None,
    })?;
    if existing != payload {
        bail!("immutable object differs from local bytes: gs://{bucket}/{name}");
    }

    let meta_url =
        format!("https://storage.googleapis.com/storage/v1/b/{encoded_bucket}/o/{encoded_name}");
    let meta_bytes = transport.send(&HttpRequest {
        method: "GET",
        url: &meta_url,
        bearer_token: Some(access_token),
        content_type: None,
        body: None,
    })?;
    extract_generation(&meta_bytes)
}

fn extract_generation(bytes: &[u8]) -> Result<String> {
    let val: Value = serde_json::from_slice(bytes).context("invalid GCS object JSON")?;
    let gen_str = val
        .get("generation")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing `generation` in GCS response"))?;
    ensure!(!gen_str.is_empty(), "GCS generation must be non-empty");
    Ok(gen_str.to_string())
}

fn publish_registry_with<T, F>(
    transport: &T,
    is_ancestor: F,
    project: &str,
    package: &Path,
    access_token: &str,
    source_commit: &str,
) -> Result<String>
where
    T: HttpTransport,
    F: FnMut(&str, &str) -> Result<bool>,
{
    let registry_path = package.join("registry.json");
    let registry_bytes = std::fs::read(&registry_path)
        .with_context(|| format!("failed reading {}", registry_path.display()))?;
    let registry: MirrorRegistry =
        serde_json::from_slice(&registry_bytes).context("invalid registry.json")?;
    let bucket = format!("{project}-swarm-proof-registry-public");

    let generation =
        catalog_precondition(transport, is_ancestor, &bucket, access_token, source_commit)?;
    let objects =
        upload_immutable_modules(transport, &bucket, package, &registry.modules, access_token)?;

    let registry_hash = sha256_hex(&registry_bytes);
    immutable_upload(
        transport,
        &bucket,
        &format!("catalogs/{registry_hash}.json"),
        &registry_bytes,
        access_token,
    )?;

    let receipt = PublicationReceipt {
        objects,
        registry_sha256: &registry_hash,
        schema: "swarm.lean-publication/v1",
        source_commit,
    };
    let receipt_bytes =
        canonical_json(&receipt).context("failed serializing publication receipt")?;
    immutable_upload(
        transport,
        &bucket,
        &format!("publications/{source_commit}.json"),
        &receipt_bytes,
        access_token,
    )?;

    activate_and_verify_catalog(
        transport,
        &bucket,
        &generation,
        source_commit,
        &registry_hash,
        &registry_bytes,
        access_token,
    )?;
    Ok(registry_hash)
}

fn upload_immutable_modules<T: HttpTransport>(
    transport: &T,
    bucket: &str,
    package: &Path,
    modules: &[MirrorModule],
    access_token: &str,
) -> Result<Vec<PublishedObject>> {
    let mut objects = Vec::with_capacity(modules.len());
    for module in modules {
        ensure!(
            !module.module_name.contains('/') && !module.module_name.contains(".."),
            "invalid module_name: {}",
            module.module_name
        );
        let module_id_hex = module
            .module_id
            .strip_prefix("sha256:")
            .unwrap_or(&module.module_id);
        let source_path = package.join(format!("{}.lean", module.module_name.replace('.', "/")));
        let source = std::fs::read(&source_path)
            .with_context(|| format!("failed reading {}", source_path.display()))?;
        if sha256_hex(&source) != module.source_sha256 || source.len() as u64 != module.source_bytes
        {
            bail!("source hash mismatch: {}", source_path.display());
        }
        let name = format!("modules/{module_id_hex}.lean");
        let object_generation = immutable_upload(transport, bucket, &name, &source, access_token)?;
        objects.push(PublishedObject {
            generation: object_generation,
            module_id: module.module_id.clone(),
            name,
            source_sha256: module.source_sha256.clone(),
        });
    }
    ensure!(
        objects.len() == modules.len(),
        "every module must produce a receipt record"
    );
    Ok(objects)
}

fn activate_and_verify_catalog<T: HttpTransport>(
    transport: &T,
    bucket: &str,
    generation: &str,
    source_commit: &str,
    registry_hash: &str,
    registry_bytes: &[u8],
    access_token: &str,
) -> Result<()> {
    let encoded_bucket = percent_encode(bucket);
    let encoded_name = percent_encode("registry.json");
    let boundary = uuid::Uuid::new_v4().simple().to_string();
    let meta = serde_json::to_vec(&MultipartObjectMetadata {
        metadata: RegistryCustomMetadata {
            registry_sha256: registry_hash,
            source_commit,
        },
        name: "registry.json",
    })?;

    let mut multipart = Vec::with_capacity(
        meta.len()
            .saturating_add(registry_bytes.len())
            .saturating_add(256),
    );
    multipart.extend_from_slice(
        format!("--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n").as_bytes(),
    );
    multipart.extend_from_slice(&meta);
    multipart.extend_from_slice(
        format!("\r\n--{boundary}\r\nContent-Type: application/json\r\n\r\n").as_bytes(),
    );
    multipart.extend_from_slice(registry_bytes);
    multipart.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let activate_url = format!(
        "https://storage.googleapis.com/upload/storage/v1/b/{encoded_bucket}/o?uploadType=multipart&ifGenerationMatch={generation}"
    );
    transport.send(&HttpRequest {
        method: "POST",
        url: &activate_url,
        bearer_token: Some(access_token),
        content_type: Some(&format!("multipart/related; boundary={boundary}")),
        body: Some(&multipart),
    })?;

    let verify_url = format!(
        "https://storage.googleapis.com/storage/v1/b/{encoded_bucket}/o/{encoded_name}?alt=media"
    );
    let mirrored = transport.send(&HttpRequest {
        method: "GET",
        url: &verify_url,
        bearer_token: Some(access_token),
        content_type: None,
        body: None,
    })?;
    if mirrored != registry_bytes {
        bail!("registry mirror did not return the committed bytes");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::cell::RefCell;
    use std::collections::VecDeque;

    struct MockTransport {
        responses: RefCell<VecDeque<std::result::Result<Vec<u8>, HttpFailure>>>,
        calls: RefCell<Vec<(String, String)>>,
    }

    impl MockTransport {
        fn new(items: Vec<std::result::Result<Vec<u8>, HttpFailure>>) -> Self {
            Self {
                responses: RefCell::new(items.into()),
                calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl HttpTransport for MockTransport {
        fn send(&self, request: &HttpRequest<'_>) -> std::result::Result<Vec<u8>, HttpFailure> {
            self.calls
                .borrow_mut()
                .push((request.method.to_string(), request.url.to_string()));
            self.responses
                .borrow_mut()
                .pop_front()
                .expect("unexpected HTTP call in MockTransport")
        }
    }

    #[test]
    fn initial_publication_requires_generation_zero() {
        let mock = MockTransport::new(vec![Err(HttpFailure::Status {
            status: 404,
            body: b"missing".to_vec(),
        })]);
        let gen_id = catalog_precondition(
            &mock,
            |_, _| panic!("ancestor check should not run on 404"),
            "bucket",
            "token",
            &"a".repeat(40),
        )
        .unwrap();
        assert_eq!(gen_id, "0");
    }

    #[test]
    fn existing_publication_requires_exact_generation() {
        let meta = serde_json::to_vec(&json!({
            "generation": "123",
            "metadata": { "source_commit": "a".repeat(40) }
        }))
        .unwrap();
        let mock = MockTransport::new(vec![Ok(meta)]);
        let mut checked = Vec::new();
        let gen_id = catalog_precondition(
            &mock,
            |prev, curr| {
                checked.push((prev.to_string(), curr.to_string()));
                Ok(true)
            },
            "bucket",
            "token",
            &"b".repeat(40),
        )
        .unwrap();
        assert_eq!(gen_id, "123");
        assert_eq!(checked, vec![("a".repeat(40), "b".repeat(40))]);
    }

    #[test]
    fn stale_commit_rejected() {
        let meta = serde_json::to_vec(&json!({
            "generation": "123",
            "metadata": { "source_commit": "b".repeat(40) }
        }))
        .unwrap();
        let mock = MockTransport::new(vec![Ok(meta)]);
        let err = catalog_precondition(&mock, |_, _| Ok(false), "bucket", "token", &"a".repeat(40))
            .unwrap_err()
            .to_string();
        assert!(err.contains("stale"), "unexpected error: {err}");
    }

    #[test]
    fn missing_commit_cannot_overwrite_catalog() {
        let mock = MockTransport::new(vec![Ok(br#"{"generation":"123"}"#.to_vec())]);
        let err = catalog_precondition(&mock, |_, _| Ok(true), "bucket", "token", &"a".repeat(40))
            .unwrap_err()
            .to_string();
        assert!(err.contains("trustworthy"), "unexpected error: {err}");
    }

    #[test]
    fn retry_requires_identical_immutable_bytes() {
        let conflict = Err(HttpFailure::Status {
            status: 412,
            body: b"exists".to_vec(),
        });
        let mock_ok = MockTransport::new(vec![
            conflict.clone(),
            Ok(b"same".to_vec()),
            Ok(br#"{"generation":"5"}"#.to_vec()),
        ]);
        assert_eq!(
            immutable_upload(&mock_ok, "bucket", "module", b"same", "token").unwrap(),
            "5"
        );

        let mock_diff = MockTransport::new(vec![conflict, Ok(b"different".to_vec())]);
        let err = immutable_upload(&mock_diff, "bucket", "module", b"same", "token")
            .unwrap_err()
            .to_string();
        assert!(err.contains("differs"), "unexpected error: {err}");
    }

    #[test]
    fn full_publish_flow_uploads_immutables_before_cas_catalog_activation() {
        let dir = std::env::temp_dir().join(format!("xtask-publish-{}", uuid::Uuid::new_v4()));
        let mod_dir = dir.join("SwarmProofs").join("Generated");
        std::fs::create_dir_all(&mod_dir).unwrap();
        let lean_src = b"-- SPDX-License-Identifier: Apache-2.0\ntheorem t : True := trivial\n";
        std::fs::write(mod_dir.join("H1234.lean"), lean_src).unwrap();
        let reg_bytes = serde_json::to_vec(&json!({
            "modules": [{
                "module_id": "sha256:1234",
                "module_name": "SwarmProofs.Generated.H1234",
                "source_sha256": sha256_hex(lean_src),
                "source_bytes": lean_src.len()
            }]
        }))
        .unwrap();
        std::fs::write(dir.join("registry.json"), &reg_bytes).unwrap();

        let mock = MockTransport::new(vec![
            Err(HttpFailure::Status {
                status: 404,
                body: Vec::new(),
            }),
            Ok(br#"{"generation":"1"}"#.to_vec()),
            Ok(br#"{"generation":"2"}"#.to_vec()),
            Ok(br#"{"generation":"3"}"#.to_vec()),
            Ok(br#"{"generation":"4"}"#.to_vec()),
            Ok(reg_bytes.clone()),
        ]);
        let commit = "c".repeat(40);
        let hash =
            publish_registry_with(&mock, |_, _| Ok(true), "proj", &dir, "tok", &commit).unwrap();
        assert_eq!(hash, sha256_hex(&reg_bytes));

        let calls = mock.calls.borrow();
        assert_eq!(calls.len(), 6);
        assert!(calls[1].1.contains("modules%2F1234.lean"));
        assert!(calls[2].1.contains("catalogs%2F"));
        assert!(calls[3].1.contains("publications%2F"));
        assert!(calls[4]
            .1
            .contains("uploadType=multipart&ifGenerationMatch=0"));
        assert!(calls[5].1.contains("registry.json?alt=media"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
