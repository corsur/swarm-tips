/**
 * EVM session setup (Level 2, wallet-as-player) — the Base-game analog of
 * `matchmaking-tx.ts` `setupSessionIfNeeded`. The main wallet's ONE popup is a
 * single `openSession(sessionKey, expiry)` contract call that BOTH registers the
 * session key as the wallet's on-chain delegate AND forwards `stake + gas` to it.
 * Afterwards the gas-only session key stakes/plays AS the wallet with zero
 * further popups, while the wallet stays the on-chain player and payout
 * recipient. No session→wallet binding is needed — the contract records the
 * wallet natively (see CoordinationGame v3 createGame/joinGame `player`).
 * Dependencies are injectable for tests.
 */

import {
  encodeFunctionData,
  type Address,
  type Chain,
  type Hex,
  type WalletClient,
} from "viem";
import { evmReadClient, evmSessionAddress } from "./session.js";
import { COORDINATION_GAME_ABI } from "./abi.js";

/**
 * Gas headroom funded on top of the stake: covers create/join + commit +
 * reveal + withdrawFor. CHAIN-AWARE: Ethereum L1 gas is ~10-100x Base L2's and
 * the session runs ~4 txs, so 0.0003 ETH — fine on Base — can't cover L1 when
 * gas ticks up. Budget 0.003 ETH on L1, keep 0.0003 on L2.
 */
export const EVM_SESSION_GAS_BUFFER_L2_WEI = 300_000_000_000_000n; // 0.0003 ETH
export const EVM_SESSION_GAS_BUFFER_L1_WEI = 3_000_000_000_000_000n; // 0.003 ETH

/** Ethereum L1 chains (higher gas) — everything else here is an L2 (Base).
 *  Ethereum Sepolia (eip155:11155111) was removed as a supported testnet; only
 *  Ethereum mainnet remains an L1 target. */
const EVM_L1_CAIP2 = new Set(["eip155:1"]);

/** The session gas buffer for a chain (L1 needs far more than L2). */
export function evmSessionGasBufferWei(caip2: string): bigint {
  return EVM_L1_CAIP2.has(caip2)
    ? EVM_SESSION_GAS_BUFFER_L1_WEI
    : EVM_SESSION_GAS_BUFFER_L2_WEI;
}

/** Session lifetime — matches the 24h sessionStorage TTL of the key itself. */
const SESSION_DURATION_SECS = 24 * 60 * 60;

/**
 * How much validity a session must have LEFT to be reusable.
 *
 * A session that expires mid-game is as bad as one already expired: the
 * commit or reveal reverts `BadSession` with the stake already locked. The
 * contract's own commit + reveal windows bound a game, so require comfortably
 * more than that rather than a token margin.
 */
export const SESSION_MIN_REMAINING_SECS = 60 * 60;

/**
 * Does an on-chain `sessions(wallet)` record authorize `sessionAddr` for long
 * enough to play? Pure, so the rule is testable without a chain.
 *
 * Mirrors the contract's `_actsFor` (`s.sessionKey == actor && now < s.expiry`)
 * and adds the remaining-life margin — the contract only has to answer "valid
 * right now", the client has to answer "valid for the whole game".
 *
 * Address comparison is case-insensitive: these are checksummed hex from two
 * different sources, and a raw `===` on checksummed addresses is exactly the
 * bug that made the e2e harness assert against the wrong player.
 */
export function evmSessionRecordIsLive(
  record: { sessionKey: string; expiry: bigint },
  sessionAddr: string,
  nowSecs: number,
  minRemainingSecs = SESSION_MIN_REMAINING_SECS,
): boolean {
  if (record.sessionKey.toLowerCase() !== sessionAddr.toLowerCase()) return false;
  return record.expiry > BigInt(nowSecs + minRemainingSecs);
}

/** Read `sessions(wallet)` and apply {@link evmSessionRecordIsLive}. */
async function evmSessionIsLive(
  read: Pick<ReturnType<typeof evmReadClient>, "readContract">,
  contract: Address,
  wallet: Address,
  sessionAddr: Address,
  nowSecs: number,
): Promise<boolean> {
  try {
    const res = (await read.readContract({
      address: contract,
      abi: COORDINATION_GAME_ABI,
      functionName: "sessions",
      args: [wallet],
    })) as readonly [string, bigint];
    return evmSessionRecordIsLive(
      { sessionKey: res[0], expiry: res[1] },
      sessionAddr,
      nowSecs,
    );
  } catch (e) {
    // An unreadable registration is NOT proof of a live session. Fall through
    // to re-opening: the cost is one extra popup, whereas assuming "live" is
    // the lockout this whole change exists to remove. Logged, not swallowed.
    console.warn(
      "[evm-session-tx] could not read sessions(); assuming NOT live and reopening",
      e,
    );
    return false;
  }
}

export interface EvmSessionFundDeps {
  /** Read client for the SELECTED chain (its RPCs/chain must match the chain
   *  being funded). The caller builds it via `evmReadClient(chain, rpcs)`. */
  readClient: Pick<
    ReturnType<typeof evmReadClient>,
    "getBalance" | "waitForTransactionReceipt" | "readContract"
  >;
  /** The SELECTED chain the tx must land on. Passed explicitly because the
   *  wagmi wallet client is bound to the chain the wallet was on at CONNECT
   *  time — after fund()'s switchChainAsync, `mainWalletClient.chain` is stale
   *  (observed live: wallet connected on Ethereum, switched to Base, tx then
   *  built with chain=Ethereum and rejected by viem's chain-mismatch guard). */
  chain: Chain;
  /** Injectable "now" (unix secs) for deterministic expiry in tests. */
  nowSecs?: () => number;
}

/**
 * Ensure the session key is registered on `contract` as `mainAddress`'s delegate
 * and holds at least `stakeWei + gas buffer`, via a single wallet-signed
 * `openSession` call (the one popup). Returns the openSession tx hash, or null
 * when the session is genuinely reusable.
 *
 * "Reusable" means BOTH funded and still-valid on-chain, and it used to mean
 * only the first. The old rule was `balance >= need -> return null`, justified
 * as "funding always goes through openSession, so a funded session is a
 * registered one". That inference held until `openSession` gained an expiry:
 * `_actsFor` requires `block.timestamp < s.expiry`, and balance says nothing
 * about time. A wallet whose session key kept its balance past 24h — funded but
 * never spent, or a WINNER, since winnings credit the wallet rather than the
 * session key — had every createGame/joinGame revert `BadSession`, and could
 * never recover, because the balance check kept reporting "already set up".
 * Observed live on Base Sepolia: session expired 17.7h earlier, key still
 * holding 0.0064 ETH, every game creation reverting.
 *
 * So the on-chain registration is now read directly. Balance evidences funding;
 * only `sessions(mainAddress)` evidences authority.
 */
export async function setupEvmSessionIfNeeded(
  mainWalletClient: WalletClient,
  sessionKey: Hex,
  contract: Address,
  stakeWei: bigint,
  caip2: string,
  deps: EvmSessionFundDeps,
): Promise<Hex | null> {
  const read = deps.readClient;
  const now = deps.nowSecs ? deps.nowSecs() : Math.floor(Date.now() / 1000);

  const sessionAddr = evmSessionAddress(sessionKey);
  // GAS ONLY. `need` is what the SESSION KEY must hold, and since v6 the stake
  // no longer lives there — it is escrowed to the player's `withdrawable`
  // ledger by openSessionAndDeposit and debited by createGame/joinGame with
  // msg.value == 0. Every skip/top-up/visibility check below keys off `need`,
  // so they all follow. Leaving the stake here was the custody bug the escrow
  // work exists to close: a lost or expired session key took it, unrecoverably.
  const need = evmSessionGasBufferWei(caip2);
  // `pending`, not the default `latest`, and the reason is the read CLIENT: it
  // is a viem `fallback` across several RPC providers, so consecutive reads are
  // NOT monotonic — one can land on a node that has not yet seen the block
  // carrying the previous game's stake spend. A stale-HIGH balance makes the
  // skip below fire, the session goes unfunded, and the next createGame/joinGame
  // dies with "total cost exceeds the balance of the account" while the wallet
  // that should have funded it sits flush. Observed live: session holding
  // exactly the gas buffer (0.0003) while sending value 0.0032.
  // TWO reads, take the lower: see conservativeBalance. `pending` rather than
  // the default `latest` for the node's most current view.
  const balance = conservativeBalance(
    await Promise.all([
      read.getBalance({ address: sessionAddr, blockTag: "pending" }),
      read.getBalance({ address: sessionAddr, blockTag: "pending" }),
    ]),
  );
  const mainAddress = mainWalletClient.account!.address;
  if (balance >= need) {
    const registered = await evmSessionIsLive(
      read,
      contract,
      mainAddress,
      sessionAddr,
      now,
    );
    if (registered) {
      // CONFIRM AT `latest` BEFORE TRUSTING THE SKIP. `conservativeBalance`
      // takes the lower of two reads, but both come from the SAME viem
      // `fallback` client — so when it routes to a lagging node twice, the two
      // reads agree and the minimum is still stale-HIGH. `pending` is the
      // stale-prone view; `latest` on a lagging node errs LOW, which is the
      // safe direction here: it costs a redundant top-up, never a skipped one.
      //
      // Skipping wrongly is the expensive mistake, and it has no recovery path:
      // this branch returns before waitForSessionFunding, so nothing re-checks
      // the balance before the stake goes out. Observed live on Base Sepolia —
      // session holding exactly the 0.0003 gas buffer while createGame sent
      // value 0.0032, failing with "the total cost of executing this
      // transaction exceeds the balance of the account" while the funding
      // wallet sat on 0.0239.
      const settled = await read.getBalance({
        address: sessionAddr,
        blockTag: "latest",
      });
      if (settled >= need) {
        // Gas + registration are not enough to skip: the ESCROW must also still
        // hold this game's stake. openSessionAndDeposit credits
        // withdrawable[wallet] by exactly stakeWei; once a game consumes it via
        // _takeStake the ledger drops to ~zero, so a session that is gas-funded
        // AND registered can still be UNSTAKED for the next game. Skipping here
        // would fire createGame/joinGame against an empty escrow → revert. This
        // is what makes stake-before-queue correct across games: re-deposit (one
        // popup) whenever the escrow no longer covers the stake — and topUp is 0
        // when gas already suffices, so the popup escrows only the stake.
        //
        // CONSERVATIVE READ, same discipline as conservativeBalance for gas: the
        // read CLIENT is a viem `fallback` across several RPCs, so a single read
        // can land on a lagging node that still reports the PREVIOUS game's
        // withdrawable — a stale-HIGH value that would wrongly skip the deposit
        // and leave createGame/joinGame to revert `BadStake` on an empty escrow
        // (observed: game 2's createGame reverting while withdrawable was
        // actually 0). Take the LOWER of two reads: erring low costs a redundant
        // deposit (over-funds the reclaimable escrow), erring high strands the
        // game — so the min is the safe direction.
        const escrowed = conservativeBalance(
          await Promise.all([
            read.readContract({
              address: contract,
              abi: COORDINATION_GAME_ABI,
              functionName: "withdrawable",
              args: [mainAddress],
            }) as Promise<bigint>,
            read.readContract({
              address: contract,
              abi: COORDINATION_GAME_ABI,
              functionName: "withdrawable",
              args: [mainAddress],
            }) as Promise<bigint>,
          ]),
        );
        if (escrowed >= stakeWei) return null;
        console.info(
          "[evm-session-tx] session funded+registered but escrow below stake — re-depositing",
          { wallet: mainAddress, escrowed: escrowed.toString(), stakeWei: stakeWei.toString() },
        );
      } else {
        console.info(
          "[evm-session-tx] pending balance looked sufficient but settled does not — funding",
          { session: sessionAddr, pending: balance.toString(), settled: settled.toString() },
        );
      }
    } else {
      console.info(
        "[evm-session-tx] session key is funded but NOT live on-chain — reopening",
        { session: sessionAddr, wallet: mainAddress },
      );
    }
  }

  const expiry = BigInt(now + SESSION_DURATION_SECS);
  // Clamp at zero. Re-opening an EXPIRED-but-funded session is now a reachable
  // path, and there `balance > need`, so the old `need - balance` was negative —
  // viem rejects a negative value, so the very case this fix exists to handle
  // would have thrown instead of recovering. The re-open still needs to happen;
  // it just needs no money.
  // Fund from the SETTLED balance, not the pending one that just proved
  // unreliable: `need - balance` with a stale-high `balance` under-funds by
  // exactly the amount the stale read overstated, which is how a session ends
  // up holding the gas buffer and nothing else.
  const settledForTopUp = await read.getBalance({ address: sessionAddr, blockTag: "latest" });
  const fundingBase = settledForTopUp < balance ? settledForTopUp : balance;
  const topUp = fundingBase >= need ? 0n : need - fundingBase;
  // FULL funding decision, in one line. The bug this exists to expose: the
  // wallet spent exactly 3 x the stake while `sessions(player)` still reported a
  // session key holding only the gas buffer — meaning either openSession funded
  // an address other than the one the client later signs with, or the stake left
  // immediately. Those imply opposite fixes and cannot be told apart from the
  // outside, so every input to the decision is logged rather than inferred.
  console.info("[evm-session-tx] openSessionAndDeposit — funding decision", {
    sessionAddrDerivedFromKey: sessionAddr,
    contract,
    caip2,
    stakeWei: stakeWei.toString(),
    gasBufferWei: evmSessionGasBufferWei(caip2).toString(),
    needWei: need.toString(),
    balancePendingWei: balance.toString(),
    balanceSettledWei: settledForTopUp.toString(),
    fundingBaseWei: fundingBase.toString(),
    topUpWei: topUp.toString(),
    // Where each half of msg.value lands. `gasToSessionWei` is forwarded to the
    // session EOA; `stakeToEscrowWei` is credited to withdrawable[wallet].
    gasToSessionWei: topUp.toString(),
    stakeToEscrowWei: stakeWei.toString(),
    expiry: expiry.toString(),
  });
  // ONE popup for both halves. `deposit()` credits msg.sender and `_takeStake`
  // debits withdrawable[player] — the WALLET — so an escrowed stake cannot be
  // sent by the session key. A separate deposit would therefore have cost a
  // second wallet signature on every first game; openSessionAndDeposit splits
  // gas from stake inside the one transaction instead.
  const data = encodeFunctionData({
    abi: COORDINATION_GAME_ABI,
    functionName: "openSessionAndDeposit",
    args: [sessionAddr, expiry, topUp],
  });
  const txHash = await mainWalletClient.sendTransaction({
    account: mainWalletClient.account!,
    chain: deps.chain,
    to: contract,
    data,
    value: topUp + stakeWei,
  });
  // viem resolves waitForTransactionReceipt for REVERTED transactions too, so
  // the status must be checked. Without this a reverted openSession returned its
  // hash as a successful setup and play continued with a session key that was
  // never registered on-chain and never funded — every later session-signed call
  // then failed with a confusing downstream error.
  //
  // A receipt-fetch TIMEOUT is NOT a revert: on mainnet the read fallback's
  // getTransactionReceipt lags even though the tx mined, and this raw wait would
  // throw WaitForTransactionReceiptTimeoutError — falsely failing setup so the
  // player could never enqueue (observed: two-browser mainnet stuck on "finding"
  // because both deposits "timed out" though they landed). Tolerate the timeout
  // and let the AUTHORITATIVE effect check below (waitForSessionFunding — a
  // bounded poll of the actual session+escrow funding, which openSessionAndDeposit
  // credits atomically) decide: it succeeds if the deposit landed and times out
  // if it truly did not. A returned receipt with a non-success status is a real
  // revert and still throws.
  let receipt = null;
  try {
    receipt = await read.waitForTransactionReceipt({ hash: txHash });
  } catch (e) {
    console.warn(
      "[evm-session-tx] openSessionAndDeposit receipt fetch timed out — verifying funding on-chain instead of failing setup",
      { txHash, error: String(e) },
    );
  }
  if (receipt && receipt.status !== "success") {
    throw new Error(
      `openSessionAndDeposit reverted (tx ${txHash}) — session key not registered, or the stake was not escrowed`,
    );
  }

  // A RECEIPT PROVES INCLUSION, NOT VISIBILITY. The read client is a viem
  // `fallback` over several providers whose consecutive reads are not monotonic
  // (see conservativeBalance below), so the very next call can be routed to a
  // node that has not yet seen the block this receipt came from. createGame is
  // then estimated against a session key that still reads as empty and dies
  // with "the total cost ... exceeds the balance of the account: have 0".
  //
  // Observed live on Base mainnet: openSession landed and funded the session
  // key with 0.003 ETH, and createGame failed with `have 0` moments later. The
  // funding was never the problem; the barrier was missing.
  try {
    await waitForSessionFunding(read, sessionAddr, need);
  } catch (fundingErr) {
    // SELF-CORRECT an under-provisioned session. openSessionAndDeposit's `topUp`
    // is sized from a pre-deposit balance read; when that read comes back
    // stale-HIGH — a deterministic session key that carried a residual balance
    // across games, or a fallback node lagging a prior spend — topUp is computed
    // too low and the session lands below `need`. Hard-failing here stranded the
    // player on "finding" (observed on Base mainnet with a reused session). Top up
    // the exact shortfall with a plain wallet→session transfer and re-verify,
    // rather than abandoning a game whose stake is already escrowed. Off the happy
    // path (a fresh session funds correctly in one shot), so one-popup still holds
    // for a normal first game.
    const have = await read.getBalance({ address: sessionAddr, blockTag: "latest" });
    const shortfall = need > have ? need - have : 0n;
    if (shortfall === 0n) throw fundingErr; // funded after all — a different fault
    console.warn(
      "[evm-session-tx] session under-funded after deposit — topping up the shortfall directly",
      { sessionAddr, have: have.toString(), need: need.toString(), shortfall: shortfall.toString() },
    );
    const topUpHash = await mainWalletClient.sendTransaction({
      account: mainWalletClient.account!,
      chain: deps.chain,
      to: sessionAddr,
      value: shortfall,
    });
    try {
      await read.waitForTransactionReceipt({ hash: topUpHash });
    } catch {
      // receipt-fetch lag — the funding poll below is the authoritative check
    }
    await waitForSessionFunding(read, sessionAddr, need);
  }

  // What the chain ACTUALLY holds for this session after funding, and what the
  // contract believes the session key is. If these disagree, the client is
  // signing with a key the contract does not recognise — the hypothesis that
  // cannot be tested without this line.
  try {
    const [after, onChain] = await Promise.all([
      read.getBalance({ address: sessionAddr, blockTag: "latest" }),
      read.readContract({
        address: contract,
        abi: COORDINATION_GAME_ABI,
        functionName: "sessions",
        args: [mainAddress],
      }) as Promise<readonly [string, bigint]>,
    ]);
    console.info("[evm-session-tx] post-funding state", {
      txHash,
      sessionAddrDerivedFromKey: sessionAddr,
      sessionAddrOnChain: onChain[0],
      addressesMatch: onChain[0].toLowerCase() === sessionAddr.toLowerCase(),
      sessionBalanceWei: after.toString(),
      needWei: need.toString(),
      coversStake: after >= need,
    });
  } catch (e) {
    console.warn("[evm-session-tx] could not read post-funding state", e);
  }
  return txHash;
}

/** How long to wait for a landed funding to become readable before giving up. */
const FUNDING_VISIBLE_ATTEMPTS = 20;
const FUNDING_VISIBLE_DELAY_MS = 500;

/**
 * Block until the session key's balance actually reflects `need`.
 *
 * Uses the same conservative (minimum-of-two) read as the skip decision, so a
 * lagging provider delays us rather than waving us through.
 */
export async function waitForSessionFunding(
  read: Pick<ReturnType<typeof evmReadClient>, "getBalance">,
  sessionAddr: `0x${string}`,
  need: bigint,
): Promise<void> {
  // `need` is a GENEROUS gas BUFFER (~0.0003), an order of magnitude over the
  // ~0.00002 a full game actually spends. Fund-then-check aims for exactly `need`
  // but the session routinely lands a few WEI under it — a reused/deterministic
  // session carrying sub-need dust, or plain rounding — and a strict `>= need`
  // then hard-failed a session holding 99.5% of the buffer, stranding the player
  // on "finding" over microether (observed on Base mainnet: have 298523179693112
  // vs need 300000000000000). Accept within 5%: still ~13x the real gas need.
  const floor = need - need / 20n;
  for (let i = 0; i < FUNDING_VISIBLE_ATTEMPTS; i++) {
    // `latest`, NOT `pending`. Two `pending` reads from the same viem fallback
    // client are not independent: routed to a lagging node twice they agree, and
    // conservativeBalance's minimum is still stale-HIGH. This barrier then
    // returned "funded" for a session holding only the gas buffer, createGame
    // died with "the total cost ... exceeds the balance of the account", and the
    // UI advanced to phase "funded" for a game that was never created. `latest`
    // on a lagging node errs LOW, which only costs another poll.
    const seen = conservativeBalance(
      await Promise.all([
        read.getBalance({ address: sessionAddr, blockTag: "latest" }),
        read.getBalance({ address: sessionAddr, blockTag: "latest" }),
      ]),
    );
    if (seen >= floor) return;
    await new Promise((r) => setTimeout(r, FUNDING_VISIBLE_DELAY_MS));
  }
  throw new Error(
    `session ${sessionAddr} funded on-chain but its balance is still below ${need} wei ` +
      `after ${(FUNDING_VISIBLE_ATTEMPTS * FUNDING_VISIBLE_DELAY_MS) / 1000}s — ` +
      `RPC providers are lagging; retry rather than sending a transaction that will revert`,
  );
}

/**
 * The balance to trust when deciding whether to skip the funding tx.
 *
 * The read client is a viem `fallback` over several RPC providers, so
 * consecutive reads are NOT monotonic: one can land on a node that has not yet
 * seen the block carrying the previous game's stake spend. A stale-HIGH balance
 * makes the skip fire, the session goes unfunded, and the next createGame /
 * joinGame dies with "total cost exceeds the balance of the account" while the
 * funding wallet sits flush. Observed live: session holding exactly the gas
 * buffer (0.0003) while sending value 0.0032.
 *
 * Taking the MINIMUM makes the error one-directional. A lagging node can now
 * only cause a redundant top-up — cheap, and self-correcting on the next game —
 * never a wrongly skipped one, which strands the whole cell.
 *
 * Deliberately NOT solved with a safety margin on the threshold: `topUp` is
 * `need - balance`, so a balance sitting between the margin and `need` would
 * send an openSession worth ZERO wei — a wallet popup, for a real user, that
 * funds nothing.
 */
export function conservativeBalance(reads: readonly bigint[]): bigint {
  if (reads.length === 0) throw new Error("conservativeBalance needs at least one read");
  return reads.reduce((lo, b) => (b < lo ? b : lo));
}
