"""Outputs JSON list of stuck mainnet shillbot tasks + their on-chain truth.

Categories:
  needs_expire:  on-chain Submitted/Approved past T+14d
  needs_finalize: on-chain Verified past challenge_window
  fs_stale:       on-chain CLOSED but Firestore lagged
  recent:         too recent to act on yet
"""

import warnings
warnings.filterwarnings("ignore")
import json
import sys
import time
import base64
from datetime import datetime
import requests
from google.cloud import firestore

FIRESTORE_PROJECT = "coordination-game-prod"
RPC = "https://api.mainnet-beta.solana.com"
EXPIRE_TIMEOUT = 14 * 24 * 3600
CHALLENGE_WINDOW = 24 * 3600


def main() -> None:
    db = firestore.Client(project=FIRESTORE_PROJECT)

    docs = []
    addrs = []
    for t in db.collection("tasks").stream():
        d = t.to_dict()
        if (
            d.get("network") == "mainnet"
            and d.get("state") in ("submitted", "verified", "approved")
            and d.get("on_chain_address")
        ):
            docs.append((t.id, d))
            addrs.append(d["on_chain_address"])

    states_by_addr = {}
    for i in range(0, len(addrs), 100):
        batch = addrs[i : i + 100]
        r = requests.post(
            RPC,
            json={
                "jsonrpc": "2.0",
                "id": 1,
                "method": "getMultipleAccounts",
                "params": [batch, {"encoding": "base64"}],
            },
        ).json()
        for addr, acct in zip(batch, r["result"]["value"]):
            if acct is None:
                states_by_addr[addr] = (255, 0)
            else:
                data = base64.b64decode(acct["data"][0])
                state = data[80]
                escrow = int.from_bytes(data[82:90], "little")
                states_by_addr[addr] = (state, escrow)

    now = int(time.time())
    out = {"needs_expire": [], "needs_finalize": [], "fs_stale": [], "recent": []}

    for tid, d in docs:
        addr = d["on_chain_address"]
        onchain_state, escrow = states_by_addr[addr]
        sub = d.get("submitted_at")
        sub_ts = None
        if sub:
            try:
                s = str(sub).replace("Z", "+00:00")
                sub_ts = int(datetime.fromisoformat(s).timestamp())
            except Exception:
                sub_ts = None

        entry = {
            "task_id": tid,
            "on_chain_address": addr,
            "fs_state": d.get("state"),
            "platform": d.get("platform"),
            "submitted_at_ts": sub_ts,
            "on_chain_state": onchain_state,
            "on_chain_escrow": escrow,
        }

        if onchain_state == 255:
            out["fs_stale"].append(entry)
        elif onchain_state in (2, 7):  # Submitted, Approved
            if sub_ts and now - sub_ts > EXPIRE_TIMEOUT:
                out["needs_expire"].append(entry)
            else:
                out["recent"].append(entry)
        elif onchain_state == 3:  # Verified
            if sub_ts and now - sub_ts > CHALLENGE_WINDOW:
                out["needs_finalize"].append(entry)
            else:
                out["recent"].append(entry)
        else:
            out["recent"].append(entry)

    json.dump(out, sys.stdout)


if __name__ == "__main__":
    main()
