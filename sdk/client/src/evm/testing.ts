// E2E test-mode gate — intentionally dependency-free (only `window.localStorage`)
// so the Playwright harness can import the flag key via the
// `@swarm-tips/client/evm/testing`
// subpath WITHOUT pulling wagmi/viem into the test package.

/**
 * localStorage key that opts a page into the E2E test connectors. The
 * `?evmTestWallet=`/`?evmRpc=` query params are honored ONLY when this key is
 * set to "1". A query param alone can't activate the test path — see
 * `e2eTestModeEnabled` for why this closes the prod-phishing hole.
 */
export const E2E_TEST_MODE_KEY = "swarm_e2e_test_mode";

/**
 * Is this page running under the E2E harness?
 *
 * The deployed prod bundle is the SAME bundle the Playwright e2e drives against
 * coordination.game, so `import.meta.env.DEV` is false in both and can't gate
 * the test connectors. We instead require an explicit localStorage opt-in that
 * ONLY the harness can set: Playwright's `addInitScript` runs on the page origin
 * before any page script, so it can seed `localStorage[E2E_TEST_MODE_KEY]`. A
 * phishing link can carry `?evmTestWallet=` query params but CANNOT write to a
 * victim's coordination.game localStorage (it's origin-scoped, not URL-settable),
 * so a real user clicking such a link never activates the private-key connector
 * or the RPC override. Fail-closed: any localStorage access error → disabled.
 */
export function e2eTestModeEnabled(): boolean {
  if (typeof window === "undefined") return false;
  try {
    return window.localStorage.getItem(E2E_TEST_MODE_KEY) === "1";
  } catch {
    return false;
  }
}
