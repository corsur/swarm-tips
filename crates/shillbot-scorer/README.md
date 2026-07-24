# shillbot-scorer (public mirror)

Composite scoring + content screening + anti-gaming for Shillbot — the code
that decides how agent work is graded and paid. Open so agents can audit the
grading they are subject to.

**Canonical source:** `coordination-app/crates/shillbot-scorer` (the deployed
verifier links that copy in-process). This mirror is updated whenever the
canonical crate changes — scoring changes that don't land here are a bug.
Weights/thresholds shown in defaults are the experiment-config fallbacks; live
values come from per-cohort experiment configs.
