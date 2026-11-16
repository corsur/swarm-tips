use anyhow::{ensure, Context, Result};
use std::fmt::Write as _;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub struct HttpRequest<'a> {
    pub method: &'a str,
    pub url: &'a str,
    pub bearer_token: Option<&'a str>,
    pub content_type: Option<&'a str>,
    pub body: Option<&'a [u8]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpFailure {
    Status { status: u16, body: Vec<u8> },
    Transport(String),
}

impl std::fmt::Display for HttpFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Status { status, body } => {
                let snippet = String::from_utf8_lossy(body);
                write!(f, "HTTP {status}: {snippet}")
            }
            Self::Transport(msg) => write!(f, "HTTP transport failure: {msg}"),
        }
    }
}

impl std::error::Error for HttpFailure {}

pub trait HttpTransport {
    fn send(&self, request: &HttpRequest<'_>) -> std::result::Result<Vec<u8>, HttpFailure>;
}

pub struct CurlTransport;

struct ScopedTempFile {
    path: PathBuf,
}

impl ScopedTempFile {
    fn create(prefix: &str, contents: &[u8]) -> Result<Self> {
        ensure!(!prefix.is_empty(), "temp file prefix must be non-empty");
        let seq = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let name = format!("xtask-{prefix}-{}-{seq}.tmp", std::process::id());
        let path = std::env::temp_dir().join(name);
        write_private_file(&path, contents)?;
        ensure!(path.exists(), "temp file must exist after creation");
        Ok(Self { path })
    }
}

impl Drop for ScopedTempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(unix)]
fn write_private_file(path: &Path, contents: &[u8]) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("failed to create temp file {}", path.display()))?;
    file.write_all(contents)
        .with_context(|| format!("failed to write temp file {}", path.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn write_private_file(path: &Path, contents: &[u8]) -> Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("failed to create temp file {}", path.display()))?;
    file.write_all(contents)
        .with_context(|| format!("failed to write temp file {}", path.display()))?;
    Ok(())
}

fn validate_bearer_token(token: &str) -> std::result::Result<(), HttpFailure> {
    if token.is_empty()
        || !token
            .bytes()
            .all(|b| b.is_ascii_graphic() && b != b'"' && b != b'\\')
    {
        return Err(HttpFailure::Transport(
            "invalid bearer token characters".into(),
        ));
    }
    Ok(())
}

impl HttpTransport for CurlTransport {
    fn send(&self, request: &HttpRequest<'_>) -> std::result::Result<Vec<u8>, HttpFailure> {
        if !request.url.starts_with("https://") && !request.url.starts_with("http://127.0.0.1") {
            return Err(HttpFailure::Transport(format!(
                "refusing non-HTTPS URL: {}",
                request.url
            )));
        }
        if let Some(token) = request.bearer_token {
            validate_bearer_token(token)?;
        }
        let resp_file = ScopedTempFile::create("resp", &[])
            .map_err(|e| HttpFailure::Transport(e.to_string()))?;
        let req_file = match request.body {
            Some(payload) => Some(
                ScopedTempFile::create("req", payload)
                    .map_err(|e| HttpFailure::Transport(e.to_string()))?,
            ),
            None => None,
        };
        let status = run_curl_command(
            request,
            &resp_file.path,
            req_file.as_ref().map(|f| f.path.as_path()),
        )?;
        let body = std::fs::read(&resp_file.path)
            .map_err(|e| HttpFailure::Transport(format!("failed to read response body: {e}")))?;
        if (200..300).contains(&status) {
            Ok(body)
        } else {
            Err(HttpFailure::Status { status, body })
        }
    }
}

fn run_curl_command(
    request: &HttpRequest<'_>,
    resp_path: &Path,
    req_path: Option<&Path>,
) -> std::result::Result<u16, HttpFailure> {
    let mut cmd = Command::new("curl");
    cmd.arg("-sS")
        .arg("--max-time")
        .arg("30")
        .arg("-X")
        .arg(request.method)
        .arg("-o")
        .arg(resp_path)
        .arg("-w")
        .arg("%{http_code}");
    if let Some(ct) = request.content_type {
        cmd.arg("-H").arg(format!("Content-Type: {ct}"));
    }
    if let Some(body_path) = req_path {
        let mut arg = std::ffi::OsString::from("@");
        arg.push(body_path.as_os_str());
        cmd.arg("--data-binary").arg(arg);
    }
    if request.bearer_token.is_some() {
        cmd.arg("--config").arg("-").stdin(Stdio::piped());
    } else {
        cmd.stdin(Stdio::null());
    }
    cmd.arg("--")
        .arg(request.url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| HttpFailure::Transport(format!("failed to spawn curl: {e}")))?;
    if let Some(token) = request.bearer_token {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| HttpFailure::Transport("missing curl stdin pipe".into()))?;
        stdin
            .write_all(format!("header = \"Authorization: Bearer {token}\"\n").as_bytes())
            .map_err(|e| {
                HttpFailure::Transport(format!("failed writing curl stdin config: {e}"))
            })?;
    }
    let output = child
        .wait_with_output()
        .map_err(|e| HttpFailure::Transport(format!("failed waiting for curl: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(HttpFailure::Transport(format!(
            "curl exited {}: {stderr}",
            output.status
        )));
    }
    let code_str = String::from_utf8_lossy(&output.stdout);
    code_str
        .trim()
        .parse::<u16>()
        .map_err(|e| HttpFailure::Transport(format!("invalid HTTP status `{code_str}`: {e}")))
}

/// Percent-encode all bytes outside RFC 3986 unreserved characters (`safe=""`).
pub fn percent_encode(input: &str) -> String {
    let mut encoded = String::with_capacity(input.len());
    for byte in input.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    debug_assert!(encoded.len() >= input.len());
    encoded
}

pub fn gcloud_access_token() -> Result<String> {
    let output = Command::new("gcloud")
        .args(["auth", "print-access-token"])
        .stdin(Stdio::null())
        .output()
        .context("failed to invoke `gcloud auth print-access-token`")?;
    ensure!(
        output.status.success(),
        "`gcloud auth print-access-token` failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    let token = String::from_utf8(output.stdout)
        .context("gcloud access token is not valid UTF-8")?
        .trim()
        .to_string();
    ensure!(!token.is_empty(), "gcloud returned an empty access token");
    Ok(token)
}

pub fn git_head_commit(cwd: &Path) -> Result<String> {
    ensure!(
        cwd.exists(),
        "git working directory must exist: {}",
        cwd.display()
    );
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(cwd)
        .stdin(Stdio::null())
        .output()
        .context("failed to invoke `git rev-parse HEAD`")?;
    ensure!(
        output.status.success(),
        "`git rev-parse HEAD` failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    let commit = String::from_utf8(output.stdout)
        .context("git HEAD commit is not valid UTF-8")?
        .trim()
        .to_string();
    ensure!(
        commit.len() == 40
            && commit
                .bytes()
                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')),
        "unexpected git HEAD commit format: {commit}"
    );
    Ok(commit)
}

pub fn git_is_ancestor(ancestor: &str, descendant: &str) -> Result<bool> {
    ensure!(
        !ancestor.is_empty() && !descendant.is_empty(),
        "commit hashes must be non-empty"
    );
    let status = Command::new("git")
        .args(["merge-base", "--is-ancestor", ancestor, descendant])
        .stdin(Stdio::null())
        .status()
        .context("failed to invoke `git merge-base --is-ancestor`")?;
    Ok(status.success())
}
