// ---------------------------------------------------------------------------
// Shillbot test suite
// Backward-compatible entrypoint aggregating all lifecycle, dispute, session,
// admin, and concurrent limit sub-suites.
// ---------------------------------------------------------------------------

require("./shillbot/lifecycle.test.ts");
require("./shillbot/disputes.test.ts");
require("./shillbot/session-delegates.test.ts");
require("./shillbot/admin-and-governance.test.ts");
require("./shillbot/concurrent-limits.test.ts");
