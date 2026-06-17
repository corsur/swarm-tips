"""
significance.py — rigorous effect-size & equivalence audit for every company claim.

The omnibus chi-square is SIGNIFICANT in several windows, so "companies are indistinguishable"
is the wrong claim. The defensible claim is about EFFECT SIZE: the between-company association is
negligible-to-small, and that requires three things the raw chi-square / raw Cramer's V do not give:

  1. BIAS-CORRECTED Cramer's V (Bergsma 2013). With 20 macro-families x 7 companies (df=114), raw V
     is inflated: even independent data yields E[V] ~ sqrt(df / (N * min(r-1,k-1))). We report the
     null-expected V alongside, so the reader sees how close the observed value sits to pure noise.
  2. A CONFIDENCE INTERVAL on the (corrected) effect size, via nonparametric bootstrap over problems.
     "Small effect" is only defensible if the UPPER CI bound is below a threshold.
  3. An EQUIVALENCE verdict (TOST-style): is the upper 95% CI bound on corrected V below the
     "small" (0.1) / "moderate" (0.3) Cohen thresholds? Failing to reject H0 is NOT equivalence;
     bounding the effect size is.

Pairwise: the "Uber/Apple are the outliers, FAANG-5 are interchangeable" claim is validated by
per-pair corrected V + bootstrap CI (effect sizes), not just BH-corrected p-values (absence of a
significant p is not evidence of equivalence).

Usage:
  python significance.py [--window all] [--boot 4000] [--seed 12345]
Depends on analyze.build_long (taxonomy.json + labels.csv).
"""
import argparse, csv, os
from itertools import combinations
import numpy as np
from scipy.stats import chi2_contingency, ncx2, chi2 as chi2dist
from analyze import build_long, contingency, WINDOWS, HERE, _find

SMALL, MODERATE = 0.1, 0.3  # Cohen thresholds for Cramer's V


def load_scheme_map():
    """surface family -> recursion scheme (fold / dp / relaxation / bisection); absent -> tail."""
    return {r["surface_family"]: r["scheme"] for r in csv.DictReader(open(_find("family_scheme_rule.csv")))}


def regroup(df, level):
    """The 'company' comparison is reported over recursion schemes by default (the 'wide but
    shallow' claim); --level family reports the finer 20-surface-family grouping. We reuse all the
    downstream machinery by overwriting the 'macro' column with the chosen grouping."""
    if level == "family":
        return df
    rule = load_scheme_map()
    return df.assign(macro=df["family"].map(lambda f: rule.get(f, "tail")))


def _stats(tab):
    """Drop all-zero rows/cols (a bootstrap sample may miss a category), then chi2 on the
    reduced table with its reduced dimensions used consistently for normalization."""
    tab = np.asarray(tab, float)
    tab = tab[tab.sum(1) > 0][:, tab.sum(0) > 0]
    n = tab.sum()
    r, k = tab.shape
    if r < 2 or k < 2 or n <= 1:
        return 0.0, n, r, k
    with np.errstate(all="ignore"):
        stat = chi2_contingency(tab, correction=False)[0]
    return float(stat), n, r, k


def raw_V(tab):
    stat, n, r, k = _stats(tab)
    if n == 0:
        return 0.0
    return float(np.sqrt((stat / n) / max(min(r - 1, k - 1), 1)))


def corrected_V(tab):
    """Bergsma (2013) bias-corrected Cramer's V; 0 when the correction drives phi^2 negative."""
    stat, n, r, k = _stats(tab)
    if n <= 1 or r < 2 or k < 2:
        return 0.0
    phi2 = stat / n
    phi2c = max(0.0, phi2 - (r - 1) * (k - 1) / (n - 1))
    rc = r - (r - 1) ** 2 / (n - 1)
    kc = k - (k - 1) ** 2 / (n - 1)
    denom = max(min(rc - 1, kc - 1), 1e-12)
    return float(np.sqrt(phi2c / denom))


def null_expected_V(tab):
    """E[V] when rows and columns are independent (pure-noise floor)."""
    _, n, r, k = _stats(tab)
    if n == 0 or r < 2 or k < 2:
        return 0.0
    df = (r - 1) * (k - 1)
    return float(np.sqrt(df / (n * max(min(r - 1, k - 1), 1))))


def power_V(tab, V=0.1, alpha=0.05):
    n = tab.sum()
    r, k = tab.shape
    df = (r - 1) * (k - 1)
    lam = n * V * V * max(min(r - 1, k - 1), 1)
    return float(1 - ncx2.cdf(chi2dist.ppf(1 - alpha, df), df, lam))


def bootstrap_V_ci(rows, n_boot, rng, statfn):
    """rows: list of (company, macro). Resample problems with replacement, recompute statfn(V).

    Returns a BIAS-CORRECTED bootstrap interval. statfn (corrected_V) is floored at 0, so the naive
    percentile interval of resamples is biased upward — for a near-null effect it can float entirely
    above the full-sample point estimate, which is not a valid CI. We subtract the bootstrap bias
    (mean of resamples minus the full-sample statistic) so the interval is centered on the point
    estimate, then clip the (necessarily non-negative) bounds at 0.
    """
    comp = np.array([c for c, _ in rows])
    mac = np.array([m for _, m in rows])
    comps = sorted(set(comp)); macs = sorted(set(mac))
    ci = {c: i for i, c in enumerate(comps)}; mi = {m: i for i, m in enumerate(macs)}
    cidx = np.array([ci[c] for c in comp]); midx = np.array([mi[m] for m in mac])
    n = len(rows)
    full = np.zeros((len(comps), len(macs)))
    np.add.at(full, (cidx, midx), 1.0)
    theta = statfn(full)  # full-sample point estimate
    vals = np.empty(n_boot)
    for b in range(n_boot):
        s = rng.integers(0, n, n)
        tab = np.zeros((len(comps), len(macs)))
        np.add.at(tab, (cidx[s], midx[s]), 1.0)
        vals[b] = statfn(tab)
    adj = vals - (vals.mean() - theta)  # recenter on the point estimate
    return float(max(0.0, np.percentile(adj, 2.5))), float(max(0.0, np.percentile(adj, 97.5)))


def equiv_verdict(upper):
    if upper < SMALL:
        return f"EQUIVALENCE established: upper 95% CI ({upper:.3f}) < {SMALL} -> effect is NEGLIGIBLE"
    if upper < MODERATE:
        return f"bounded SMALL: upper 95% CI ({upper:.3f}) < {MODERATE} (excludes a moderate effect)"
    return f"NOT bounded small: upper 95% CI ({upper:.3f}) >= {MODERATE}"


def magnitude(v):
    """Cohen bucket for a corrected Cramer's V point estimate."""
    if v < SMALL:
        return "negligible"
    if v < MODERATE:
        return "small"
    return "moderate+"


def pair_verdict(point, lo):
    """A pair DIFFERS only if BOTH the full-sample point estimate is positive AND the bootstrap
    lower bound clears 0. Requiring point>0 guards against the upward bias of bootstrapping a
    floored (max(0,.)) statistic, which can lift the lower bound off 0 even when the full-sample
    corrected V is exactly 0. The magnitude bucket reports whether a real difference even matters."""
    if point > 0 and lo > 0:
        return f"DIFFER ({magnitude(point)})"
    return "indistinguishable"


def rows_of(df, window, companies=None):
    sub = df[df.window == window]
    if companies is not None:
        sub = sub[sub.company.isin(companies)]
    return list(zip(sub.company, sub.macro))


def viable(df, window, floor):
    sub = df[df.window == window]
    sizes = sub.groupby("company").num.nunique()
    return sizes[sizes >= floor].index.tolist(), dict(sizes[sizes < floor])


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--window", default="all")
    ap.add_argument("--boot", type=int, default=4000)
    ap.add_argument("--floor", type=int, default=40)
    ap.add_argument("--seed", type=int, default=12345)
    ap.add_argument("--level", choices=["scheme", "family"], default="scheme",
                    help="group firms over the four recursion schemes +tail (default, the headline) "
                         "or the twenty surface families (finer, more conservative)")
    args = ap.parse_args()
    rng = np.random.default_rng(args.seed)
    df = regroup(build_long("count"), args.level)

    print("=" * 78)
    print(f"OMNIBUS effect size per window  (grouping: {args.level})  (raw V vs bias-corrected V vs null-noise floor)")
    print("  claim is about EFFECT SIZE, not the p-value: the omnibus is significant in several")
    print("  windows, so the defensible statement is 'significant but negligible/small'.")
    print("=" * 78)
    hdr = f"{'window':>22} {'N':>5} {'cos':>3} {'rawV':>6} {'corrV':>6} {'nullV':>6} {'corrV 95% CI':>16} {'pow.1':>6}"
    print(hdr)
    for w in WINDOWS:
        keep, _ = viable(df, w, args.floor)
        if len(keep) < 2:
            continue
        rows = rows_of(df, w, keep)
        tab = contingency(df[(df.window == w) & (df.company.isin(keep))]).values
        lo, hi = bootstrap_V_ci(rows, args.boot, rng, corrected_V)
        print(f"{w:>22} {int(tab.sum()):>5} {len(keep):>3} {raw_V(tab):>6.3f} "
              f"{corrected_V(tab):>6.3f} {null_expected_V(tab):>6.3f} "
              f"[{lo:.3f},{hi:.3f}]  {power_V(tab):>5.2f}")
    # focal-window equivalence verdict
    keep, dropped = viable(df, args.window, args.floor)
    rows = rows_of(df, args.window, keep)
    lo, hi = bootstrap_V_ci(rows, args.boot, rng, corrected_V)
    print("\n" + "=" * 78)
    print(f"EQUIVALENCE VERDICT  (window={args.window}, companies={keep})")
    print("=" * 78)
    print(f"  bias-corrected V = {corrected_V(contingency(df[(df.window==args.window) & (df.company.isin(keep))]).values):.3f}")
    print(f"  {equiv_verdict(hi)}")
    if dropped:
        print(f"  excluded (< {args.floor} problems, underpowered): {dropped}")

    print("\n" + "=" * 78)
    print(f"PAIRWISE effect sizes  (window={args.window}) — which companies actually differ?")
    print("  corrected V per pair + 95% CI; 'differs' = point estimate > 0 AND CI lower bound > 0,")
    print("  tagged with its Cohen magnitude (negligible <0.1 / small <0.3 / moderate+ >=0.3).")
    print("=" * 78)
    res = []
    for a, b in combinations(keep, 2):
        rows = rows_of(df, args.window, [a, b])
        tab = contingency(df[(df.window == args.window) & (df.company.isin([a, b]))]).values
        lo, hi = bootstrap_V_ci(rows, max(args.boot // 2, 1000), rng, corrected_V)
        res.append((a, b, corrected_V(tab), lo, hi))
    for a, b, v, lo, hi in sorted(res, key=lambda x: -x[2]):
        print(f"  {a:>10} vs {b:<10} corrV={v:.3f}  CI=[{lo:.3f},{hi:.3f}]  [{pair_verdict(v, lo)}]")


if __name__ == "__main__":
    main()
