fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let vk_path = format!("{}/../../circuits/bool_range/verifying_key.rs", manifest_dir);

    // Recompile the program whenever the verifying key is regenerated.
    println!("cargo:rerun-if-changed={}", vk_path);

    // Warn loudly if the placeholder key is still in place so developers
    // don't accidentally test against a key that rejects every proof.
    match std::fs::read_to_string(&vk_path) {
        Ok(contents) if contents.contains("THIS IS A PLACEHOLDER") => {
            println!(
                "cargo:warning=ZK verifying key is a placeholder — \
                all proofs will be rejected at runtime. \
                Run `make setup` (requires circom + node) to generate the real key."
            );
        }
        Err(e) => {
            println!(
                "cargo:warning=Could not read verifying_key.rs ({}). \
                Run `make setup` to generate it.",
                e
            );
        }
        _ => {}
    }
}
