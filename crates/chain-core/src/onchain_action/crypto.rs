//! Zero-dependency cryptographic & encoding primitives for `onchain_action`
//! (`chain-core` compiles into Solana BPF with default features, so default
//! builds pull zero external crates).

/// Standard FIPS 180-4 SHA-256 initial hash values.
const SHA256_H0: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// Standard FIPS 180-4 SHA-256 round constants.
const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

fn compress_block(state: &mut [u32; 8], block: &[u8; 64]) {
    let mut w = [0u32; 64];
    let mut i = 0usize;
    while i < 16 {
        let base = i.saturating_mul(4);
        w[i] = u32::from_be_bytes([
            block[base],
            block[base | 1],
            block[base | 2],
            block[base | 3],
        ]);
        i = i.saturating_add(1);
    }
    while i < 64 {
        let w15 = w[i.saturating_sub(15)];
        let w2 = w[i.saturating_sub(2)];
        let s0 = w15.rotate_right(7) ^ w15.rotate_right(18) ^ (w15 >> 3);
        let s1 = w2.rotate_right(17) ^ w2.rotate_right(19) ^ (w2 >> 10);
        w[i] = w[i.saturating_sub(16)]
            .wrapping_add(s0)
            .wrapping_add(w[i.saturating_sub(7)])
            .wrapping_add(s1);
        i = i.saturating_add(1);
    }

    let mut a = state[0];
    let mut b = state[1];
    let mut c = state[2];
    let mut d = state[3];
    let mut e = state[4];
    let mut f = state[5];
    let mut g = state[6];
    let mut h = state[7];

    let mut round = 0usize;
    while round < 64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ ((!e) & g);
        let temp1 = h
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(SHA256_K[round])
            .wrapping_add(w[round]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let temp2 = s0.wrapping_add(maj);

        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(temp1);
        d = c;
        c = b;
        b = a;
        a = temp1.wrapping_add(temp2);
        round = round.saturating_add(1);
    }

    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
    state[4] = state[4].wrapping_add(e);
    state[5] = state[5].wrapping_add(f);
    state[6] = state[6].wrapping_add(g);
    state[7] = state[7].wrapping_add(h);
}

/// Compute FIPS 180-4 SHA-256 over `data`.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut state = SHA256_H0;
    let mut chunks = data.chunks_exact(64);
    for chunk in &mut chunks {
        let mut block = [0u8; 64];
        block.copy_from_slice(chunk);
        compress_block(&mut state, &block);
    }

    let rem = chunks.remainder();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut tail = [0u8; 128];
    tail[..rem.len()].copy_from_slice(rem);
    tail[rem.len()] = 0x80;

    let total_tail = if rem.len() < 56 { 64usize } else { 128usize };
    tail[total_tail.saturating_sub(8)..total_tail].copy_from_slice(&bit_len.to_be_bytes());

    let mut first = [0u8; 64];
    first.copy_from_slice(&tail[..64]);
    compress_block(&mut state, &first);
    if total_tail == 128 {
        let mut second = [0u8; 64];
        second.copy_from_slice(&tail[64..128]);
        compress_block(&mut state, &second);
    }

    let mut out = [0u8; 32];
    let mut idx = 0usize;
    while idx < 8 {
        let base = idx.saturating_mul(4);
        out[base..base.saturating_add(4)].copy_from_slice(&state[idx].to_be_bytes());
        idx = idx.saturating_add(1);
    }
    debug_assert_eq!(out.len(), 32);
    out
}

/// Decode a `0x`-prefixed lowercase or mixed-case hex string of exact byte length `N`.
pub fn decode_0x_hex_exact<const N: usize>(input: &str) -> Option<[u8; N]> {
    let hex_body = input.strip_prefix("0x")?;
    if hex_body.len() != N.checked_mul(2)? {
        return None;
    }
    let bytes = hex_body.as_bytes();
    let mut out = [0u8; N];
    let mut i = 0usize;
    while i < N {
        let hi = hex_nibble(bytes[i.saturating_mul(2)])?;
        let lo = hex_nibble(bytes[i.saturating_mul(2).saturating_add(1)])?;
        out[i] = (hi << 4) | lo;
        i = i.saturating_add(1);
    }
    Some(out)
}

/// Decode a `0x`-prefixed hex string of `1..=MAX_BYTES` bytes.
pub fn decode_0x_hex_bounded(input: &str, max_bytes: usize) -> Option<Vec<u8>> {
    let hex_body = input.strip_prefix("0x")?;
    if hex_body.is_empty() || hex_body.len() % 2 != 0 {
        return None;
    }
    let byte_len = hex_body.len() / 2;
    if byte_len > max_bytes {
        return None;
    }
    let raw = hex_body.as_bytes();
    let mut out = Vec::with_capacity(byte_len);
    let mut i = 0usize;
    while i < byte_len {
        let hi = hex_nibble(raw[i.saturating_mul(2)])?;
        let lo = hex_nibble(raw[i.saturating_mul(2).saturating_add(1)])?;
        out.push((hi << 4) | lo);
        i = i.saturating_add(1);
    }
    Some(out)
}

/// Encode bytes as `0x`-prefixed lowercase hex.
pub fn encode_0x_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len().saturating_mul(2).saturating_add(2));
    out.push_str("0x");
    for &b in bytes {
        out.push(HEX[usize::from(b >> 4)] as char);
        out.push(HEX[usize::from(b & 0x0f)] as char);
    }
    out
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b.saturating_sub(b'0')),
        b'a'..=b'f' => Some(b.saturating_sub(b'a').saturating_add(10)),
        b'A'..=b'F' => Some(b.saturating_sub(b'A').saturating_add(10)),
        _ => None,
    }
}

/// Decode Bitcoin/Solana base58 into an exact `N`-byte array (bounded input `<= 128` chars).
pub fn decode_base58_exact<const N: usize>(input: &str) -> Option<[u8; N]> {
    const ALPHABET: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    if input.is_empty() || input.len() > 128 {
        return None;
    }
    let mut leading_ones = 0usize;
    for &b in input.as_bytes() {
        if b == b'1' {
            leading_ones = leading_ones.saturating_add(1);
        } else {
            break;
        }
    }
    if leading_ones > N {
        return None;
    }

    let mut acc = [0u8; N];
    for &ch in input.as_bytes() {
        let digit = ALPHABET.iter().position(|&c| c == ch)? as u32;
        let mut carry = digit;
        let mut idx = N;
        while idx > 0 {
            idx = idx.saturating_sub(1);
            let val = u32::from(acc[idx]).saturating_mul(58).saturating_add(carry);
            acc[idx] = (val & 0xff) as u8;
            carry = val >> 8;
        }
        if carry != 0 {
            return None;
        }
    }
    // Verify leading zeros match leading '1's so overlong encodings are rejected.
    if acc.iter().take(leading_ones).any(|&b| b != 0) {
        return None;
    }
    if leading_ones < N && acc[leading_ones] == 0 {
        return None;
    }
    Some(acc)
}

const B64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Encode bytes as RFC 4648 §5 URL-safe base64 without `=` padding.
pub fn encode_base64url_nopad(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().saturating_mul(4).saturating_add(2) / 3);
    let mut chunks = data.chunks_exact(3);
    for c in &mut chunks {
        let n = (u32::from(c[0]) << 16) | (u32::from(c[1]) << 8) | u32::from(c[2]);
        out.push(B64URL[((n >> 18) & 0x3f) as usize] as char);
        out.push(B64URL[((n >> 12) & 0x3f) as usize] as char);
        out.push(B64URL[((n >> 6) & 0x3f) as usize] as char);
        out.push(B64URL[(n & 0x3f) as usize] as char);
    }
    let rem = chunks.remainder();
    if rem.len() == 1 {
        let n = u32::from(rem[0]) << 16;
        out.push(B64URL[((n >> 18) & 0x3f) as usize] as char);
        out.push(B64URL[((n >> 12) & 0x3f) as usize] as char);
    } else if rem.len() == 2 {
        let n = (u32::from(rem[0]) << 16) | (u32::from(rem[1]) << 8);
        out.push(B64URL[((n >> 18) & 0x3f) as usize] as char);
        out.push(B64URL[((n >> 12) & 0x3f) as usize] as char);
        out.push(B64URL[((n >> 6) & 0x3f) as usize] as char);
    }
    out
}

fn b64url_val(b: u8) -> Option<u32> {
    match b {
        b'A'..=b'Z' => Some(u32::from(b.saturating_sub(b'A'))),
        b'a'..=b'z' => Some(u32::from(b.saturating_sub(b'a').saturating_add(26))),
        b'0'..=b'9' => Some(u32::from(b.saturating_sub(b'0').saturating_add(52))),
        b'-' => Some(62),
        b'_' => Some(63),
        _ => None,
    }
}

/// Decode RFC 4648 §5 URL-safe unpadded base64 into an exact 65-byte ECDSA signature
/// (87 base64url chars with canonical zero trailing bits).
pub fn decode_base64url_sig65(input: &str) -> Option<[u8; 65]> {
    let raw = input.strip_suffix('=').unwrap_or(input).as_bytes();
    if raw.len() != 87 {
        return None;
    }
    let mut out = [0u8; 65];
    let mut in_idx = 0usize;
    let mut out_idx = 0usize;
    while in_idx < 84 {
        let v0 = b64url_val(raw[in_idx])?;
        let v1 = b64url_val(raw[in_idx.saturating_add(1)])?;
        let v2 = b64url_val(raw[in_idx.saturating_add(2)])?;
        let v3 = b64url_val(raw[in_idx.saturating_add(3)])?;
        let n = (v0 << 18) | (v1 << 12) | (v2 << 6) | v3;
        out[out_idx] = ((n >> 16) & 0xff) as u8;
        out[out_idx.saturating_add(1)] = ((n >> 8) & 0xff) as u8;
        out[out_idx.saturating_add(2)] = (n & 0xff) as u8;
        in_idx = in_idx.saturating_add(4);
        out_idx = out_idx.saturating_add(3);
    }
    // Final 3 base64url chars encode 2 bytes (16 bits) + 2 zero padding bits.
    let v0 = b64url_val(raw[84])?;
    let v1 = b64url_val(raw[85])?;
    let v2 = b64url_val(raw[86])?;
    if (v2 & 0x03) != 0 {
        return None;
    }
    let n = (v0 << 18) | (v1 << 12) | (v2 << 6);
    out[63] = ((n >> 16) & 0xff) as u8;
    out[64] = ((n >> 8) & 0xff) as u8;
    Some(out)
}
