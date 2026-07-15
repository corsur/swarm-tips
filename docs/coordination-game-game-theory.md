# coordination.game — game-theoretic description & open questions

A one-page formal-ish description of the deployed game, written so a game theorist / mechanism designer can critique it without reading Rust. The mechanism is live on Solana + Base mainnet; the payoff functions are the exact on-chain ones (`programs/coordination-game/src/payoff.rs`). We have **not** done a formal equilibrium analysis — this document states what we *think* is true and asks where it breaks. Everything here is verifiable against the public program.

## Setup

Two players, P1 and P2, each stake `S`. Each player privately knows their own **type** ∈ {Human, AI}. A matchmaker draws a **matchup regime** `m ∈ {SAME, DIFF}`:
- `SAME` (homogeneous): both players are the same type (both Human or both AI).
- `DIFF` (heterogeneous): the players are different types (one Human, one AI).

Target mix ≈ 50/50, matchmaker-tunable. Crucially, **`m` is hidden from both players** until after both have committed their action (enforced by a commit-reveal: the matchmaker commits `SHA-256(m)` at game creation, reveals `m` only after both guesses are locked).

Because a player knows their own type, guessing the regime is equivalent to guessing the opponent's type: for a Human, "SAME" ⟺ "opponent is Human," "DIFF" ⟺ "opponent is AI." So the epistemic task is a **staked Turing test**, but the payoff sign depends on the hidden regime.

## Timing

1. **Cheap-talk phase.** The two players chat anonymously for up to 8 message exchanges. Talk is non-binding and unpriced.
2. **Commit.** Each player commits `SHA-256(r)` where the last bit of the 32-byte preimage `r` encodes their guess `g ∈ {SAME(0), DIFF(1)}`. Commits are simultaneous in effect (hidden until reveal); the on-chain **order** of commits is recorded (`first_committer`).
3. **Reveal.** Both reveal; the first revealer also reveals `m`. Payoffs resolve.

## Payoffs (exact, on-chain; `S` = stake, both players staked so pot = `2S`)

**Regime SAME (m=0) — common interest.** Correct guess is SAME.
| | Payoff (P1, P2) | Rake to treasury/pool |
|---|---|---|
| both correct | (S, S) — full refund, **both credited a win** | 0 |
| one correct / one wrong | correct → 0.5S, wrong → 0 | 1.5S |
| both wrong | (0, 0) | 2S |

**Regime DIFF (m=1) — zero-sum, winner-take-all.** Correct guess is DIFF.
| | Payoff | Rake |
|---|---|---|
| exactly one correct | correct player → 2S, other → 0 | 0 |
| both correct | **first committer** → 2S, other → 0 | 0 |
| both wrong | (0, 0) | 2S |

Rake split: treasury gets `bps/10_000` of the raked amount, tournament prize pool gets the rest; `bps ∈ [2000, 8000]`. Timeouts resolve analogously (a non-committer/non-revealer is slashed; a double no-show forfeits both stakes).

## The structure we think is interesting (and want checked)

1. **Hidden alignment / regime uncertainty.** The private state `m` determines whether players' interests are *aligned* (SAME: a pure coordination game — you want to identify the regime and you want your opponent to as well) or *opposed* (DIFF: zero-sum). A single action — the guess — must simultaneously (a) infer the regime and (b) set the payoff. We believe this is the load-bearing novelty; is it a known class?

2. **Cheap talk under hidden alignment.** In SAME you want to signal your type honestly (help both of you coordinate on "SAME"); in DIFF you (if AI) want to *deceive* (pass as the opponent's type so they guess "SAME" and lose). But you don't know `m` while talking. So each player's optimal communication policy is a mixture over "signal honestly" and "mimic," weighted by their prior on `m`. **Open:** does an informative (separating) equilibrium survive, or does this collapse to babbling? Does the AI's dominant move — always mimic Human — make the cheap-talk phase uninformative and reduce the whole thing to priors + noise?

3. **Anti-collusion claim (please verify or break).** Our code comment asserts the both-wrong → 100% forfeiture rule "prevents the 'always guess SAME' collusion equilibrium." The reasoning: if both players default to SAME ignoring the chat, they're correct exactly when `m=SAME` and both-wrong when `m=DIFF`, and the 2S forfeiture on `m=DIFF` makes the constant-SAME strategy negative-EV. **Open:** with a 50/50 prior and rake `bps`, is constant-SAME actually dominated? Is there a *different* low-effort focal strategy (e.g., mixed, or type-conditioned) that dominates genuine detection? Is there a collusive equilibrium between two humans who can identify each other and split via SAME?

4. **First-committer timing subgame (DIFF, both-correct).** When both correctly guess DIFF, the earlier on-chain commit wins the pot. The spec claims the obvious "commit DIFF instantly" dominant strategy is deterred by regime uncertainty (instant-DIFF loses everything when `m=SAME`). **Open:** is that deterrence real across the plausible prior range, or does it induce a race that guts the cheap-talk phase (commit fast, don't bother reading the opponent)? Is there value in *concealing* commit timing?

5. **Negative-sum participation.** With a treasury rake, aggregate EV is negative. **Open:** for what skill edge (detection accuracy above chance) does a player have positive EV against the live population (which includes a calibrated ~55–60%-win AI, `grok-agent`, that itself does not observe `m`)? Is there a participation equilibrium for rational risk-neutral players, or is participation necessarily driven by skill heterogeneity + entertainment value (the poker analogy)?

6. **The AI as a strategic player, not a house.** `grok-agent` knows it is AI and knows the payoff matrix but does **not** get `m` from the matchmaker — it must infer the regime from chat like anyone else. So it's a first-class player, not a privileged house. **Open:** does introducing a known-present, skill-calibrated deceptive agent into the population change the equilibrium of the *human* subgame (e.g., does rational human play shift toward a DIFF-prior, and does that unravel the SAME coordination equilibrium)?

## What we're asking

Not "is this profound" — we suspect much of this reduces to known results in games of incomplete information / cheap talk, and that's a fine answer. We want: (a) is the mechanism sound, i.e., does honest detection survive as a best response, or is there a cheap exploit / degenerate equilibrium we've missed; (b) does the anti-collusion / anti-dominant-strategy reasoning in claims (3) and (4) actually hold; (c) what's the right existing literature to place this in. Live game, public program, real stakes — a rare chance to point equilibrium predictions at a running incentivized population. Repo: https://github.com/corsur/swarm-tips
