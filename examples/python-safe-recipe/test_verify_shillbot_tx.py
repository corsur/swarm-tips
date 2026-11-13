#!/usr/bin/env python3
"""Unit tests for standalone fail-closed verify_shillbot_tx.py (Phase 5.1)."""
from __future__ import annotations

import base64
import io
import json
import struct
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))

import verify_shillbot_tx as vst  # noqa: E402


def encode_u16(val: int) -> bytes:
    out = bytearray()
    rem = val
    while True:
        elem = rem & 0x7F
        rem >>= 7
        if rem == 0:
            out.append(elem)
            return bytes(out)
        out.append(elem | 0x80)


def pubkey_from_seed(seed: int) -> str:
    raw = hashlib.sha256(f"test-pubkey-{seed}".encode("utf-8")).digest()
    return vst.b58encode(raw)


import hashlib  # noqa: E402


def build_wire_tx(
    *,
    signers: list[str],
    readonly_signers: int = 0,
    non_signers: list[str] | None = None,
    readonly_unsigned: int = 0,
    instructions: list[tuple[str, list[str], bytes]],
    v0: bool = False,
    num_alts: int = 0,
) -> str:
    """Construct a minimal valid Solana legacy or v0 wire transaction in base64."""
    acct_keys: list[str] = []
    for k in signers:
        if k not in acct_keys:
            acct_keys.append(k)
    for k in non_signers or []:
        if k not in acct_keys:
            acct_keys.append(k)
    for prog_id, ix_accts, _ in instructions:
        for a in ix_accts:
            if a not in acct_keys:
                acct_keys.append(a)
        if prog_id not in acct_keys:
            acct_keys.append(prog_id)

    num_req_sigs = len(signers)
    sigs_bytes = encode_u16(num_req_sigs) + (b"\x00" * 64 * num_req_sigs)

    prefix = b"\x80" if v0 else b""
    header = bytes([num_req_sigs, readonly_signers, readonly_unsigned])
    keys_bytes = encode_u16(len(acct_keys)) + b"".join(vst.b58decode(k) for k in acct_keys)
    blockhash = b"\x11" * 32

    ix_chunks: list[bytes] = []
    for prog_id, ix_accts, data in instructions:
        prog_idx = acct_keys.index(prog_id)
        acct_indices = bytes(acct_keys.index(a) for a in ix_accts)
        ix_chunks.append(
            bytes([prog_idx])
            + encode_u16(len(acct_indices))
            + acct_indices
            + encode_u16(len(data))
            + data
        )
    ixs_bytes = encode_u16(len(ix_chunks)) + b"".join(ix_chunks)
    alt_bytes = encode_u16(num_alts) if v0 else b""
    raw = sigs_bytes + prefix + header + keys_bytes + blockhash + ixs_bytes + alt_bytes
    return base64.b64encode(raw).decode("ascii")


def cb_heap_frame(heap_bytes: int) -> tuple[str, list[str], bytes]:
    return (vst.COMPUTE_BUDGET_ID, [], b"\x01" + struct.pack("<I", heap_bytes))


def cb_unit_limit(units: int) -> tuple[str, list[str], bytes]:
    return (vst.COMPUTE_BUDGET_ID, [], b"\x02" + struct.pack("<I", units))


def cb_unit_price(micro_lamports: int) -> tuple[str, list[str], bytes]:
    return (vst.COMPUTE_BUDGET_ID, [], b"\x03" + struct.pack("<Q", micro_lamports))


def make_claim_ix(task: str, agent: str) -> tuple[str, list[str], bytes]:
    accts = [
        task,
        pubkey_from_seed(101),  # global_state
        pubkey_from_seed(102),  # agent_state
        agent,                  # [3] agent signer
        pubkey_from_seed(103),  # system_program
    ]
    return (vst.SHILLBOT_ID, accts, vst.anchor_disc("claim_task"))


def make_claim_session_ix(
    task: str, agent_wallet: str, delegate: str, payer: str
) -> tuple[str, list[str], bytes]:
    accts = [
        task,
        pubkey_from_seed(101),  # global_state
        pubkey_from_seed(102),  # agent_state
        agent_wallet,           # [3] agent_wallet (non-signer)
        delegate,               # [4] delegate signer
        payer,                  # [5] payer signer
        pubkey_from_seed(103),  # system_program
    ]
    return (vst.SHILLBOT_ID, accts, vst.anchor_disc("claim_task_session"))


def make_set_payout_ix(
    task: str, agent: str, payout_dest: str, extra_accts: list[str] | None = None
) -> tuple[str, list[str], bytes]:
    accts = [task, agent, *(extra_accts or [])]
    data = vst.anchor_disc("set_payout_to") + vst.b58decode(payout_dest)
    return (vst.SHILLBOT_ID, accts, data)


def make_submit_ix(task: str, agent: str, content_id: str = "proof-1") -> tuple[str, list[str], bytes]:
    encoded = content_id.encode("utf-8")
    data = vst.anchor_disc("submit_work") + struct.pack("<I", len(encoded)) + encoded
    accts = [
        task,
        pubkey_from_seed(101),  # global_state
        pubkey_from_seed(102),  # agent_state
        agent,                  # [3] agent signer
    ]
    return (vst.SHILLBOT_ID, accts, data)


def make_submit_session_ix(
    task: str, agent_wallet: str, delegate: str, content_id: str = "proof-1"
) -> tuple[str, list[str], bytes]:
    encoded = content_id.encode("utf-8")
    data = vst.anchor_disc("submit_work_session") + struct.pack("<I", len(encoded)) + encoded
    accts = [
        task,
        pubkey_from_seed(101),  # global_state
        pubkey_from_seed(102),  # agent_state
        agent_wallet,           # [3] agent_wallet
        delegate,               # [4] delegate signer
    ]
    return (vst.SHILLBOT_ID, accts, data)


def make_create_ix(task: str, client: str, escrow_lamports: int = 50_000_000) -> tuple[str, list[str], bytes]:
    # [disc:8][task_id:8][escrow_lamports:8][content_hash:32][platform:1][deadline:8][req_id:1]
    data = (
        vst.anchor_disc("create_task")
        + struct.pack("<QQ", 42, escrow_lamports)
        + (b"\x22" * 32)
        + struct.pack("<BqB", 1, 1_800_000_000, 0)
    )
    accts = [
        pubkey_from_seed(101),  # [0] global_state
        task,                   # [1] task
        pubkey_from_seed(104),  # [2] escrow
        client,                 # [3] client signer
        pubkey_from_seed(103),  # [4] system_program
        pubkey_from_seed(105),  # [5] identity_record (optional account slot)
    ]
    return (vst.SHILLBOT_ID, accts, data)


def make_approve_ix(task: str, client: str) -> tuple[str, list[str], bytes]:
    return (vst.SHILLBOT_ID, [task, client], vst.anchor_disc("approve_task"))


def make_finalize_ix(task: str, recipient: str) -> tuple[str, list[str], bytes]:
    accts = [
        task,
        pubkey_from_seed(104),  # [1] escrow
        recipient,              # [2] payout recipient
        pubkey_from_seed(106),  # [3] treasury
        pubkey_from_seed(101),  # [4] global_state
    ]
    return (vst.SHILLBOT_ID, accts, vst.anchor_disc("finalize_task"))


def make_verify_attested_ix(task: str, attester: str) -> tuple[str, list[str], bytes]:
    data = vst.anchor_disc("verify_task_attested") + struct.pack("<Q", 9_000) + (b"\x33" * 32)
    accts = [
        task,
        pubkey_from_seed(101),  # [1] global_state
        attester,               # [2] attester signer
    ]
    return (vst.SHILLBOT_ID, accts, data)


class VerifyShillbotTxHardeningTests(unittest.TestCase):
    def setUp(self) -> None:
        self.wallet = pubkey_from_seed(1)
        self.attacker = pubkey_from_seed(2)
        self.payout = pubkey_from_seed(3)
        self.task = pubkey_from_seed(4)
        self.sponsor = pubkey_from_seed(5)

    def test_rejects_unexpected_set_payout_to_hijack(self) -> None:
        claim_with_hijack = build_wire_tx(
            signers=[self.wallet],
            instructions=[
                make_claim_ix(self.task, self.wallet),
                make_set_payout_ix(self.task, self.wallet, self.attacker),
            ],
        )
        with self.assertRaisesRegex(ValueError, "without explicit --payout-to"):
            vst.verify_tx(claim_with_hijack, "claim", self.wallet, task_pda=self.task)

        # Even if caller passes --payout-to, mismatched destination bytes must fail closed
        with self.assertRaisesRegex(ValueError, "destination pubkey does not match"):
            vst.verify_tx(
                claim_with_hijack,
                "claim",
                self.wallet,
                task_pda=self.task,
                payout_to=self.payout,
            )

        # Valid explicit payout_to succeeds
        claim_with_legit_payout = build_wire_tx(
            signers=[self.wallet],
            instructions=[
                make_claim_ix(self.task, self.wallet),
                make_set_payout_ix(self.task, self.wallet, self.payout),
            ],
        )
        res = vst.verify_tx(
            claim_with_legit_payout,
            "claim",
            self.wallet,
            task_pda=self.task,
            payout_to=self.payout,
        )
        self.assertTrue(res["ok"])

        # set_payout_to injected alongside finalize_task is ALWAYS rejected
        finalize_with_hijack = build_wire_tx(
            signers=[self.wallet],
            instructions=[
                make_finalize_ix(self.task, self.payout),
                make_set_payout_ix(self.task, self.wallet, self.payout),
            ],
        )
        with self.assertRaisesRegex(ValueError, "Unexpected set_payout_to"):
            vst.verify_tx(
                finalize_with_hijack,
                "finalize",
                self.wallet,
                task_pda=self.task,
                payout_to=self.payout,
            )

    def test_rejects_set_payout_to_wrong_authority_or_extra_accounts_or_zero_pubkey(self) -> None:
        # Attacker at accounts[1] while victim wallet is fee_payer signer
        wrong_auth_tx = build_wire_tx(
            signers=[self.wallet, self.attacker],
            instructions=[
                make_claim_ix(self.task, self.wallet),
                make_set_payout_ix(self.task, self.attacker, self.payout),
            ],
        )
        with self.assertRaisesRegex(ValueError, r"set_payout_to authority slot \[1\]"):
            vst.verify_tx(
                wrong_auth_tx,
                "claim",
                self.wallet,
                task_pda=self.task,
                payout_to=self.payout,
            )

        # Extra trailing account on set_payout_to (must be exact 2 accounts)
        extra_acct_tx = build_wire_tx(
            signers=[self.wallet],
            instructions=[
                make_claim_ix(self.task, self.wallet),
                make_set_payout_ix(self.task, self.wallet, self.payout, [self.attacker]),
            ],
        )
        with self.assertRaisesRegex(ValueError, "exact 2 accounts"):
            vst.verify_tx(
                extra_acct_tx,
                "claim",
                self.wallet,
                task_pda=self.task,
                payout_to=self.payout,
            )

        # Zero pubkey as payout_to is rejected
        zero_pubkey = "1" * 32
        with self.assertRaisesRegex(ValueError, "zero pubkey"):
            vst.verify_tx(
                extra_acct_tx,
                "claim",
                self.wallet,
                task_pda=self.task,
                payout_to=zero_pubkey,
            )

        # Task PDA as payout_to is rejected
        same_as_task_tx = build_wire_tx(
            signers=[self.wallet],
            instructions=[
                make_claim_ix(self.task, self.wallet),
                make_set_payout_ix(self.task, self.wallet, self.task),
            ],
        )
        with self.assertRaisesRegex(ValueError, "cannot equal the task PDA"):
            vst.verify_tx(
                same_as_task_tx,
                "claim",
                self.wallet,
                task_pda=self.task,
                payout_to=self.task,
            )

    def test_compute_budget_heap_frame_does_not_bypass_price_cap(self) -> None:
        # RequestHeapFrame (tag 1) followed by excessive SetComputeUnitPrice (tag 3) MUST fail!
        bypass_attempt = build_wire_tx(
            signers=[self.wallet],
            instructions=[
                cb_heap_frame(64 * 1024),
                cb_unit_price(500_000),  # > 100,000 micro-lamports cap
                make_claim_ix(self.task, self.wallet),
            ],
        )
        with self.assertRaisesRegex(ValueError, "exceeds max"):
            vst.verify_tx(bypass_attempt, "claim", self.wallet, task_pda=self.task)

        # Valid RequestHeapFrame + bounded SetComputeUnitLimit + bounded SetComputeUnitPrice passes
        valid_cb = build_wire_tx(
            signers=[self.wallet],
            instructions=[
                cb_heap_frame(64 * 1024),
                cb_unit_limit(250_000),
                cb_unit_price(50_000),
                make_claim_ix(self.task, self.wallet),
            ],
        )
        res = vst.verify_tx(valid_cb, "claim", self.wallet, task_pda=self.task)
        self.assertTrue(res["ok"])

        # Duplicate ComputeBudget tag rejected
        dup_cb = build_wire_tx(
            signers=[self.wallet],
            instructions=[
                cb_unit_price(10_000),
                cb_unit_price(20_000),
                make_claim_ix(self.task, self.wallet),
            ],
        )
        with self.assertRaisesRegex(ValueError, "Duplicate ComputeBudget"):
            vst.verify_tx(dup_cb, "claim", self.wallet, task_pda=self.task)

        # Malformed RequestHeapFrame length (e.g., 9 bytes instead of 5) rejected
        malformed_heap = build_wire_tx(
            signers=[self.wallet],
            instructions=[
                (vst.COMPUTE_BUDGET_ID, [], b"\x01" + b"\x00" * 8),
                make_claim_ix(self.task, self.wallet),
            ],
        )
        with self.assertRaisesRegex(ValueError, "RequestHeapFrame length"):
            vst.verify_tx(malformed_heap, "claim", self.wallet, task_pda=self.task)

    def test_enforces_exact_authority_slot_indices(self) -> None:
        # Wallet is fee_payer (in signers!), but attacker is placed in the instruction authority slot!
        cases = [
            ("claim", make_claim_ix(self.task, self.attacker), r"authority slot \[3\]"),
            (
                "claim",
                make_claim_session_ix(self.task, pubkey_from_seed(99), self.attacker, self.wallet),
                r"authority slot \[4\]",
            ),
            ("submit", make_submit_ix(self.task, self.attacker), r"authority slot \[3\]"),
            (
                "submit",
                make_submit_session_ix(self.task, pubkey_from_seed(99), self.attacker),
                r"authority slot \[4\]",
            ),
            ("create", make_create_ix(self.task, self.attacker), r"authority slot \[3\]"),
            ("approve", make_approve_ix(self.task, self.attacker), r"authority slot \[1\]"),
        ]
        for action, bad_ix, expected_err in cases:
            with self.subTest(action=action, expected_err=expected_err):
                tx_b64 = build_wire_tx(
                    signers=[self.wallet, self.attacker],
                    instructions=[bad_ix],
                )
                with self.assertRaisesRegex(ValueError, expected_err):
                    vst.verify_tx(tx_b64, action, self.wallet, task_pda=self.task)

        # Positive session delegation check: wallet is delegate at [4], sponsor is fee_payer at [5]
        valid_session_claim = build_wire_tx(
            signers=[self.sponsor, self.wallet],
            instructions=[
                make_claim_session_ix(self.task, pubkey_from_seed(99), self.wallet, self.sponsor)
            ],
        )
        res = vst.verify_tx(
            valid_session_claim,
            "claim",
            self.wallet,
            fee_payer=self.sponsor,
            task_pda=self.task,
        )
        self.assertEqual(res["instruction"], "claim_task_session")

        # verify_task_attested: unsigned attester at [2] must fail even if wallet is fee_payer
        unsigned_attester_tx = build_wire_tx(
            signers=[self.wallet],
            non_signers=[self.attacker],
            instructions=[make_verify_attested_ix(self.task, self.attacker)],
        )
        with self.assertRaisesRegex(ValueError, r"attester slot \[2\] must be a signer"):
            vst.verify_tx(unsigned_attester_tx, "verify", self.wallet, task_pda=self.task)

        # verify_task_attested: mismatched oracle_authority must fail
        signed_wrong_attester_tx = build_wire_tx(
            signers=[self.wallet, self.attacker],
            instructions=[make_verify_attested_ix(self.task, self.attacker)],
        )
        with self.assertRaisesRegex(ValueError, "expected oracle_authority"):
            vst.verify_tx(
                signed_wrong_attester_tx,
                "verify",
                self.wallet,
                task_pda=self.task,
                oracle_authority=self.payout,
            )

    def test_cli_main_supports_payout_to_and_escrow_lamports(self) -> None:
        atomic_tx = build_wire_tx(
            signers=[self.wallet],
            instructions=[
                make_claim_ix(self.task, self.wallet),
                make_set_payout_ix(self.task, self.wallet, self.payout),
                make_submit_ix(self.task, self.wallet, "eip155:84532:0xabc:0"),
            ],
        )
        # Without --payout-to -> exit code 1
        out_fail = io.StringIO()
        with patch.object(
            sys,
            "argv",
            [
                "verify_shillbot_tx.py",
                "--action",
                "submit",
                "--wallet",
                self.wallet,
                "--task-pda",
                self.task,
                "--tx",
                atomic_tx,
            ],
        ), patch("sys.stdout", out_fail):
            code = vst.main()
        self.assertEqual(code, 1)
        self.assertFalse(json.loads(out_fail.getvalue())["ok"])

        # With matching --payout-to -> exit code 0
        out_ok = io.StringIO()
        with patch.object(
            sys,
            "argv",
            [
                "verify_shillbot_tx.py",
                "--action",
                "submit",
                "--wallet",
                self.wallet,
                "--task-pda",
                self.task,
                "--payout-to",
                self.payout,
                "--tx",
                atomic_tx,
            ],
        ), patch("sys.stdout", out_ok):
            code = vst.main()
        self.assertEqual(code, 0)
        self.assertTrue(json.loads(out_ok.getvalue())["ok"])


if __name__ == "__main__":
    unittest.main()
