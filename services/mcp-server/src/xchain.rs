//! Cross-chain (Solana ↔ EVM) discovery for MCP agents.
//!
//! The `xchain_supported_chains` tool exposes the chain registry's cross-chain
//! match configuration so an agent can discover which chains it can stake on
//! and at what amount before registering a wallet. This is the read-only entry
//! point to the cross-chain game; the stake-and-match tools (which pair a
//! Solana leg with an EVM leg via the game-api matchmaker and return unsigned
//! per-leg transactions) build on the same registry as their single source of
//! truth for addresses and stake amounts.
//!
//! Pure over `chain_registry` — no network, no state — so it is unit-tested
//! directly against the registry.

use serde_json::{json, Value};

/// One chain's agent-facing cross-chain match configuration. Base-unit amounts
/// are stringified: they are `u128` (wei) and can exceed JSON's safe-integer
/// range, so a string is the lossless wire form.
fn chain_json(e: &chain_registry::ChainEntry) -> Value {
    let namespace = chain_core::ChainId::parse(e.chain_id)
        .map(|c| c.namespace().as_str())
        .unwrap_or("unknown");
    // `live` is the deployment truth: a scaffolded mainnet chain (Base, Ethereum)
    // has no game contract yet, so it lists as "coming soon" — an agent must not
    // try to stake there. Base Sepolia is live today.
    let live = e.is_live(chain_registry::ContractPurpose::CrossChainGame);
    json!({
        "chain_id": e.chain_id,
        "namespace": namespace,
        "display_name": e.display_name,
        "native_symbol": e.native_symbol,
        "native_decimals": e.native_decimals,
        "stake_base_units": e.stake_base_units.to_string(),
        "max_tranche_base_units": e.max_tranche_base_units.to_string(),
        "claim_window_secs": e.claim_window_secs,
        "game_contract": e.game_contract,
        "live": live,
        "status": if live { "live" } else { "coming_soon" },
    })
}

/// The full `xchain_supported_chains` response: every registered chain plus a
/// plain-language description of how cross-chain matches resolve.
pub fn supported_chains_response() -> Value {
    let chains: Vec<Value> = chain_registry::all().map(chain_json).collect();
    json!({
        "chains": chains,
        "match_model": "A cross-chain match pairs a Solana leg (leg A) with an EVM leg \
            (leg B). Each player stakes their own chain's native coin into that chain's \
            game contract; the operator locks a float-pool tranche on each leg covering \
            the cross-chain counter-value of the opponent's stake. The match is settled \
            by a single certificate co-signed by both players' per-match session keys and \
            the operator, which each chain verifies independently and executes only its \
            own leg of — so a double-claim across chains is structurally impossible.",
        "settlement": "Instant path: a co-signed outcome certificate settles both legs. \
            Contested/timeout path: optimistic claims against the latest co-signed \
            checkpoint, with an equivocation-slash backstop.",
        "status": "testnet",
        "note": "Mainnet cross-chain routes are gated pending legal review and EVM \
            authority key rotation; only testnet chains (Solana devnet, Base Sepolia) \
            are live today.",
    })
}

/// The active EVM testnet chain for wallet registration. Mainnet EVM is
/// gated (decision.md §6), so a registered `0x` wallet is scoped to Base
/// Sepolia until that gate clears.
const EVM_TESTNET_CHAIN: &str = chain_registry::BASE_SEPOLIA_CAIP2;

/// Validate an EVM address and return its CAIP-10 account id on the active
/// testnet EVM chain. Accepts `0x` + exactly 40 hex chars (case-insensitive;
/// EIP-55 checksum is not enforced — wallets emit mixed-case but agents may
/// pass lowercase). Returns `Err(reason)` for malformed input so the boundary
/// rejects rather than guesses.
pub fn evm_account_id(address: &str) -> Result<String, String> {
    let hex = address
        .strip_prefix("0x")
        .ok_or("EVM address must start with 0x")?;
    if hex.len() != 40 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("EVM address must be 0x followed by 40 hex characters".to_string());
    }
    let chain = chain_core::ChainId::parse(EVM_TESTNET_CHAIN).map_err(|e| e.to_string())?;
    chain_core::AccountId::from_parts(&chain, address)
        .map(|a| a.as_str().to_string())
        .map_err(|e| e.to_string())
}

/// Resolve a session-bound wallet to the `(caip2_chain, chain_native_address)`
/// the cross-chain queue expects. An EVM registration is bound as its CAIP-10
/// account (`eip155:84532:0x…`) — split it. A Solana registration is bound as a
/// raw base58 pubkey — map it to the testnet cross-chain Solana chain (devnet)
/// from the registry. Returns `None` if neither shape matches.
pub fn resolve_xchain_wallet(bound: &str) -> Option<(String, String)> {
    if let Ok(acct) = chain_core::AccountId::parse(bound) {
        return Some((
            acct.chain().as_str().to_string(),
            acct.address().to_string(),
        ));
    }
    // Raw base58 Solana pubkey (the Solana register_wallet path stores this).
    if !bound.is_empty() && !bound.starts_with("0x") {
        let chain = chain_registry::cross_chain_solana()?.chain_id;
        return Some((chain.to_string(), bound.to_string()));
    }
    None
}

/// Seconds before `match_deadline` that the EVM leg's funding window closes.
/// `createMatch` requires `fundDeadline < matchDeadline`; the cert carries only
/// `match_deadline`, so the funding deadline is derived here.
const FUND_WINDOW_BEFORE_DEADLINE: u64 = 300;

fn decode_0x<const N: usize>(s: &str) -> Result<[u8; N], String> {
    let hexs = s.strip_prefix("0x").ok_or("expected 0x-prefixed hex")?;
    let bytes = hex::decode(hexs).map_err(|e| format!("bad hex: {e}"))?;
    // A cert word is 32 bytes (addresses left-padded); take the low N bytes.
    let start = bytes.len().checked_sub(N).ok_or("hex too short")?;
    // The discarded high bytes MUST be zero. Truncating them silently meant a
    // 32-byte word carrying junk (or a deliberately crafted value) decoded to a
    // DIFFERENT address than the one supplied, with no error — the caller then
    // built a cross-chain call against an address the payload never named.
    if bytes[..start].iter().any(|b| *b != 0) {
        return Err(format!(
            "hex value does not fit in {N} bytes: high bytes are non-zero"
        ));
    }
    bytes[start..]
        .try_into()
        .map_err(|_| "wrong length".to_string())
}

/// Build the unsigned EVM `createMatch` call for the EVM player from a matched
/// relay payload (`xchain_find_match`/`status` → `match`). The EVM leg is
/// `leg_b`; the counterparty session key is the Solana leg's. `playerIsP1` is
/// `a_is_p1 == 0` (the contract enforces the EVM seat is the complement of leg
/// A's). The client fills gas/nonce/chainId at submit time.
pub fn build_evm_create_match_call(payload: &Value) -> Result<Value, String> {
    let leg_b = payload.get("leg_b").ok_or("payload missing leg_b")?;
    let leg_a = payload.get("leg_a").ok_or("payload missing leg_a")?;
    let str_at = |v: &Value, k: &str| -> Result<String, String> {
        v.get(k)
            .and_then(|x| x.as_str())
            .map(str::to_string)
            .ok_or_else(|| format!("missing string field {k}"))
    };

    let match_id = decode_0x::<32>(&str_at(payload, "match_id")?)?;
    let contract = decode_0x::<20>(&str_at(leg_b, "contract")?)?;
    let session_key = decode_0x::<20>(&str_at(leg_b, "session_key")?)?;
    let counter_session_key = decode_0x::<20>(&str_at(leg_a, "session_key")?)?;
    let stake_wei: u128 = str_at(leg_b, "stake_base_units")?
        .parse()
        .map_err(|e| format!("bad stake: {e}"))?;
    let match_deadline = payload
        .get("match_deadline")
        .and_then(Value::as_u64)
        .ok_or("missing match_deadline")?;
    // REJECT a missing/malformed a_is_p1 rather than defaulting. This byte
    // decides which SEAT the EVM player takes, and unwrap_or(1) silently chose
    // "leg A is P1" => the EVM player is P2. A payload that lost the field
    // therefore built a createMatch for the WRONG seat, which the contract
    // accepts (it only enforces the complement) and which then mismatches the
    // Solana leg at settle. The sibling build_lock_tranche_call already uses
    // the required `u64_at("a_is_p1")?` form; this path now matches it.
    let a_is_p1 = payload
        .get("a_is_p1")
        .and_then(Value::as_u64)
        .ok_or("payload missing or malformed a_is_p1")?;
    let player_is_p1 = a_is_p1 == 0;
    let fund_deadline = match_deadline.saturating_sub(FUND_WINDOW_BEFORE_DEADLINE);
    // Operator authorization for createMatch (M4): game-api signs the exact
    // creation (matchId bound to the player + session keys + seat + deadlines) at
    // pairing and relays it here; the contract rejects an unauthorized createMatch.
    let operator_sig = decode_0x::<65>(&str_at(payload, "create_match_operator_sig")?)?;

    let call = evm_chain::build_create_match_parts(
        contract,
        match_id,
        session_key,
        counter_session_key,
        player_is_p1,
        fund_deadline,
        match_deadline,
        operator_sig,
        stake_wei,
    );
    let (to, data, value) = call.to_hex_parts();
    Ok(json!({
        "chain": str_at(leg_b, "chain")?,
        "to": to,
        "data": data,
        "value_wei": value,
        "fund_deadline": fund_deadline,
        "match_deadline": match_deadline,
        "instructions": "Sign this as an EIP-1559 transaction with your EVM wallet \
            (fill gas/nonce/chainId/maxFeePerGas locally) and submit it to fund your \
            leg. value_wei is the stake sent as native ETH.",
    }))
}

/// Build the unsigned EVM permissionless `lockTranche` call from a matched relay
/// payload. The operator's match-live signature (`operator_signature`, produced
/// at pairing) authorizes the lock — no new operator action — and the cert is
/// reconstructed from the payload (chain tags recomputed as `keccak256(chain id)`,
/// matching game-api's `chain_tag`). The locked amount is `legB.tranche`. The
/// client submits + pays gas. Returns the same client-ready shape as the fund
/// builder.
pub fn build_evm_lock_call(payload: &Value) -> Result<Value, String> {
    let leg_a = payload.get("leg_a").ok_or("payload missing leg_a")?;
    let leg_b = payload.get("leg_b").ok_or("payload missing leg_b")?;
    let str_at = |v: &Value, k: &str| -> Result<String, String> {
        v.get(k)
            .and_then(|x| x.as_str())
            .map(str::to_string)
            .ok_or_else(|| format!("missing string field {k}"))
    };
    let u64_at = |k: &str| -> Result<u64, String> {
        payload
            .get(k)
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("missing u64 field {k}"))
    };
    let leg_parts = |leg: &Value| -> Result<evm_chain::LegParts, String> {
        Ok(evm_chain::LegParts {
            // chain_tag isn't carried in the payload; recompute it the same way
            // game-api does so the reconstructed cert hashes identically (else the
            // operator signature won't verify on-chain).
            chain_tag: chain_core::cert_schema::keccak256(str_at(leg, "chain")?.as_bytes()),
            contract: decode_0x::<32>(&str_at(leg, "contract")?)?,
            player: decode_0x::<32>(&str_at(leg, "player")?)?,
            session_key: decode_0x::<20>(&str_at(leg, "session_key")?)?,
            stake: str_at(leg, "stake_base_units")?
                .parse()
                .map_err(|e| format!("bad stake: {e}"))?,
            tranche: str_at(leg, "tranche_base_units")?
                .parse()
                .map_err(|e| format!("bad tranche: {e}"))?,
        })
    };

    let cert = evm_chain::CertParts {
        match_id: decode_0x::<32>(&str_at(payload, "match_id")?)?,
        tournament_id: u64_at("tournament_id")?,
        matchup_commitment: decode_0x::<32>(&str_at(payload, "matchup_commitment")?)?,
        leg_a: leg_parts(leg_a)?,
        leg_b: leg_parts(leg_b)?,
        quote_timestamp: u64_at("quote_timestamp")?,
        quote_max_age_secs: u32::try_from(u64_at("quote_max_age_secs")?)
            .map_err(|_| "quote_max_age overflow".to_string())?,
        match_deadline: u64_at("match_deadline")?,
        claim_window_secs: u32::try_from(u64_at("claim_window_secs")?)
            .map_err(|_| "claim_window overflow".to_string())?,
        a_is_p1: u8::try_from(u64_at("a_is_p1")?).map_err(|_| "a_is_p1 overflow".to_string())?,
    };
    let contract = decode_0x::<20>(&str_at(leg_b, "contract")?)?;
    let mut operator_sig = decode_0x::<65>(&str_at(payload, "operator_signature")?)?;
    // The relay stores signatures with v normalized to 0/1 (the form the
    // Solana program and game-api verify); the EVM contract's ECDSA.recover
    // demands 27/28. Passing the relay form through verbatim made EVERY
    // lockTranche revert ECDSAInvalidSignature (0xf645eedf) — found live on
    // Base Sepolia 2026-08-30; the v+27 patched call landed. Tolerate both
    // forms here so neither storage convention can brick the lock.
    if operator_sig[64] < 27 {
        operator_sig[64] = operator_sig[64].saturating_add(27);
    }

    let call = evm_chain::build_lock_tranche_parts(contract, cert, operator_sig);
    let (to, data, value) = call.to_hex_parts();
    Ok(json!({
        "chain": str_at(leg_b, "chain")?,
        "to": to,
        "data": data,
        "value_wei": value,
        "match_id": str_at(payload, "match_id")?,
        "instructions": "Permissionless lock: sign this as an EIP-1559 transaction \
            with your EVM wallet (fill gas/nonce/chainId locally) and submit it to \
            lock your leg's tranche. value_wei is 0; you pay only gas. The operator \
            signature in the call authorizes it — no operator action needed.",
    }))
}

/// Build the unsigned EVM refund call (permissionless) for the EVM leg of a
/// cross-chain match, from a matched relay payload. `kind` is `"timeout"`
/// (after the claim window) or `"nocert"` (a funded match that never got a
/// cert). Returns the same client-ready shape as the fund builder.
pub fn build_evm_refund_call(payload: &Value, kind: &str) -> Result<Value, String> {
    let leg_b = payload.get("leg_b").ok_or("payload missing leg_b")?;
    let contract_hex = leg_b
        .get("contract")
        .and_then(|x| x.as_str())
        .ok_or("missing leg_b.contract")?;
    let match_id_hex = payload
        .get("match_id")
        .and_then(|x| x.as_str())
        .ok_or("missing match_id")?;
    let contract = decode_0x::<20>(contract_hex)?;
    let match_id = decode_0x::<32>(match_id_hex)?;

    let call = match kind {
        "timeout" => evm_chain::build_refund_timeout_parts(contract, match_id),
        "nocert" => evm_chain::build_refund_no_cert_parts(contract, match_id),
        other => return Err(format!("unknown refund kind '{other}' (timeout|nocert)")),
    };
    let (to, data, value) = call.to_hex_parts();
    Ok(json!({
        "chain": leg_b.get("chain").and_then(|x| x.as_str()).ok_or("missing leg_b.chain")?,
        "to": to,
        "data": data,
        "value_wei": value,
        "kind": kind,
        "instructions": "Sign this as an EIP-1559 transaction with your EVM wallet (refund is \
            permissionless; you pay only gas) and submit it to reclaim your stake.",
    }))
}

/// The `register_wallet` response for an EVM (`0x`) wallet. Unlike the Solana
/// path it carries no balance: the server holds no EVM RPC client by design
/// (cross-chain txs are built as unsigned calls the agent's own tooling
/// populates and submits), so the agent checks its own balance.
pub fn evm_registration_response(account_id: &str) -> Value {
    json!({
        "account": account_id,
        "namespace": "eip155",
        "chain": EVM_TESTNET_CHAIN,
        "status": "registered",
        "note": "EVM wallet registered for cross-chain Coordination Game matches on \
            Base Sepolia (testnet). Call xchain_supported_chains for stake amounts and \
            the deployed contract; cross-chain match tools return unsigned EVM \
            transactions you sign and submit locally (the server never holds your key \
            and never submits on your behalf).",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evm_account_id_accepts_valid_address_and_builds_caip10() {
        let acct =
            evm_account_id("0x996213ed4099707059b8b5d7489ffF23dAC9770d").expect("valid address");
        assert_eq!(
            acct,
            "eip155:84532:0x996213ed4099707059b8b5d7489ffF23dAC9770d"
        );
        // The CAIP-10 must round-trip through chain-core's parser.
        let parsed = chain_core::AccountId::parse(&acct).expect("valid CAIP-10");
        assert_eq!(parsed.chain().as_str(), EVM_TESTNET_CHAIN);
    }

    #[test]
    fn evm_account_id_rejects_malformed() {
        assert!(evm_account_id("996213ed4099707059b8b5d7489ffF23dAC9770d").is_err()); // no 0x
        assert!(evm_account_id("0x1234").is_err()); // too short
        assert!(evm_account_id("0xZZ6213ed4099707059b8b5d7489ffF23dAC9770d").is_err()); // non-hex
        assert!(evm_account_id("0x996213ed4099707059b8b5d7489ffF23dAC9770d00").is_err());
        // too long
    }

    /// A relay payload shaped like xchain::cert_relay_payload (leg_b = EVM).
    fn sample_create_match_payload() -> Value {
        json!({
            "match_id": format!("0x{}", "aa".repeat(32)),
            "match_deadline": 1_765_007_200u64,
            "a_is_p1": 0, // leg A (Solana) is NOT P1 => EVM player IS P1
            "leg_a": { "session_key": format!("0x{}", "13".repeat(20)) },
            "leg_b": {
                "chain": "eip155:84532",
                "contract": format!("0x{}", format!("{:0>64}", "c2eb26078dd5b1957883e1a9d651a28ef1f62aff")),
                "session_key": format!("0x{}", "23".repeat(20)),
                "stake_base_units": "10000000000000",
            },
            "create_match_operator_sig": format!("0x{}", "33".repeat(65)),
        })
    }

    #[test]
    fn build_evm_create_match_call_from_relay_payload() {
        let payload = sample_create_match_payload();
        let call = build_evm_create_match_call(&payload).expect("valid payload");
        // value is the stake; to is the unpadded 20-byte contract.
        assert_eq!(call["value_wei"], "10000000000000");
        assert_eq!(
            call["to"].as_str().unwrap().to_lowercase(),
            "0xc2eb26078dd5b1957883e1a9d651a28ef1f62aff"
        );
        // calldata: 0x + createMatch selector + 7 head words (6 static + operatorSig
        // offset) + the bytes tail (len + 65 padded to 96).
        let data = call["data"].as_str().unwrap();
        assert!(data.starts_with("0x"));
        assert_eq!(data.len(), 2 + (4 + 7 * 32 + 32 + 96) * 2);
        // fund_deadline derived strictly before match_deadline.
        assert!(call["fund_deadline"].as_u64().unwrap() < 1_765_007_200);
    }

    #[test]
    fn build_evm_create_match_call_rejects_missing_fields() {
        assert!(build_evm_create_match_call(&json!({})).is_err());
        assert!(build_evm_create_match_call(&json!({"leg_a":{},"leg_b":{}})).is_err());
    }

    fn lock_test_payload() -> Value {
        let leg = |sk: &str| {
            json!({
                "chain": "eip155:84532",
                "contract": format!("0x{}", "11".repeat(32)),
                "player": format!("0x{}", "22".repeat(32)),
                "session_key": format!("0x{}", sk.repeat(20)),
                "stake_base_units": "10000000000000",
                "tranche_base_units": "10000000000000",
            })
        };
        json!({
            "match_id": format!("0x{}", "aa".repeat(32)),
            "tournament_id": 2u64,
            "matchup_commitment": format!("0x{}", "bb".repeat(32)),
            "match_deadline": 1_765_007_200u64,
            "claim_window_secs": 3600u64,
            "quote_timestamp": 1_765_000_000u64,
            "quote_max_age_secs": 600u64,
            "a_is_p1": 1,
            "operator_signature": format!("0x{}", "cd".repeat(65)),
            "leg_a": leg("13"),
            "leg_b": {
                "chain": "eip155:84532",
                "contract": format!("0x{}", format!("{:0>64}", "c2eb26078dd5b1957883e1a9d651a28ef1f62aff")),
                "player": format!("0x{}", "22".repeat(32)),
                "session_key": format!("0x{}", "23".repeat(20)),
                "stake_base_units": "10000000000000",
                "tranche_base_units": "10000000000000",
            },
        })
    }

    #[test]
    fn build_evm_lock_call_from_relay_payload() {
        let payload = lock_test_payload();
        let call = build_evm_lock_call(&payload).expect("valid payload");
        // Permissionless lock: pays only gas (value 0); `to` is the EVM contract.
        assert_eq!(call["value_wei"], "0");
        assert_eq!(
            call["to"].as_str().unwrap().to_lowercase(),
            "0xc2eb26078dd5b1957883e1a9d651a28ef1f62aff"
        );
        let data = call["data"].as_str().unwrap();
        assert!(data.starts_with("0x"));
        // selector(4) + cert tuple + dynamic operatorSig (65 bytes) → well over 200 bytes.
        assert!(data.len() > 2 + 200 * 2, "lock calldata unexpectedly short");
        assert_eq!(call["match_id"], format!("0x{}", "aa".repeat(32)));
    }

    /// Live regression (Base Sepolia, 2026-08-30): the relay stores the
    /// operator match-live signature with v in 0/1 form; forwarding it
    /// verbatim made every lockTranche revert ECDSAInvalidSignature. The
    /// builder must emit contract-form v (27/28) — and leave already-
    /// normalized signatures untouched.
    #[test]
    fn build_evm_lock_call_normalizes_relay_form_v_to_contract_form() {
        let payload_with_v = |v: u8| {
            let mut sig = vec![0xcdu8; 65];
            sig[64] = v;
            let mut p = lock_test_payload();
            p["operator_signature"] = json!(format!("0x{}", hex::encode(sig)));
            p
        };
        // The dynamic bytes arg is ABI-tail-padded: the final 32-byte word is
        // the 65th signature byte (v) followed by 31 zeros.
        // Skip "0x" + the 8-hex selector so 32-byte words align.
        let v_word = |call: &serde_json::Value| {
            call["data"].as_str().unwrap()[2 + 8..]
                .chars()
                .collect::<Vec<_>>()
                .chunks(64)
                .last()
                .unwrap()
                .iter()
                .collect::<String>()
        };
        let relay_form = build_evm_lock_call(&payload_with_v(0)).expect("valid");
        assert!(
            v_word(&relay_form).starts_with("1b"),
            "v=0 must become 27 (0x1b), got word {}",
            v_word(&relay_form)
        );
        let contract_form = build_evm_lock_call(&payload_with_v(28)).expect("valid");
        assert!(
            v_word(&contract_form).starts_with("1c"),
            "v=28 must pass through untouched"
        );
    }

    #[test]
    fn build_evm_lock_call_rejects_missing_fields() {
        assert!(build_evm_lock_call(&json!({})).is_err());
        assert!(build_evm_lock_call(&json!({"leg_a":{},"leg_b":{}})).is_err());
    }

    #[test]
    fn build_evm_refund_call_for_timeout_and_nocert() {
        let payload = json!({
            "match_id": format!("0x{}", "aa".repeat(32)),
            "leg_b": {
                "chain": "eip155:84532",
                "contract": format!("0x{}", format!("{:0>64}", "c2eb26078dd5b1957883e1a9d651a28ef1f62aff")),
            },
        });
        let timeout = build_evm_refund_call(&payload, "timeout").expect("timeout refund");
        assert_eq!(timeout["value_wei"], "0");
        assert_eq!(
            timeout["to"].as_str().unwrap().to_lowercase(),
            "0xc2eb26078dd5b1957883e1a9d651a28ef1f62aff"
        );
        let nocert = build_evm_refund_call(&payload, "nocert").expect("nocert refund");
        // Different instruction → different calldata.
        assert_ne!(timeout["data"], nocert["data"]);
        // Unknown kind is rejected.
        assert!(build_evm_refund_call(&payload, "bogus").is_err());
    }

    #[test]
    fn resolve_xchain_wallet_splits_evm_caip10_and_maps_solana_base58() {
        // EVM: CAIP-10 binding splits into (chain, 0x address).
        let (chain, addr) =
            resolve_xchain_wallet("eip155:84532:0x996213ed4099707059b8b5d7489ffF23dAC9770d")
                .expect("evm caip-10");
        assert_eq!(chain, "eip155:84532");
        assert_eq!(addr, "0x996213ed4099707059b8b5d7489ffF23dAC9770d");

        // Solana: a raw base58 pubkey maps to the testnet cross-chain chain.
        let (chain, addr) =
            resolve_xchain_wallet("CKsZf8gZzfPdEXHxLUEPvAi5C5sXBV9aDhqmsM7yTipv").expect("solana");
        assert_eq!(chain, "solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1");
        assert_eq!(addr, "CKsZf8gZzfPdEXHxLUEPvAi5C5sXBV9aDhqmsM7yTipv");

        // Empty input is not resolvable.
        assert!(resolve_xchain_wallet("").is_none());
    }

    #[test]
    fn evm_registration_response_carries_account_and_chain() {
        let resp = evm_registration_response("eip155:84532:0xabc");
        assert_eq!(resp["account"], "eip155:84532:0xabc");
        assert_eq!(resp["namespace"], "eip155");
        assert_eq!(resp["chain"], EVM_TESTNET_CHAIN);
        assert_eq!(resp["status"], "registered");
    }

    #[test]
    fn lists_every_registered_chain_with_required_fields() {
        let resp = supported_chains_response();
        let chains = resp["chains"].as_array().expect("chains is an array");
        assert_eq!(chains.len(), chain_registry::all().count());

        for c in chains {
            // Every field the agent needs to decide whether to play must be present.
            assert!(c["chain_id"].is_string(), "chain_id missing");
            assert!(
                matches!(c["namespace"].as_str(), Some("solana") | Some("eip155")),
                "namespace must be a known CAIP namespace, got {:?}",
                c["namespace"]
            );
            assert!(c["native_symbol"].is_string(), "native_symbol missing");
            // Stake is stringified u128 and must parse back to a positive value.
            let stake: u128 = c["stake_base_units"]
                .as_str()
                .expect("stake_base_units is a string")
                .parse()
                .expect("stake_base_units parses as u128");
            assert!(stake > 0, "stake must be positive");
        }
    }

    #[test]
    fn base_sepolia_is_present_with_its_deployed_contract() {
        let resp = supported_chains_response();
        let chains = resp["chains"].as_array().unwrap();
        let base = chains
            .iter()
            .find(|c| c["chain_id"] == "eip155:84532")
            .expect("Base Sepolia registered");
        assert_eq!(base["namespace"], "eip155");
        assert_eq!(base["native_symbol"], "ETH");
        // The deployed CrossChainGame address must be surfaced to agents.
        assert!(
            base["game_contract"]
                .as_str()
                .unwrap_or("")
                .starts_with("0x"),
            "Base Sepolia must expose its deployed contract address"
        );
    }

    #[test]
    fn decode_0x_rejects_nonzero_high_bytes() {
        // A left-padded address is fine...
        let padded = format!("0x{}{}", "0".repeat(24), "11".repeat(20));
        assert!(decode_0x::<20>(&padded).is_ok());
        // ...but junk in the discarded high bytes must NOT be truncated away:
        // that silently yields a different address than the payload named.
        let dirty = format!("0x{}{}{}", "0".repeat(22), "ff", "11".repeat(20));
        let err = decode_0x::<20>(&dirty).unwrap_err();
        assert!(err.contains("high bytes are non-zero"), "{err}");
    }

    #[test]
    fn decode_0x_still_rejects_short_and_malformed_input() {
        assert!(decode_0x::<20>("0x1234").is_err());
        assert!(decode_0x::<20>("nope").is_err());
        assert!(decode_0x::<20>("0xzz").is_err());
    }

    #[test]
    fn create_match_rejects_a_missing_a_is_p1_instead_of_guessing_the_seat() {
        // a_is_p1 decides the EVM player's SEAT. Defaulting it silently built a
        // createMatch for the wrong seat, which the contract accepts and which
        // only diverges from the Solana leg at settle.
        let mut payload = sample_create_match_payload();
        payload.as_object_mut().unwrap().remove("a_is_p1");
        let err = build_evm_create_match_call(&payload).unwrap_err();
        assert!(err.contains("a_is_p1"), "{err}");
    }
}
