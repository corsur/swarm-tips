pragma circom 2.0.0;

include "circomlib/circuits/sha256/sha256.circom";
include "circomlib/circuits/bitify.circom";

// Proves two things simultaneously:
//   1. guess ∈ {0, 1}
//   2. SHA-256(guess_byte || salt_bytes) == commitment
//
// The commitment is split into two 128-bit halves to fit within the BN254
// scalar field (~254 bits). The on-chain reveal_guess independently verifies
// SHA-256(guess || salt) == stored_commitment using sol_sha256.
//
// Public inputs:
//   commitment_high  — high 128 bits of SHA-256(guess || salt) as a field element
//   commitment_low   — low  128 bits of SHA-256(guess || salt) as a field element
//
// Private inputs:
//   guess            — 0 (Human) or 1 (AI)
//   salt             — 256 bits, big-endian (MSB of first byte is index 0)

template BoolRange() {
    signal input guess;
    signal input salt[256];
    signal input commitment_high;
    signal input commitment_low;

    // Constraint: guess is boolean
    guess * (guess - 1) === 0;

    // SHA-256 over 264 bits: 1-byte guess || 32-byte salt.
    // The Sha256 component takes bits in big-endian order (MSB of first byte = index 0).
    component sha = Sha256(264);

    // First byte: 0x00 or 0x01 in big-endian bit order.
    // Bits 0..6 are always zero; bit 7 (LSB of the byte) holds the guess value.
    for (var i = 0; i < 7; i++) {
        sha.in[i] <== 0;
    }
    sha.in[7] <== guess;

    // Remaining 256 bits: salt, already in big-endian bit order.
    for (var i = 0; i < 256; i++) {
        sha.in[8 + i] <== salt[i];
    }

    // Convert high 128 bits of SHA-256 output to a field element.
    // Bits2Num expects in[0] = LSB, so we reverse the bit order.
    component high = Bits2Num(128);
    for (var i = 0; i < 128; i++) {
        high.in[127 - i] <== sha.out[i];
    }
    commitment_high === high.out;

    // Convert low 128 bits of SHA-256 output to a field element.
    component low = Bits2Num(128);
    for (var i = 0; i < 128; i++) {
        low.in[127 - i] <== sha.out[128 + i];
    }
    commitment_low === low.out;
}

component main {public [commitment_high, commitment_low]} = BoolRange();
