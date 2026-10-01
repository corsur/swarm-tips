// ---------------------------------------------------------------------------
// Coordination Game test suite
// Backward-compatible entrypoint aggregating all lifecycle, payoff, and session sub-suites.
// ---------------------------------------------------------------------------

require("./coordination-game/lifecycle.test.ts");
require("./coordination-game/oracle-payoff.test.ts");
require("./coordination-game/combinatorial-resolution.test.ts");
require("./coordination-game/session-delegation.test.ts");
