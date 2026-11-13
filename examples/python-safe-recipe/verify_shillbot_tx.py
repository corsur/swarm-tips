#!/usr/bin/env python3
"""Standalone Python 3 stdlib fail-closed Shillbot Solana transaction verifier (C10)."""
from __future__ import annotations

import argparse
import base64
import hashlib
import json
import sys

B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
B58_MAP = {ch: idx for idx, ch in enumerate(B58)}
SHILLBOT_ID = "2tR37nqMpwdV4DVUHjzUmL1rH2DtkA8zrRA4EAhT7KMi"
COMPUTE_BUDGET_ID = "ComputeBudget111111111111111111111111111111"
SECP256K1_ID = "KeccakSecp256k11111111111111111111111111111"
SWITCHBOARD_IDS = {
    "mainnet": "SBondMDrcV3K4kxZR1HNVT7osZxAHVHgYXL5Ze1oMUv",
    "devnet": "Aio4gaXjXzJNVLtzwtNVmSqGKpANtXhybbkhtAC94ji2",
}

MAX_COMPUTE_UNIT_PRICE_MICROLAMPORTS = 100_000
MAX_COMPUTE_UNIT_LIMIT = 1_400_000
MIN_HEAP_FRAME_BYTES = 32 * 1024
MAX_HEAP_FRAME_BYTES = 256 * 1024

# Exact Anchor account-index specification:
# name -> (min_accts, exact_accts, task_slot, authority_slot, allows_sponsored_fee_payer)
INSTRUCTION_SPECS: dict[str, tuple[int, int | None, int | None, int | str, bool]] = {
    "create_task": (6, None, 1, 3, False),
    "claim_task": (5, None, 0, 3, True),
    "claim_task_session": (7, None, 0, 4, True),
    "set_payout_to": (2, 2, 0, 1, True),
    "submit_work": (4, None, 0, 3, True),
    "submit_work_session": (5, None, 0, 4, True),
    "approve_task": (2, None, 0, 1, False),
    "verify_task": (3, None, 0, "verify_task", False),
    "verify_task_attested": (3, None, 0, "verify_task_attested", True),
    "finalize_task": (5, None, 0, "fee_payer", False),
    "challenge_task": (5, None, 0, 3, False),
    "expire_task": (3, None, 0, "fee_payer", False),
    "resolve_challenge": (8, None, 0, 3, False),
    "resolve_challenge_default": (7, None, 0, "fee_payer", False),
    "create_session": (4, None, None, 1, False),
    "revoke_session": (2, None, None, 1, False),
    "register_identity": (3, None, None, 1, False),
    "revoke_identity": (2, None, None, 1, False),
    "close_agent_state": (2, None, None, 1, False),
    "migrate_agent_state": (2, None, None, 0, False),
    "migrate_task": (2, None, 0, 1, False),
    "initialize": (4, None, None, 1, False),
    "update_params": (2, None, None, 1, False),
    "transfer_authority": (2, None, None, 1, False),
    "update_treasury": (2, None, None, 1, False),
    "update_oracle_authority": (2, None, None, 1, False),
    "emergency_return": (2, None, None, 1, False),
}


def anchor_disc(name: str) -> bytes:
    return hashlib.sha256(f"global:{name}".encode("utf-8")).digest()[:8]


DISC_TO_NAME: dict[bytes, str] = {anchor_disc(name): name for name in INSTRUCTION_SPECS}

ACTION_TO_IX_NAMES: dict[str, tuple[str, ...]] = {
    "create": ("create_task",),
    "claim": ("claim_task", "claim_task_session"),
    "submit": ("submit_work", "submit_work_session"),
    "approve": ("approve_task",),
    "verify": ("verify_task", "verify_task_attested"),
    "finalize": ("finalize_task",),
}


def b58encode(data: bytes) -> str:
    zeros = len(data) - len(data.lstrip(b"\x00"))
    num, out = int.from_bytes(data, "big"), []
    while num:
        num, rem = divmod(num, 58)
        out.append(B58[rem])
    return "1" * zeros + "".join(reversed(out))


def b58decode(text: str) -> bytes:
    if not text:
        raise ValueError("Empty base58 string")
    zeros = 0
    for ch in text:
        if ch == "1":
            zeros += 1
        else:
            break
    num = 0
    for ch in text:
        if ch not in B58_MAP:
            raise ValueError(f"Invalid base58 character: {ch!r}")
        num = num * 58 + B58_MAP[ch]
    body = b"" if num == 0 else num.to_bytes((num.bit_length() + 7) // 8, "big")
    out = b"\x00" * zeros + body
    if b58encode(out) != text:
        raise ValueError("Non-canonical base58 encoding")
    return out


def decode_pubkey_32(text: str, label: str) -> bytes:
    raw = b58decode(text)
    if len(raw) != 32:
        raise ValueError(f"Invalid {label} public key length: expected 32 bytes, got {len(raw)}")
    return raw


def read_u16(buf: bytes, pos: int) -> tuple[int, int]:
    val, shift = 0, 0
    for _ in range(3):
        if pos >= len(buf):
            raise ValueError("Truncated compact-u16")
        b = buf[pos]
        pos += 1
        val |= (b & 0x7F) << shift
        if not (b & 0x80):
            return val, pos
        shift += 7
    raise ValueError("Invalid compact-u16")


def validate_compute_budget_instructions(
    ixs: list[tuple[str, list[str], bytes]],
    max_price_microlamports: int = MAX_COMPUTE_UNIT_PRICE_MICROLAMPORTS,
) -> None:
    seen_tags: set[int] = set()
    for prog_id, _, data in ixs:
        if prog_id != COMPUTE_BUDGET_ID:
            continue
        if len(data) == 0:
            raise ValueError("Empty ComputeBudget instruction data")
        tag = data[0]
        if tag in seen_tags:
            raise ValueError(f"Duplicate ComputeBudget instruction tag {tag}")
        seen_tags.add(tag)

        if tag == 1:
            # RequestHeapFrame(u32 bytes) — validate and continue (never early-return!)
            if len(data) != 5:
                raise ValueError(
                    f"Malformed ComputeBudget RequestHeapFrame length: expected 5, got {len(data)}"
                )
            heap_bytes = int.from_bytes(data[1:5], "little")
            if (
                heap_bytes < MIN_HEAP_FRAME_BYTES
                or heap_bytes > MAX_HEAP_FRAME_BYTES
                or heap_bytes % 1024 != 0
            ):
                raise ValueError(f"ComputeBudget RequestHeapFrame {heap_bytes} out of bounds")
            continue

        if tag == 2:
            # SetComputeUnitLimit(u32 units)
            if len(data) != 5:
                raise ValueError(
                    f"Malformed ComputeBudget SetComputeUnitLimit length: expected 5, got {len(data)}"
                )
            units = int.from_bytes(data[1:5], "little")
            if units < 1 or units > MAX_COMPUTE_UNIT_LIMIT:
                raise ValueError(f"ComputeBudget SetComputeUnitLimit {units} out of bounds")
            continue

        if tag == 3:
            # SetComputeUnitPrice(u64 microLamports)
            if len(data) != 9:
                raise ValueError(
                    f"Malformed ComputeBudget SetComputeUnitPrice length: expected 9, got {len(data)}"
                )
            price = int.from_bytes(data[1:9], "little")
            if price > max_price_microlamports:
                raise ValueError(
                    f"ComputeBudget SetComputeUnitPrice {price} exceeds max {max_price_microlamports}"
                )
            continue

        if tag == 4:
            # SetLoadedAccountsDataSizeLimit(u32 bytes)
            if len(data) != 5:
                raise ValueError(
                    f"Malformed ComputeBudget SetLoadedAccountsDataSizeLimit length: expected 5, got {len(data)}"
                )
            limit = int.from_bytes(data[1:5], "little")
            if limit == 0:
                raise ValueError("ComputeBudget SetLoadedAccountsDataSizeLimit cannot be zero")
            continue

        raise ValueError(f"Unsupported or deprecated ComputeBudget instruction tag {tag}")


def validate_set_payout_to_ix(
    accts: list[str],
    data: bytes,
    wallet: str,
    target_task: str,
    expected_payout_bytes: bytes | None,
    expected_payout_b58: str | None,
) -> None:
    if expected_payout_bytes is None or expected_payout_b58 is None:
        raise ValueError("Unexpected set_payout_to instruction without explicit --payout-to")
    if expected_payout_bytes == b"\x00" * 32:
        raise ValueError("set_payout_to destination cannot be the default zero pubkey")
    if expected_payout_b58 == target_task:
        raise ValueError("set_payout_to destination cannot equal the task PDA")
    if len(accts) != 2:
        raise ValueError(f"set_payout_to requires exact 2 accounts [task, agent], got {len(accts)}")
    if accts[0] != target_task:
        raise ValueError("set_payout_to account [0] does not match target task PDA")
    if accts[1] != wallet:
        raise ValueError("set_payout_to authority slot [1] does not match expected wallet")
    if len(data) != 40:
        raise ValueError(f"set_payout_to data must be exact 40 bytes, got {len(data)}")
    if data[8:40] != expected_payout_bytes:
        raise ValueError("set_payout_to destination pubkey does not match expected --payout-to")


def verify_authority_and_shape(
    ix_name: str,
    accts: list[str],
    data: bytes,
    wallet: str,
    fee_payer_actual: str,
    expected_fee_payer: str,
    signers: list[str],
    oracle_authority: str | None,
) -> None:
    min_accts, exact_accts, _, auth_slot, sponsored_ok = INSTRUCTION_SPECS[ix_name]
    if len(accts) < min_accts:
        raise ValueError(f"Instruction {ix_name} requires >= {min_accts} accounts, got {len(accts)}")
    if exact_accts is not None and len(accts) != exact_accts:
        raise ValueError(f"Instruction {ix_name} requires exact {exact_accts} accounts, got {len(accts)}")

    if not sponsored_ok and fee_payer_actual != expected_fee_payer:
        raise ValueError(
            f"Instruction {ix_name} requires fee_payer {expected_fee_payer}, got {fee_payer_actual}"
        )

    if isinstance(auth_slot, int):
        actual_auth = accts[auth_slot]
        if actual_auth != wallet:
            raise ValueError(
                f"Instruction {ix_name} authority slot [{auth_slot}] is {actual_auth}, expected {wallet}"
            )
        if actual_auth not in signers:
            raise ValueError(
                f"Instruction {ix_name} authority slot [{auth_slot}] ({actual_auth}) is not a signer"
            )
        if ix_name == "claim_task_session" and accts[5] not in signers:
            raise ValueError("Instruction claim_task_session payer slot [5] is not a signer")
    elif auth_slot == "fee_payer":
        if fee_payer_actual != expected_fee_payer:
            raise ValueError(
                f"Instruction {ix_name} requires fee_payer {expected_fee_payer}, got {fee_payer_actual}"
            )
    elif auth_slot == "verify_task":
        if len(data) != 48:
            raise ValueError(f"Instruction verify_task data must be 48 bytes, got {len(data)}")
        if oracle_authority and accts[2] != oracle_authority:
            raise ValueError("verify_task switchboard_feed [2] differs from expected oracle_authority")
        if fee_payer_actual != expected_fee_payer:
            raise ValueError(
                f"Instruction verify_task requires fee_payer {expected_fee_payer}, got {fee_payer_actual}"
            )
    elif auth_slot == "verify_task_attested":
        if len(data) != 48:
            raise ValueError(
                f"Instruction verify_task_attested data must be 48 bytes, got {len(data)}"
            )
        attester = accts[2]
        if attester not in signers:
            raise ValueError("Instruction verify_task_attested attester slot [2] must be a signer")
        if oracle_authority and attester != oracle_authority:
            raise ValueError(
                "Instruction verify_task_attested attester slot [2] differs from expected oracle_authority"
            )
        wallet_is_attester = attester == wallet
        wallet_is_expected_payer = fee_payer_actual == expected_fee_payer and wallet == expected_fee_payer
        if not wallet_is_attester and not wallet_is_expected_payer:
            raise ValueError(
                f"Instruction verify_task_attested requires wallet {wallet} at attester [2] or fee_payer"
            )


def verify_tx(
    tx_b64: str,
    action: str,
    wallet: str,
    network: str = "mainnet",
    fee_payer: str | None = None,
    task_pda: str | None = None,
    payout_to: str | None = None,
    oracle_authority: str | None = None,
    escrow_lamports: int | None = None,
) -> dict:
    if network not in SWITCHBOARD_IDS:
        raise ValueError(f"Unsupported network: {network}")
    if action not in ACTION_TO_IX_NAMES and action not in INSTRUCTION_SPECS:
        raise ValueError(f"Unsupported action: {action}")

    decode_pubkey_32(wallet, "wallet")
    if fee_payer is not None:
        decode_pubkey_32(fee_payer, "fee_payer")
    if task_pda is not None:
        decode_pubkey_32(task_pda, "task_pda")
    if oracle_authority is not None:
        decode_pubkey_32(oracle_authority, "oracle_authority")
    expected_payout_bytes: bytes | None = None
    if payout_to is not None:
        expected_payout_bytes = decode_pubkey_32(payout_to, "payout_to")
        if expected_payout_bytes == b"\x00" * 32:
            raise ValueError("payout_to cannot be the default zero pubkey")

    raw = base64.b64decode(tx_b64.strip())
    num_sigs, pos = read_u16(raw, 0)
    pos += 64 * num_sigs
    if pos >= len(raw):
        raise ValueError("Truncated wire transaction header")
    is_v0 = bool(raw[pos] & 0x80)
    if is_v0:
        if (raw[pos] & 0x7F) != 0:
            raise ValueError("Unsupported transaction version")
        pos += 1
    if pos + 3 > len(raw):
        raise ValueError("Truncated message header")
    num_req_sigs = raw[pos]
    pos += 3
    num_keys, pos = read_u16(raw, pos)
    if num_keys < 1 or num_req_sigs > num_keys or pos + num_keys * 32 + 32 > len(raw):
        raise ValueError("Malformed account key table in wire transaction")
    keys = [b58encode(raw[pos + i * 32 : pos + (i + 1) * 32]) for i in range(num_keys)]
    pos += num_keys * 32 + 32  # skip static keys + 32-byte recent blockhash
    num_ixs, pos = read_u16(raw, pos)
    ixs: list[tuple[str, list[str], bytes]] = []
    for _ in range(num_ixs):
        if pos >= len(raw) or raw[pos] >= num_keys:
            raise ValueError("Invalid program_id index in compiled instruction")
        prog_id = keys[raw[pos]]
        accts_len, pos = read_u16(raw, pos + 1)
        if pos + accts_len > len(raw):
            raise ValueError("Truncated instruction accounts")
        acct_indices = raw[pos : pos + accts_len]
        if any(idx >= num_keys for idx in acct_indices):
            raise ValueError("Out-of-bounds account index in compiled instruction")
        accts = [keys[i] for i in acct_indices]
        data_len, pos = read_u16(raw, pos + accts_len)
        if pos + data_len > len(raw):
            raise ValueError("Truncated instruction data")
        ixs.append((prog_id, accts, raw[pos : pos + data_len]))
        pos += data_len
    if is_v0:
        num_alts, pos = read_u16(raw, pos)
        if num_alts != 0:
            raise ValueError("Address lookup tables are forbidden")
    if pos != len(raw):
        raise ValueError("Unexpected trailing bytes in wire transaction")

    signers = keys[:num_req_sigs]
    fee_payer_actual = keys[0]
    expected_fee_payer = fee_payer or wallet
    if wallet not in signers:
        raise ValueError("Wallet is not a required signer")
    if fee_payer and fee_payer_actual != fee_payer:
        raise ValueError(f"Fee payer {fee_payer_actual} differs from expected {fee_payer}")

    is_verify = action in ("verify", "verify_task", "verify_task_attested")
    allowed = {SHILLBOT_ID, COMPUTE_BUDGET_ID} | (
        {SWITCHBOARD_IDS[network], SECP256K1_ID} if is_verify else set()
    )
    for prog_id, _, _ in ixs:
        if prog_id not in allowed:
            raise ValueError(f"Unexpected program {prog_id}")

    validate_compute_budget_instructions(ixs)

    decoded_shillbot: list[tuple[int, str, list[str], bytes]] = []
    for tx_idx, (prog_id, accts, data) in enumerate(ixs):
        if prog_id != SHILLBOT_ID:
            continue
        if len(data) < 8:
            raise ValueError("Shillbot instruction shorter than 8-byte discriminator")
        ix_name = DISC_TO_NAME.get(data[:8])
        if ix_name is None:
            raise ValueError("Unknown Shillbot instruction discriminator")
        decoded_shillbot.append((tx_idx, ix_name, accts, data))

    target_names = ACTION_TO_IX_NAMES.get(action, (action,))
    lifecycle = [entry for entry in decoded_shillbot if entry[1] in target_names]
    if len(lifecycle) != 1:
        raise ValueError(
            f"Expected single Shillbot {'/'.join(target_names)} instruction without unexpected companions"
        )

    primary_idx, primary_name, primary_accts, primary_data = lifecycle[0]
    companions = [entry for entry in decoded_shillbot if entry is not lifecycle[0]]

    verify_authority_and_shape(
        primary_name,
        primary_accts,
        primary_data,
        wallet,
        fee_payer_actual,
        expected_fee_payer,
        signers,
        oracle_authority,
    )

    _, _, task_slot, _, _ = INSTRUCTION_SPECS[primary_name]
    target_task: str | None = primary_accts[task_slot] if task_slot is not None else None
    if task_pda and target_task != task_pda:
        raise ValueError("Task PDA differs from expected")

    if primary_name == "create_task":
        if len(primary_data) < 24:
            raise ValueError("Create instruction data is truncated")
        if escrow_lamports is not None:
            actual_escrow = int.from_bytes(primary_data[16:24], "little")
            if actual_escrow != escrow_lamports:
                raise ValueError(
                    f"Escrow amount {actual_escrow} differs from expected {escrow_lamports}"
                )
    elif primary_name in ("submit_work", "submit_work_session"):
        if len(primary_data) < 12:
            raise ValueError("Submit instruction data is truncated")
        content_len = int.from_bytes(primary_data[8:12], "little")
        if len(primary_data) != 12 + content_len:
            raise ValueError("Submit instruction content_id length mismatch")
    elif primary_name == "set_payout_to":
        assert target_task is not None
        validate_set_payout_to_ix(
            primary_accts,
            primary_data,
            wallet,
            target_task,
            expected_payout_bytes,
            payout_to,
        )
    elif primary_name == "finalize_task" and payout_to is not None:
        if primary_accts[2] != payout_to:
            raise ValueError("finalize_task payout recipient slot [2] differs from expected --payout-to")

    def valid_claim_companion(comp_name: str, comp_accts: list[str]) -> bool:
        if primary_name == "submit_work":
            return (
                comp_name == "claim_task"
                and len(comp_accts) >= 5
                and comp_accts[0] == primary_accts[0]
                and comp_accts[3] == wallet
            )
        if primary_name == "submit_work_session":
            return (
                comp_name == "claim_task_session"
                and len(comp_accts) >= 7
                and comp_accts[0] == primary_accts[0]
                and comp_accts[3] == primary_accts[3]
                and comp_accts[4] == wallet
                and comp_accts[5] in signers
            )
        return False

    is_claim = action == "claim" or primary_name in ("claim_task", "claim_task_session")
    is_submit = action == "submit" or primary_name in ("submit_work", "submit_work_session")

    if len(companions) == 0:
        if payout_to is not None and primary_name not in ("set_payout_to", "finalize_task"):
            raise ValueError("Expected set_payout_to companion instruction for --payout-to")
    elif is_claim and len(companions) == 1 and companions[0][1] == "set_payout_to":
        if primary_name != "claim_task":
            raise ValueError("set_payout_to companion requires direct agent claim_task")
        comp_idx, _, comp_accts, comp_data = companions[0]
        if comp_idx <= primary_idx:
            raise ValueError("payout route must follow claim")
        assert target_task is not None
        validate_set_payout_to_ix(
            comp_accts,
            comp_data,
            wallet,
            target_task,
            expected_payout_bytes,
            payout_to,
        )
    elif (
        is_submit
        and len(companions) == 1
        and companions[0][1] in ("claim_task", "claim_task_session")
    ):
        if payout_to is not None:
            raise ValueError("Expected set_payout_to companion instruction for --payout-to")
        comp_idx, comp_name, comp_accts, _ = companions[0]
        if primary_idx != comp_idx + 1 or not valid_claim_companion(comp_name, comp_accts):
            raise ValueError(
                f"Expected single Shillbot {primary_name} instruction without unexpected companions"
            )
    elif (
        is_submit
        and len(companions) == 2
        and companions[0][1] == "claim_task"
        and companions[1][1] == "set_payout_to"
    ):
        claim_idx, claim_name, claim_accts, _ = companions[0]
        payout_idx, _, payout_accts, payout_data = companions[1]
        if (
            primary_name != "submit_work"
            or payout_idx != claim_idx + 1
            or primary_idx != payout_idx + 1
            or not valid_claim_companion(claim_name, claim_accts)
        ):
            raise ValueError(
                f"Expected single Shillbot {primary_name} instruction without unexpected companions"
            )
        assert target_task is not None
        validate_set_payout_to_ix(
            payout_accts,
            payout_data,
            wallet,
            target_task,
            expected_payout_bytes,
            payout_to,
        )
    else:
        if any(c[1] == "set_payout_to" for c in companions):
            raise ValueError("Unexpected set_payout_to companion instruction")
        raise ValueError(
            f"Expected single Shillbot {primary_name} instruction without unexpected companions"
        )

    return {
        "ok": True,
        "action": action,
        "instruction": primary_name,
        "fee_payer": fee_payer_actual,
        "signers": signers,
    }


def main() -> int:
    p = argparse.ArgumentParser(description="Verify unsigned Shillbot Solana transaction")
    p.add_argument(
        "--action",
        required=True,
        choices=[*ACTION_TO_IX_NAMES.keys(), *INSTRUCTION_SPECS.keys()],
    )
    p.add_argument("--wallet", required=True)
    p.add_argument("--fee-payer")
    p.add_argument("--task-pda")
    p.add_argument("--payout-to")
    p.add_argument("--oracle-authority")
    p.add_argument("--escrow-lamports", type=int)
    p.add_argument("--network", default="mainnet", choices=["mainnet", "devnet"])
    p.add_argument("--tx")
    args = p.parse_args()
    try:
        res = verify_tx(
            args.tx or sys.stdin.read(),
            args.action,
            args.wallet,
            args.network,
            args.fee_payer,
            args.task_pda,
            payout_to=args.payout_to,
            oracle_authority=args.oracle_authority,
            escrow_lamports=args.escrow_lamports,
        )
        print(json.dumps(res))
        return 0
    except Exception as exc:
        print(json.dumps({"ok": False, "error": str(exc)}))
        return 1


if __name__ == "__main__":
    sys.exit(main())
