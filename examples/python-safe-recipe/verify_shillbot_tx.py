#!/usr/bin/env python3
"""Standalone Python 3 stdlib fail-closed Shillbot Solana transaction verifier (C10)."""
import argparse, base64, hashlib, json, sys

B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
SHILLBOT_ID = "2tR37nqMpwdV4DVUHjzUmL1rH2DtkA8zrRA4EAhT7KMi"
COMPUTE_BUDGET_ID = "ComputeBudget111111111111111111111111111111"
SECP256K1_ID = "KeccakSecp256k11111111111111111111111111111"
SWITCHBOARD_IDS = {
    "mainnet": "SBondMDrcV3K4kxZR1HNVT7osZxAHVHgYXL5Ze1oMUv",
    "devnet": "Aio4gaXjXzJNVLtzwtNVmSqGKpANtXhybbkhtAC94ji2",
}


def b58encode(data: bytes) -> str:
    zeros = len(data) - len(data.lstrip(b"\x00"))
    num, out = int.from_bytes(data, "big"), []
    while num:
        num, rem = divmod(num, 58)
        out.append(B58[rem])
    return "1" * zeros + "".join(reversed(out))


def read_u16(buf: bytes, pos: int) -> tuple[int, int]:
    val, shift = 0, 0
    for _ in range(3):
        b = buf[pos]
        pos += 1
        val |= (b & 0x7F) << shift
        if not (b & 0x80):
            return val, pos
        shift += 7
    raise ValueError("Invalid compact-u16")


def verify_tx(tx_b64: str, action: str, wallet: str, network: str, fee_payer: str | None, task_pda: str | None) -> dict:
    raw = base64.b64decode(tx_b64.strip())
    num_sigs, pos = read_u16(raw, 0)
    pos += 64 * num_sigs
    is_v0 = bool(raw[pos] & 0x80)
    if is_v0:
        if (raw[pos] & 0x7F) != 0:
            raise ValueError("Unsupported transaction version")
        pos += 1
    num_req_sigs = raw[pos]
    pos += 3
    num_keys, pos = read_u16(raw, pos)
    keys = [b58encode(raw[pos + i * 32 : pos + (i + 1) * 32]) for i in range(num_keys)]
    pos += num_keys * 32 + 32  # skip static keys + 32-byte recent blockhash
    num_ixs, pos = read_u16(raw, pos)
    ixs = []
    for _ in range(num_ixs):
        prog_id = keys[raw[pos]]
        accts_len, pos = read_u16(raw, pos + 1)
        accts = [keys[i] for i in raw[pos : pos + accts_len]]
        data_len, pos = read_u16(raw, pos + accts_len)
        ixs.append((prog_id, accts, raw[pos : pos + data_len]))
        pos += data_len
    if is_v0:
        num_alts, pos = read_u16(raw, pos)
        if num_alts != 0:
            raise ValueError("Address lookup tables are forbidden")
    if pos != len(raw):
        raise ValueError("Unexpected trailing bytes in wire transaction")

    signers = keys[:num_req_sigs]
    if wallet not in signers:
        raise ValueError("Wallet is not a required signer")
    if fee_payer and keys[0] != fee_payer:
        raise ValueError(f"Fee payer {keys[0]} differs from expected {fee_payer}")

    allowed = {SHILLBOT_ID, COMPUTE_BUDGET_ID} | ({SWITCHBOARD_IDS[network], SECP256K1_ID} if action == "verify" else set())
    for prog_id, _, _ in ixs:
        if prog_id not in allowed:
            raise ValueError(f"Unexpected program {prog_id}")

    ix_name = "submit_work" if action == "submit" else f"{action}_task"
    expected_disc = hashlib.sha256(f"global:{ix_name}".encode()).digest()[:8]
    shillbot_ixs = [(accts, data) for prog_id, accts, data in ixs if prog_id == SHILLBOT_ID]
    if len(shillbot_ixs) != 1 or shillbot_ixs[0][1][:8] != expected_disc:
        raise ValueError(f"Expected single Shillbot {ix_name} instruction without unexpected companions")
    if task_pda and shillbot_ixs[0][0][1 if action == "create" else 0] != task_pda:
        raise ValueError("Task PDA differs from expected")
    return {"ok": True, "action": action, "fee_payer": keys[0], "signers": signers}


def main() -> int:
    p = argparse.ArgumentParser(description="Verify unsigned Shillbot Solana transaction")
    p.add_argument("--action", required=True, choices=["create", "claim", "submit", "approve", "verify", "finalize"])
    p.add_argument("--wallet", required=True)
    p.add_argument("--fee-payer")
    p.add_argument("--task-pda")
    p.add_argument("--network", default="mainnet", choices=["mainnet", "devnet"])
    p.add_argument("--tx")
    args = p.parse_args()
    try:
        res = verify_tx(args.tx or sys.stdin.read(), args.action, args.wallet, args.network, args.fee_payer, args.task_pda)
        print(json.dumps(res))
        return 0
    except Exception as exc:
        print(json.dumps({"ok": False, "error": str(exc)}))
        return 1


if __name__ == "__main__":
    sys.exit(main())
