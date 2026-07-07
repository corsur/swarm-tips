//! Shared golden-vector harness: fixture paths + regenerate/verify
//! helpers used by every schema's vector test (`golden_vectors.rs`,
//! `verify_vectors.rs`). The pattern: an `#[ignore]` regenerator writes
//! the committed JSON; the default verify test fails if the committed
//! file drifts from the current Rust encoder, while the Solidity mirror's
//! forge tests read the SAME file — so the encoders can never silently
//! diverge.

pub fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len().saturating_mul(2).saturating_add(2));
    s.push_str("0x");
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

pub fn fixture_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

/// Regenerate a committed fixture (called from `#[ignore]` tests only).
pub fn write_fixture(name: &str, content: &str) {
    let path = fixture_path(name);
    std::fs::create_dir_all(path.parent().expect("fixture dir")).expect("create fixtures dir");
    std::fs::write(&path, content).expect("write fixture");
}

/// Fail if the committed fixture differs from what the current encoder
/// produces. `hint` names the regenerator test to run intentionally.
pub fn assert_fixture_current(name: &str, expected: &str, hint: &str) {
    let path = fixture_path(name);
    let on_disk = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "missing {}: run the ignored {hint} test to generate it ({e})",
            path.display()
        )
    });
    assert_eq!(
        on_disk, expected,
        "{name} is stale — the Rust encoder changed; regenerate with the ignored {hint} test \
         and confirm the Solidity mirror still passes"
    );
}
