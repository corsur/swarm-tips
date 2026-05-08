# Pre-launch checklist (sensitive write surfaces)

A six-item checklist that gates any new sensitive write surface from
landing on mainnet. Run through it as part of the PR description for
the change; ship the change only after all six items have evidence in
the repo.

**What counts as a "sensitive write surface"?** Apply the checklist to:

- A new on-chain instruction (e.g. `approve_task` from blocker #3a).
- A new parameter on `update_params` or a new admin instruction
  (`transfer_authority`, `update_treasury`, `update_oracle_authority`,
  ...).
- A new platform integration with credentials that hold real money
  flow (X, YouTube, Farcaster, ...).
- A new `/internal/*` endpoint or cron / workflow that mutates
  Firestore state material to the protocol (escrow, subsidy,
  challenge, agent reputation).
- A new orchestrator-side flag that operators flip during incidents
  (e.g. `subsidy_state/halted`, `paused_platforms`).

Do NOT apply to: routine bug fixes that don't change a contract, log
messages, internal-only refactors with no behavioral change, frontend
changes that don't introduce a new write path.

## How to use it

1. **PR template inclusion.** Paste the six-item block from this
   document into the PR description. Tick each item as evidence
   lands.
2. **Pre-merge sanity check.** Run `scripts/check-prelaunch.sh
   <pr-branch>` against the local branch — the script greps the
   diff for evidence of each item and prints a per-item summary.
   The script is advisory, not normative — items it can't see
   evidence for (e.g. devnet rehearsal — no in-repo artifact) it
   reports as "not detected; mark manually."
3. **Founder gate.** For SEV-1-eligible surfaces (anything that
   touches escrow or admin authority), the founder approves the
   PR after items 1–6 are visibly green.

## The six items

### 1. Threat model

A written threat model lives next to the change. For an on-chain
change, this is the relevant section in `programs/shillbot/CLAUDE.md`
or `swarm/shillbot/CLAUDE.md`. For an off-chain change, this is the
service's `CLAUDE.md` or a dedicated `docs/threat-models/<name>.md`.
The model names: who can call this surface, what they can break, what
the protocol's defense is, what the worst-case blast radius is, what
the recovery looks like.

A change without a threat model gets a "what's the worst that could
happen?" question that the author can't answer cleanly. That's the
fail.

### 2. State-machine impact

If the change touches a state machine (Task lifecycle, Campaign
lifecycle, Game phases, Verification pipeline), the corresponding
ASCII diagram in `programs/shillbot/CLAUDE.md` /
`coordination-app/backend/shillbot-orchestrator/CLAUDE.md` /
equivalent has been updated. The diagram MUST land in the same PR
as the code change, not as a follow-up.

This is the "no stale diagrams" rule from the umbrella `swarm/CLAUDE.md`
applied as a hard pre-launch gate.

### 3. Tests

Happy path + every failure mode have unit tests. For on-chain
instructions: the `tests/shillbot.ts` / `tests/shillbot-lifecycle.ts`
suites have positive coverage AND per-error-variant rejection
coverage. For off-chain: the relevant Rust crate's `#[cfg(test)]`
module covers happy path + every error path. For frontend: the
integration tests at `frontend/shillbot/__tests__/` cover the new
write path with both success and rejection mocks.

A change that ships with happy-path-only tests is a regression
landing waiting to happen — the negative paths are exactly where
production bugs surface (per the recurring "happy-path-only OR
negative-only test pattern" feedback theme across earlier
iterations).

### 4. Runbook entry

If the change introduces a new failure mode that an on-call
operator would be paged for, that mode is in
`coordination-app/backend/shillbot-orchestrator/RUNBOOK.md` with
the standard Signal / Triage / Response / Escalation shape. If
the change introduces a new multisig action (e.g., a new
`update_params` parameter), the action recipe is in the runbook's
"Multisig action recipes" section.

A change that adds a new write surface but leaves operators
without a runbook entry will get diagnosed mid-incident, which is
the worst time to be reverse-engineering the contract.

### 5. Devnet rehearsal

The change has run end-to-end on devnet, ideally with a small
audit trail (test transaction signatures, devnet log excerpts,
Firestore snapshots before/after). For multisig actions, the
multisig has rehearsed the full propose / approve / execute flow
on devnet at least once before its first mainnet use. The runbook
explicitly says:

> *"Every multisig action recipe should be rehearsed on devnet
> before its first mainnet use."*

This item is the one most often skipped under launch pressure.
Don't.

### 6. Public disclosure plan

If the change is user-facing (clients, agents, third parties
reading our wire formats):

- Customer-facing change → release-notes entry with what changed,
  what they need to do, what's backward-compatible.
- Wire-format change (AAS, SDK, REST API) → version bump per
  the format's versioning rules; the spec at `docs/specs/aas-v1.md`
  pins the AAS rules.
- Admin action → a one-line summary on shillbot.org's status page
  (or equivalent).

If the change is purely internal (a refactor that preserves wire
format and behavior): document this and check the box. Internal-
only is a valid disclosure outcome — but state it explicitly so
the next reviewer doesn't assume it was forgotten.

---

## PR template block (copy-paste into PR description)

```markdown
## Pre-launch checklist

- [ ] **1. Threat model.** Link or quote the relevant section.
- [ ] **2. State-machine impact.** Diagram updated in: <CLAUDE.md path>.
- [ ] **3. Tests.** Happy + every failure mode. Test file: <path>.
- [ ] **4. Runbook entry.** Failure mode / action recipe in: <RUNBOOK.md section>.
- [ ] **5. Devnet rehearsal.** TX signatures / evidence: <link>.
- [ ] **6. Public disclosure plan.** Channel: <release notes / spec bump / status page / internal-only>.
```

If an item is genuinely N/A for the change (e.g., item 4 for a
non-runbook-relevant frontend tweak), strike it through with a
one-line reason rather than ticking it without evidence:

```markdown
- [x] ~~**4. Runbook entry.**~~ N/A — change is frontend-only,
  no on-call paging path.
```

---

## Why six?

Five-item lists feel exhaustive enough that PRs ship without
finishing them. Seven-item lists feel like ceremony and get
trimmed informally. Six lands in the gap: enough items to cover
the load-bearing categories (model, contract, tests, ops,
verification, communication) without inviting "this is overkill"
pushback.

If you find yourself wanting a seventh item, ask: does this
collapse into one of the six (e.g., "load test" → item 3 tests)?
If not, write it down somewhere durable and revisit when 3+
changes have hit the same gap.
