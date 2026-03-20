#!/usr/bin/env bash
# Compiles the bool_range circuit and runs the Groth16 trusted setup.
# Outputs: circuit_final.zkey, verifying_key.json, and verifying_key.rs.
#
# Prerequisites: circom, node, npm
#   cargo install --git https://github.com/iden3/circom
#   npm install -g snarkjs
#
# Run from the circuits/bool_range/ directory.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

PTAU_URL="https://storage.googleapis.com/zkevm/ptau/powersOfTau28_hez_final_16.ptau"
PTAU_FILE="powersOfTau28_hez_final_16.ptau"

echo "==> Installing circomlib..."
npm install

echo "==> Compiling circuit..."
circom bool_range.circom --r1cs --wasm --sym -l node_modules -o .

echo "==> Downloading powers of tau (phase 1, reusable)..."
if [ ! -f "$PTAU_FILE" ]; then
    curl -L "$PTAU_URL" -o "$PTAU_FILE"
else
    echo "    (already present, skipping download)"
fi

echo "==> Circuit-specific setup (phase 2)..."
snarkjs groth16 setup bool_range.r1cs "$PTAU_FILE" circuit_0000.zkey

# Single-contributor phase 2 ceremony. For a trivial 1-constraint circuit
# this is sufficient; a multi-party ceremony can be added before mainnet.
ENTROPY=$(openssl rand -hex 32)
snarkjs zkey contribute circuit_0000.zkey circuit_final.zkey \
    --name="coordination-bool-range" -e="$ENTROPY"

echo "==> Verifying final key..."
snarkjs zkey verify bool_range.r1cs "$PTAU_FILE" circuit_final.zkey

echo "==> Exporting verification key JSON..."
snarkjs zkey export verificationkey circuit_final.zkey verifying_key.json

echo "==> Generating verifying_key.rs..."
node export_vk.js

echo "==> Cleaning up intermediate files..."
rm -f circuit_0000.zkey

echo ""
echo "Done. Next step: rebuild the Solana program."
echo "  cd ../../ && cargo build"
