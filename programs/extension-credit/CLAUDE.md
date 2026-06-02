# Extension-Credit — Program Context

The **permissionless funding layer** for the credit web. Any backer can front
working capital to an agent they've extended to and be recouped, earnings-first,
from the agent's routed Shillbot payouts. Inherits all root `CLAUDE.md` standards.

Program ID: `GJLUpJHceGekHBeZMZX4ZYX4xdkK4kFw2tH6uRuQHDqm`.

## Key idea: the Advance PDA **is** the splitter vault

There is no separate vault account. The `Advance` PDA holds the routed earnings
(the agent's `payout_to` in shillbot — C2 — points at it), on top of its
rent-exempt minimum. `route_and_recoup` splits everything above the rent floor.

## Lifecycle

```
   open_advance(amount)
   (no account) ─────────────────────────▶ ACTIVE  (Advance PDA = vault)
   backer ── amount ──▶ recipient wallet        │
   backer ── rent  ──▶ Advance PDA              │
                                                │ shillbot payout_to = Advance PDA;
                                                │ finalized earnings accrue on it
                                                │
                                                │  route_and_recoup  (permissionless, repeatable)
                                                │    available = balance − rent_floor
                                                │    backer    ◀── min(available, outstanding)
                                                │    recipient ◀── remainder
                                                │
                                                ├─ close_advance (backer; outstanding==0 & drained)
                                                │     rent ──▶ backer                  (recouped)
                                                │
                                                └─ mark_default (backer)
                                                      available ──▶ backer (partial)
                                                      rent      ──▶ backer             (defaulted)
```

## PDA seeds

- `Advance`: `["advance", backer, recipient]` — one active advance per
  (backer, recipient). `init` (not `init_if_needed`) prevents resurrection while
  active; after close/default the pair can advance again.

## Immutable invariants

- **`init`, never `init_if_needed`** — the PDA holds routed funds.
- **Recoupment waterfall:** the backer is paid first, up to the outstanding
  advance (`advance_lamports − recouped_lamports`); the recipient gets only the
  remainder. `recouped_lamports` is recorded (effect) before any transfer (CEI).
- **Rent-floor accounting:** "available" earnings = `balance − Rent::minimum_balance(data_len)`.
  The fronted principal lives in the recipient's wallet, not on the PDA, so the
  PDA balance is exactly rent + accrued earnings.
- **Conservation:** every lamport routed to the vault is distributed to backer +
  recipient (crank) or swept to the backer (default); only the rent moves on close.
- **Permissionless crank:** `route_and_recoup` takes no signer — anyone can
  settle. backer/recipient accounts are bound to the advance by the PDA seeds.
- **CEI** throughout; `close` runs at instruction exit after any manual sweep.

## Instructions

- `open_advance(advance_lamports)` — front capital to the recipient; open the advance/vault.
- `route_and_recoup()` — permissionless split of accrued earnings (backer-first waterfall).
- `close_advance()` — backer closes a fully-recouped, drained advance (rent back).
- `mark_default()` — backer sweeps the vault as partial recoupment and closes (eats the rest).

## Testing requirements

- Unit: `Advance::SPACE` (113) guard.
- Anchor lifecycle (localnet, in CI): open → (simulate routed earnings by airdropping the
  vault) → route_and_recoup (assert backer recouped first, recipient remainder, conservation) →
  close; plus open → mark_default (backer sweeps); plus error paths (advance too low, self-advance,
  close-before-recouped). Each asserts lamport conservation.

## Known limitations / deferred

- **Earnings leakage:** an agent can earn under a different identity to dodge the
  split — mitigated by small, fast-recouped advances, not cryptographically.
- One advance per (backer, recipient) at a time; multi-advance / premium-yield deferred.
- SOL only (advance + vault). USDC SPL vault is a later option.
- `mark_default` is backer-arbitrated (the defrauded party decides); third-party / authority
  default is deferred.
