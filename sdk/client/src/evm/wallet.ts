import { createConfig, type Config, type CreateConnectorFn } from "wagmi";
import { injected, walletConnect } from "wagmi/connectors";
import {
  createWalletClient,
  fallback,
  http as viemHttp,
  type Chain,
  type Hex,
  type Transport,
} from "viem";
import { privateKeyToAccount } from "viem/accounts";
import { e2eTestModeEnabled } from "./testing.js";

/**
 * Resilient transport: a viem `fallback` across the given RPC URLs (fails over
 * to the next on a transient RPC error), or the chain default when none are
 * given. Public testnet RPCs (e.g. sepolia.base.org) intermittently reject
 * `eth_sendRawTransaction` with -32602; failover keeps commit/reveal from
 * dying on one bad endpoint.
 */
function buildTransport(rpcUrls?: string[]): Transport {
  if (rpcUrls && rpcUrls.length > 0) {
    return fallback(rpcUrls.map((url) => viemHttp(url)));
  }
  return viemHttp();
}

/**
 * Read an E2E test private key from a `?<param>=<0x-hex>` query param, if
 * present (default param `evmTestWallet`). Mirrors the Solana `?testWallet=`
 * pattern so Playwright can drive the EVM path headlessly. Returns null off the
 * happy path (no window, missing/malformed param) AND whenever the page isn't
 * in E2E test mode — so a crafted prod link can't activate the connector.
 */
export function getEvmTestWalletKey(param = "evmTestWallet"): Hex | null {
  if (typeof window === "undefined") return null;
  if (!e2eTestModeEnabled()) return null;
  const raw = new URLSearchParams(window.location.search).get(param);
  if (!raw) return null;
  const hex = raw.startsWith("0x") ? raw : `0x${raw}`;
  // A secp256k1 private key is 32 bytes → 64 hex chars after the 0x prefix.
  if (!/^0x[0-9a-fA-F]{64}$/.test(hex)) {
    console.error("[evm-kit] ignoring malformed test wallet param");
    return null;
  }
  return hex as Hex;
}

/**
 * Read an E2E RPC override from `?evmRpc=<https-url>`. Lets Playwright point the
 * page at a reliable provider (e.g. QuickNode) WITHOUT baking the key into the
 * public bundle. Caller gates this to test mode (test wallet present) so a
 * crafted link can't redirect a real user's wallet to a hostile RPC. Only https
 * URLs are accepted. Also gated on E2E test mode (defense in depth — its only
 * caller already requires a test wallet, which is itself test-mode-gated).
 */
export function getEvmRpcOverride(param = "evmRpc"): string | null {
  if (typeof window === "undefined") return null;
  if (!e2eTestModeEnabled()) return null;
  const raw = new URLSearchParams(window.location.search).get(param);
  if (!raw) return null;
  try {
    const url = new URL(raw);
    if (url.protocol !== "https:") {
      // Log for the same reason the malformed branch below does: a rejected
      // override is either a hostile downgrade attempt or an e2e run pointed
      // at http://, and silently returning null leaves no trace of either.
      console.error(
        `[evm-kit] ignoring non-https evmRpc param (protocol ${url.protocol})`
      );
      return null;
    }
    return url.toString();
  } catch {
    console.error("[evm-kit] ignoring malformed evmRpc param");
    return null;
  }
}

/**
 * A wagmi connector backed by a raw private key via a viem wallet client.
 * Auto-connects and signs txs + messages without a browser extension, so
 * Playwright can drive an EVM flow headlessly. Test-only — gated behind the
 * test-wallet query param at config-build time.
 */
function evmTestConnector(
  key: Hex,
  chain: Chain,
  transport: Transport
): CreateConnectorFn {
  const account = privateKeyToAccount(key);
  const client = createWalletClient({ account, chain, transport });
  // An EIP-1193 shim routing the wallet methods wagmi calls to the viem client;
  // everything else falls through to the chain transport.
  const request = async (args: {
    method: string;
    params?: unknown[];
  }): Promise<unknown> => {
    const params = args.params ?? [];
    switch (args.method) {
      case "eth_accounts":
      case "eth_requestAccounts":
        return [account.address];
      case "eth_chainId":
        return chain.id;
      case "personal_sign": {
        const [message] = params as [Hex];
        return client.signMessage({ message: { raw: message } });
      }
      case "eth_sendTransaction": {
        // wagmi hands EIP-1193 params in RPC-hex form (gas/value/nonce as 0x
        // strings). Passing those straight into viem's TYPED sendTransaction
        // produces a signed tx that strict RPCs reject ("failed to decode signed
        // transaction"). Take only the essential fields and let viem re-prepare
        // gas/fees/nonce itself — the path a plain viem walletClient uses.
        const [tx] = params as [
          { to?: Hex; data?: Hex; input?: Hex; value?: Hex }
        ];
        return client.sendTransaction({
          to: tx.to,
          data: tx.data ?? tx.input,
          value: tx.value ? BigInt(tx.value) : undefined,
        } as Parameters<typeof client.sendTransaction>[0]);
      }
      default:
        return client.request({
          method: args.method,
          params,
        } as Parameters<typeof client.request>[0]);
    }
  };
  // wagmi's injected connector subscribes to EIP-1193 events; provide no-op
  // on/removeListener so connect() doesn't crash on the private-key provider.
  return injected({
    target: {
      id: "evmTestWallet",
      name: "EVM Test Wallet",
      provider: { request, on: () => {}, removeListener: () => {} } as never,
    },
  });
}

export interface WagmiConfigOpts {
  /** Reown/WalletConnect projectId. When set (and not in test mode) the
   *  WalletConnect connector is added alongside `injected`, so mobile + hardware
   *  + QR wallets can connect. Omit → injected-only (browser extensions still
   *  work via EIP-6963). It is a public client id, safe to ship in the bundle.
   *
   *  NOTE: registering the raw wagmi connector is NOT sufficient for a
   *  RainbowKit UI — RainbowKit's modal renders its own wallet registry and
   *  ignores connectors it did not create, so a WalletConnect connector added
   *  here never appears as an option. Apps with a RainbowKit modal must pass
   *  `connectors` below instead. */
  walletConnectProjectId?: string;
  /** Connectors supplied by the app, used when NOT in test mode. This is how a
   *  RainbowKit app injects `connectorsForWallets(...)` output so its modal and
   *  the wagmi config agree on the wallet list. Keeping it an opaque array lets
   *  evm-kit stay free of any RainbowKit dependency. */
  connectors?: CreateConnectorFn[];
}

/**
 * Choose the connector set for a config. Pure, so the precedence rule is
 * unit-testable: the E2E test connector always wins (a live sweep must never be
 * hijacked by a real wallet), then app-supplied connectors, then the built-in
 * injected/WalletConnect pair.
 */
export function selectConnectors<T>(
  testConnector: T | null,
  appConnectors: T[] | undefined,
  builtIn: T[]
): T[] {
  if (testConnector) return [testConnector];
  if (appConnectors && appConnectors.length > 0) return appConnectors;
  return builtIn;
}

/** One chain in a multi-chain config, with its own failover RPCs. */
export interface ChainWithRpcs {
  chain: Chain;
  rpcUrls?: string[];
}

/**
 * Build a wagmi config for one or several EVM chains. Multi-chain lets the
 * wallet switch chains (e.g. Base Sepolia ↔ Ethereum Sepolia) so the same site
 * plays each — each chain gets its own resilient transport. The `injected`
 * connector serves humans; the private-key test connector serves E2E when the
 * test-wallet query param is present (bound to the `?evmChain=<caip2>` chain,
 * or the first listed chain).
 */
export function buildWagmiConfig(
  chains: ChainWithRpcs[],
  opts: WagmiConfigOpts = {}
): Config {
  const { walletConnectProjectId } = opts;
  const list = chains;
  const key = getEvmTestWalletKey();
  // A `?evmRpc=` override is honored ONLY in test mode (test wallet present), so
  // a crafted link can't point a real user's wallet at a hostile RPC. When set,
  // use that single reliable endpoint (no fallback) — fail-fast-or-succeed.
  const override = key ? getEvmRpcOverride() : null;
  const transports = Object.fromEntries(
    list.map(({ chain, rpcUrls: chainRpcs }) => [
      chain.id,
      override ? viemHttp(override) : buildTransport(chainRpcs),
    ])
  );
  // Test connector binds to one chain: the `?evmChain=<eip155:id>` selection if
  // it matches a listed chain, else the first. `useSwitchChain` in the game hook
  // moves the wallet to the chain the player picked before any staking tx.
  const testChain = pickTestChain(list) ?? list[0]!.chain;
  const connectors = selectConnectors<CreateConnectorFn>(
    key ? evmTestConnector(key, testChain, transports[testChain.id]!) : null,
    opts.connectors,
    walletConnectProjectId
      ? [injected(), walletConnect({ projectId: walletConnectProjectId })]
      : [injected()]
  );
  const chainTuple = list.map((c) => c.chain) as [Chain, ...Chain[]];
  return createConfig({ chains: chainTuple, connectors, transports });
}

/** Which listed chain the E2E test wallet should bind to: the `?evmChain=`
 *  eip155 selection when it matches, else null (caller defaults to first). */
function pickTestChain(list: ChainWithRpcs[]): Chain | null {
  if (typeof window === "undefined") return null;
  const raw = new URLSearchParams(window.location.search).get("evmChain");
  if (!raw) return null;
  const id = raw.startsWith("eip155:")
    ? Number(raw.slice("eip155:".length))
    : Number(raw);
  return list.find((c) => c.chain.id === id)?.chain ?? null;
}
