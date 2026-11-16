use std::path::{Path, PathBuf};

const ALLOWED_PY_PREFIXES: &[&str] = &[
    "sdk/vow-verifier-py/",
    "examples/python-safe-recipe/",
    "research/wide-but-shallow/",
];

const ALLOWED_INVOCATION_CONTEXTS: &[&str] =
    &["sdk/vow-verifier-py", "examples/python-safe-recipe"];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("xtask crate must live at <repo>/crates/xtask")
        .to_path_buf()
}

fn should_skip_dir(rel: &str) -> bool {
    matches!(
        rel,
        ".git"
            | "target"
            | "node_modules"
            | ".anchor"
            | "test-ledger"
            | "evm/lib"
            | "proofs/SwarmProofs/.lake"
    ) || rel
        .strip_prefix("proofs/SwarmProofs/")
        .is_some_and(|rest| rest.starts_with('_') && !rest.contains('/'))
}

fn collect_disallowed_py_files(root: &Path) -> Vec<String> {
    let mut violations = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(it) => it,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(rel_path) = path.strip_prefix(root) else {
                continue;
            };
            let rel = rel_path.to_string_lossy().replace('\\', "/");
            let Ok(ft) = entry.file_type() else {
                continue;
            };
            if ft.is_dir() {
                if !should_skip_dir(&rel) {
                    stack.push(path);
                }
            } else if path.extension().and_then(|s| s.to_str()) == Some("py")
                && !ALLOWED_PY_PREFIXES.iter().any(|p| rel.starts_with(p))
            {
                violations.push(rel);
            }
        }
    }
    violations.sort();
    violations
}

fn line_invokes_python(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return false;
    }
    for token in
        trimmed.split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/')))
    {
        if token.is_empty() {
            continue;
        }
        let base = token.rsplit('/').next().unwrap_or(token);
        let is_py_bin = matches!(base, "python" | "python3" | "pytest" | "pip" | "pip3")
            || base.strip_prefix("python3.").is_some_and(|minor| {
                !minor.is_empty() && minor.bytes().all(|b| b.is_ascii_digit())
            });
        if is_py_bin || base.ends_with(".py") {
            return true;
        }
    }
    false
}

fn scan_file_for_python_invocations(rel_label: &str, content: &str) -> Vec<String> {
    let lines: Vec<&str> = content.lines().collect();
    let mut violations = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        if !line_invokes_python(line) {
            continue;
        }
        let window_start = idx.saturating_sub(8);
        let window_end = (idx + 9).min(lines.len());
        let context_block = lines[window_start..window_end].join("\n");
        if ALLOWED_INVOCATION_CONTEXTS
            .iter()
            .any(|allowed| context_block.contains(allowed))
        {
            continue;
        }
        violations.push(format!("{rel_label}:{}: {}", idx + 1, line.trim()));
    }
    violations
}

fn collect_script_and_workflow_targets(root: &Path) -> Vec<(String, PathBuf)> {
    let mut targets = Vec::new();
    let workflows_dir = root.join(".github").join("workflows");
    if let Ok(entries) = std::fs::read_dir(&workflows_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if matches!(
                path.extension().and_then(|s| s.to_str()),
                Some("yml" | "yaml")
            ) {
                if let Ok(rel) = path.strip_prefix(root) {
                    targets.push((rel.to_string_lossy().replace('\\', "/"), path));
                }
            }
        }
    }

    let scripts_dir = root.join("scripts");
    if scripts_dir.is_dir() {
        let mut stack = vec![scripts_dir];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let Ok(ft) = entry.file_type() else {
                    continue;
                };
                if ft.is_dir() {
                    stack.push(path);
                } else if path.extension().and_then(|s| s.to_str()) == Some("sh") {
                    if let Ok(rel) = path.strip_prefix(root) {
                        targets.push((rel.to_string_lossy().replace('\\', "/"), path));
                    }
                }
            }
        }
    }

    let makefile = root.join("Makefile");
    if makefile.is_file() {
        targets.push(("Makefile".to_string(), makefile));
    }
    targets.sort_by(|a, b| a.0.cmp(&b.0));
    targets
}

#[test]
fn no_internal_python_scripts_or_invocations() {
    let root = repo_root();
    let py_violations = collect_disallowed_py_files(&root);
    assert!(
        py_violations.is_empty(),
        "Internal Python scripts are banned (port to crates/xtask instead): {py_violations:#?}"
    );

    let mut invocation_violations = Vec::new();
    for (rel, path) in collect_script_and_workflow_targets(&root) {
        let content = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed reading {}: {e}", path.display()));
        invocation_violations.extend(scan_file_for_python_invocations(&rel, &content));
    }
    assert!(
        invocation_violations.is_empty(),
        "Internal Python invocations in workflows/scripts/Makefile are banned (use `cargo xtask` instead): {invocation_violations:#?}"
    );
}

#[test]
fn scanner_detects_python_tokens_without_false_positives() {
    assert!(line_invokes_python(
        "run: python3 scripts/check-proof-sources.py"
    ));
    assert!(line_invokes_python("python -m unittest"));
    assert!(line_invokes_python("/usr/bin/env python3.12 foo"));
    assert!(!line_invokes_python("set -euo pipefail"));
    assert!(!line_invokes_python("# python3 in comment is ignored"));

    let allowed_yaml =
        "- name: Test vow-verifier-py\n  working-directory: sdk/vow-verifier-py\n  run: pytest\n";
    assert!(scan_file_for_python_invocations("ci.yml", allowed_yaml).is_empty());

    let forbidden_yaml = "- name: Audit\n  run: python3 scripts/audit.py\n";
    assert_eq!(
        scan_file_for_python_invocations("ci.yml", forbidden_yaml).len(),
        1
    );
}
