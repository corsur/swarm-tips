//! 32-byte-word encoding helpers shared by every canonical schema in this
//! crate (`cert_schema`, `verify_schema`). One field = one big-endian
//! 32-byte word, byte-identical to Solidity's `abi.encode` for value
//! types — that equivalence is what the golden vectors in
//! `tests/fixtures/` pin against the Solidity mirrors.

pub const WORD: usize = 32;

pub fn push_word(out: &mut Vec<u8>, word: &[u8; 32]) {
    out.extend_from_slice(word);
}

pub fn push_u128(out: &mut Vec<u8>, value: u128) {
    let mut word = [0u8; 32];
    word[16..].copy_from_slice(&value.to_be_bytes());
    out.extend_from_slice(&word);
}

pub fn push_u64(out: &mut Vec<u8>, value: u64) {
    push_u128(out, u128::from(value));
}

pub fn push_u32(out: &mut Vec<u8>, value: u32) {
    push_u128(out, u128::from(value));
}

pub fn push_u16(out: &mut Vec<u8>, value: u16) {
    push_u128(out, u128::from(value));
}

pub fn push_u8(out: &mut Vec<u8>, value: u8) {
    push_u128(out, u128::from(value));
}

pub fn push_addr20(out: &mut Vec<u8>, addr: &[u8; 20]) {
    let mut word = [0u8; 32];
    word[12..].copy_from_slice(addr);
    out.extend_from_slice(&word);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_ints_left_pad_to_one_be_word() {
        let mut out = Vec::new();
        push_u8(&mut out, 1);
        push_u16(&mut out, 0x0102);
        push_u32(&mut out, 0x01020304);
        push_u64(&mut out, 0x0102030405060708);
        assert_eq!(out.len(), 4usize.saturating_mul(WORD));
        assert_eq!(out[31], 1);
        assert_eq!(&out[62..64], &[0x01, 0x02]);
        assert_eq!(&out[92..96], &[0x01, 0x02, 0x03, 0x04]);
        assert!(out[64..92].iter().all(|b| *b == 0));
    }

    #[test]
    fn addr20_left_pads_12_zero_bytes() {
        let mut out = Vec::new();
        push_addr20(&mut out, &[0xEE; 20]);
        assert_eq!(out.len(), WORD);
        assert!(out[..12].iter().all(|b| *b == 0));
        assert!(out[12..].iter().all(|b| *b == 0xEE));
    }
}
