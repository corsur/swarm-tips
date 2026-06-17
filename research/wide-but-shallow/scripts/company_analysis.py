"""company_analysis.py — proof-based per-company solution-structure analysis.

Draws a uniform ~SAMPLE_N sample of each company's problems (seeded, stable across runs), maps the
CERTIFIED ones to their proven scheme (from the cert file's `scheme:` header — authoritative, so
label-corrected cases like 797/1871/2214 use the proven scheme), and reports:

  * per-company proof-classified count (target >= TARGET) and remaining uncertified-in-sample,
  * the union grind-list ranked by company-overlap (certify high-overlap first),
  * once companies clear TARGET: the company x proven-scheme contingency, corrected Cramer's V,
    and the per-company-vs-rest V + bootstrap CI (uber outlier? others bounded similar?).

Usage: .venv/bin/python company_analysis.py
"""
import csv, glob, json, os, random, re, sys
import numpy as np
from significance import corrected_V, bootstrap_V_ci

HERE = os.path.dirname(os.path.abspath(__file__))
COMPANIES = ["amazon", "apple", "bloomberg", "facebook", "google", "microsoft", "uber"]
SCHEMES = ["fold", "dp", "relaxation", "bisection"]
SAMPLE_N = 50
TARGET = 48
SEED = 20260616


def cert_schemes():
    """num -> proven scheme, parsed from each cert file's `scheme:` header."""
    out = {}
    for f in glob.glob(os.path.join(HERE, "lproofs/Lproofs/Problems/**/P*.lean"), recursive=True):
        m = re.search(r"P0*(\d+)_", os.path.basename(f))
        if not m:
            continue
        txt = open(f).read()
        s = re.search(r"scheme:(\w+)", txt)
        if s:
            out[m.group(1)] = s.group(1)
    return out


def company_lists():
    out = {}
    for c in COMPANIES:
        p = os.path.join(HERE, f"data/raw/{c}__all.json")
        out[c] = [str(n) for n in json.load(open(p))["order"]]
    return out


def company_samples(lists):
    rng = random.Random(SEED)
    return {c: set(rng.sample(probs, min(SAMPLE_N, len(probs)))) for c, probs in lists.items()}


def main():
    certs = cert_schemes()
    lists = company_lists()
    samples = company_samples(lists)

    # per-company classified counts
    print(f"=== per-company proof-classified (sample {SAMPLE_N}, target >={TARGET}) ===")
    counts = {}
    for c in COMPANIES:
        cls = {n for n in samples[c] if n in certs}
        counts[c] = len(cls)
        gap = max(0, TARGET - len(cls))
        print(f"  {c:10} classified {len(cls):2d}/{len(samples[c])}   need +{gap}")

    # union grind list: uncertified-in-sample, ranked by how many companies' samples include it
    from collections import Counter
    need = Counter()
    for c in COMPANIES:
        for n in samples[c]:
            if n not in certs:
                need[n] += 1
    ranked = sorted(need.items(), key=lambda kv: (-kv[1], int(kv[0])))
    print(f"\n=== grind list: {len(ranked)} distinct uncertified-in-sample (overlap-ranked) ===")
    print("  num(overlap):", " ".join(f"{n}({v})" for n, v in ranked[:40]))

    if min(counts.values()) < TARGET:
        print(f"\n[not yet powered] min classified = {min(counts.values())} < {TARGET}; keep certifying.")
        return

    # contingency company x proven-scheme (classified problems only)
    rows = []
    tab = np.zeros((len(COMPANIES), len(SCHEMES) + 1))  # + tail/other
    for i, c in enumerate(COMPANIES):
        for n in samples[c]:
            if n in certs:
                s = certs[n]
                j = SCHEMES.index(s) if s in SCHEMES else len(SCHEMES)
                tab[i, j] += 1
                rows.append((c, s if s in SCHEMES else "tail"))
    print("\n=== company x proven-scheme ===")
    print("           " + "  ".join(f"{s:>10}" for s in SCHEMES + ["tail"]))
    for i, c in enumerate(COMPANIES):
        print(f"  {c:10}" + "  ".join(f"{int(tab[i, j]):10d}" for j in range(len(SCHEMES) + 1)))
    print(f"\n  overall corrected V = {corrected_V(tab):.3f}")
    rng = np.random.default_rng(SEED)
    print("  per-company vs pooled-rest (corrected V, 95% CI):")
    for i, c in enumerate(COMPANIES):
        row = tab[i]; rest = tab.sum(0) - row
        pair = np.vstack([row, rest])
        pr = [("rest" if cc != c else c) for cc, _ in rows]
        macs = [m for _, m in rows]
        lo, hi = bootstrap_V_ci(list(zip(pr, macs)), 1000, rng, corrected_V)
        verdict = "OUTLIER" if lo > 0 else ("bounded<0.3" if hi < 0.3 else "wide")
        print(f"    {c:10} V={corrected_V(pair):.3f}  CI=[{lo:.3f},{hi:.3f}]  {verdict}")


if __name__ == "__main__":
    main()
