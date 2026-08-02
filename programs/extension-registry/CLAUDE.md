# Extension-Registry — Program Context

The on-chain edge log for the extension-credit reputation graph (*mund-creanc-witer*). An **extension** is a bonded, obligation-creating vouch: an extender locks a SOL bond and records that a recipient owes return-substance. Inherits all root `CLAUDE.md` standards (11 Anchor rules, CEI, `checked_*`, named errors, events).

Program ID: `H7whziapWzGDH1b3QQzxno69TD4braekyBZhfjNGof4j`.

## What this owns vs. doesn't

- **On-chain:** holds the bond, enforces the obligation lifecycle, emits the edge events. Nothing else.
- **Off-chain:** the web-position score is computed from the emitted events by the web-position indexer (services), anchored to the single trust root. This program does NOT compute reputation.

## State machine (the `Extension` account is ephemeral)

The `Extension` PDA exists **only while the obligation is active**. Both terminal transitions close it. The durable graph is the event stream, so a closed account loses nothing the indexer needs.

```
                       submit_extension
   (no account) ───────────────────────────▶  ACTIVE
   bond: extender ──▶ PDA                        │
                                                 │  withdraw_extension
                                                 ├───────────────────────────▶ closed
                                                 │     bond + rent ──▶ extender   (fulfilled)
                                                 │     emit ExtensionWithdrawn
                                                 │
                                                 │  default_extension (authority-gated)
                                                 └───────────────────────────▶ closed
                                                       bond ──▶ treasury            (defaulted)
                                                       rent ──▶ extender
                                                       emit ExtensionDefaulted
```

## PDA seeds

- `GlobalState`: `["extension_global"]` — registry config (authority + treasury), set once by `initialize`.
- `Extension`: `["extension", extender, recipient]` — one active extension per (extender, recipient) pair. `init` (not `init_if_needed`) prevents resurrection while active; after a terminal close, the pair can extend again.

## Immutable invariants

- **`init`, never `init_if_needed`** — the account holds the bond (fund-holding ⇒ resurrection-attack prevention; root rule).
- **Lamport conservation:** the bond is either returned in full to the extender (attest) or slashed in full to the treasury (default); the rent always returns to the extender. No lamports are created or stranded. Asserted in the anchor lifecycle tests.
- **Bond floor:** `bond_lamports >= MIN_BOND_LAMPORTS`.
- **Type gate (MVP):** only `CapabilityValidation` (type `0`) is accepted; the taxonomy (`constants.rs`) is reserved for later verifiability tiers (sybil-layer 6).
- **No self-extension:** `extender != recipient`.
- **Authority-gated default:** only `GlobalState.authority` can call `default_extension`, and the slashed bond must go to `GlobalState.treasury`. Both are set once by `initialize` (the deployer calls `initialize(root, root)` for the MVP). Third-party default reporting + authority rotation are deferred.
- **CEI:** checks → (effects) → interactions; the account `close` runs at instruction exit, after any manual `transfer_lamports`.

## Instructions

- `initialize(authority, treasury)` — one-time registry config (`GlobalState`).
- `submit_extension(extension_type, bond_lamports)` — validate → record → move bond into the PDA.
- `withdraw_extension()` — extender withdraws; `close = extender` returns bond + rent.
  (Replaced `attest_return_substance`, which emitted a "recipient attested" claim the
  program could not actually verify.)
- `default_extension()` — authority slashes the bond to treasury (`transfer_lamports`), then `close = extender` returns the rent.

## Testing requirements

- Unit: `Extension::SPACE` (106) guard.
- Anchor lifecycle (localnet, in CI): submit → withdraw (bond returned), submit → default (bond slashed to treasury, rent to extender), plus every error path (wrong type, bond too low, self-extension, non-authority default, wrong treasury/recipient/extender). **Each path asserts lamport conservation** (Σin == Σout). This is the CI-gated money-safety check.

## Known limitations / deferred

- `GlobalState` is set once and authority/treasury cannot yet be rotated — add an `update_authority` ix + governance later.
- Self-reported / third-party default is deferred; MVP is authority-arbitrated.
- Bonds are SOL (lamport vault). A USDC SPL-token vault (stable bond pricing) is a later option.
- Only `CapabilityValidation` extensions; the other taxonomy types gate on verifiability handling.
