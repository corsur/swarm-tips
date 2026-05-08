"""Update a single task's state field in Firestore.

Used by crank-stuck-shillbot-tasks.ts after a successful on-chain expire/finalize
or to reconcile Firestore for tasks already closed on-chain.

Usage: python3 reconcile-firestore-state.py <task_id> <new_state>
"""

import warnings
warnings.filterwarnings("ignore")
import sys
from datetime import datetime, timezone
from google.cloud import firestore

FIRESTORE_PROJECT = "coordination-game-prod"


def main() -> None:
    if len(sys.argv) != 3:
        print("usage: reconcile-firestore-state.py <task_id> <new_state>", file=sys.stderr)
        sys.exit(1)

    task_id = sys.argv[1]
    new_state = sys.argv[2]

    db = firestore.Client(project=FIRESTORE_PROJECT)
    ref = db.collection("tasks").document(task_id)
    ref.update({
        "state": new_state,
        "finalized_at": datetime.now(timezone.utc).isoformat(),
    })
    print(f"ok {task_id} → {new_state}")


if __name__ == "__main__":
    main()
