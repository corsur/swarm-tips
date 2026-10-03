// ---------------------------------------------------------------------------
// Shillbot test suite
// Backward-compatible entrypoint aggregating all lifecycle, dispute, session,
// admin, and concurrent limit sub-suites.
// ---------------------------------------------------------------------------

import "./shillbot/lifecycle.test.ts";
import "./shillbot/disputes.test.ts";
import "./shillbot/session-delegates.test.ts";
import "./shillbot/admin-and-governance.test.ts";
import "./shillbot/concurrent-limits.test.ts";
