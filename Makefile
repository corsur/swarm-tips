.PHONY: setup build test clean

# Run the one-time circuit trusted setup.
# Installs circom (Rust binary) and generates the verifying key + proving artifacts.
# Must be run before `make test`. Only needs to be re-run if the circuit changes.
setup:
	@echo "==> Installing circom..."
	cargo install --git https://github.com/iden3/circom
	@echo "==> Running circuit setup (downloads ~1 MB ptau file)..."
	cd circuits/bool_range && npm install && ./setup.sh
	@echo ""
	@echo "Setup complete. Commit circuits/bool_range/verifying_key.rs and"
	@echo "circuits/bool_range/circuit_final.zkey to the repository."

# Build the Solana program.
build:
	anchor build

# Run all tests against a local validator.
# Requires `make setup` to have been run at least once.
test:
	anchor test

clean:
	anchor clean
	rm -f circuits/bool_range/circuit_final.zkey
	rm -f circuits/bool_range/circuit_0000.zkey
	rm -f circuits/bool_range/bool_range.r1cs
	rm -f circuits/bool_range/bool_range.sym
	rm -rf circuits/bool_range/bool_range_js
	rm -f circuits/bool_range/powersOfTau28_hez_final_08.ptau
