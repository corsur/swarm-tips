"""
analyze.py — the statistical engine for "Do top tech companies test different things?"

Consumes:
  data/raw/{company}__{window}.json   rank-ordered (freq-desc) problem lists
  data/raw/problems.json              num -> {name, diff}
  taxonomy.json                       family -> {macro, klass}
  labels.json  (or labels.csv)        num -> family

Produces (printed report + figs/):
  Q1 per-window + pooled company x macro-family independence test (chi2 + permutation), Cramer's V
  Q2 longitudinal: Cramer's V per window (converge vs specialize)
  Q3 standardized residuals (drivers) + BH-corrected pairwise company comparisons
  Q4 core-vs-periphery axis: do companies differ in algebraic-core share
  power: achieved power per window to detect V=0.1
  artifacts: frequencies.csv (tidy long table), figs/*.png

Weighting: --weight count (default; each problem=1, valid chi2) or --weight rank
(reciprocal-rank; reported as robustness, significance via permutation only).

Usage:
  pip install pandas numpy scipy matplotlib statsmodels scikit-learn
  python analyze.py [--window six-months] [--weight count] [--perm 5000] [--floor 40]
"""
import argparse, glob, json, os
from itertools import combinations
import numpy as np
import pandas as pd
from scipy.stats import chi2_contingency, fisher_exact, ncx2, chi2 as chi2dist
from statsmodels.stats.multitest import multipletests
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

HERE = os.path.dirname(__file__)
RAW = os.path.join(HERE, "data", "raw")
WINDOWS = ["thirty-days", "three-months", "six-months", "more-than-six-months", "all"]


def _find(name):
    """Resolve a data file across both layouts: the flat working dir (everything beside the
    scripts) and the released artifact (script in scripts/, data in ../data/)."""
    for base in (HERE, os.path.join(HERE, "data"), os.path.join(HERE, "..", "data"), os.path.join(HERE, "..")):
        cand = os.path.join(base, name)
        if os.path.exists(cand):
            return cand
    return os.path.join(HERE, name)


def _raw_files():
    for base in (RAW, os.path.join(HERE, "..", "data", "raw")):
        g = [f for f in glob.glob(os.path.join(base, "*__*.json")) if "EXCLUDED" not in f]
        if g:
            return g
    return []


# ---------- load ----------
def load_labels():
    lj = _find("labels.json")
    if os.path.exists(lj) and lj.endswith(".json"):
        return {str(k): v for k, v in json.load(open(lj)).items()}
    lc = _find("labels.csv")
    if os.path.exists(lc):
        df = pd.read_csv(lc, dtype={"num": str})
        return dict(zip(df.num, df.family))
    raise SystemExit("No labels.json or labels.csv. Run make_labels.py, fill the family column, save as labels.csv.")


def build_long(weight_scheme):
    raw = _raw_files()
    if not raw:
        # Reproduce from the released derived table: frequencies.csv IS build_long('count') output
        # (the raw per-company scrape is withheld per platform terms). Rank weighting needs the raw
        # ranks/strata sizes, so it is unavailable in this mode; the headline uses count weighting.
        freq = _find("frequencies.csv")
        if not os.path.exists(freq):
            raise SystemExit("No data/raw/*.json and no frequencies.csv to reproduce from.")
        if weight_scheme != "count":
            raise SystemExit("rank-weighted reproduction needs data/raw; frequencies.csv covers the count weighting used for the headline.")
        df = pd.read_csv(freq, dtype={"num": str})
        df["weight"] = 1.0
        return df
    taxonomy = json.load(open(_find("taxonomy.json")))["families"]
    labels = load_labels()
    rows = []
    for f in raw:
        d = json.load(open(f))
        n = d["count"]
        for rank0, num in enumerate(d["order"]):
            fam = labels.get(str(num))
            if not fam:
                continue                      # unlabeled -> skipped (report separately)
            tax = taxonomy.get(fam)
            if not tax:
                raise SystemExit(f"label '{fam}' for problem {num} is not in taxonomy.json")
            w = 1.0 if weight_scheme == "count" else (n - rank0) / n
            rows.append(dict(company=d["company"], window=d["window"], num=str(num),
                             rank=rank0 + 1, family=fam, macro=tax["macro"],
                             klass=tax["klass"], weight=w))
    df = pd.DataFrame(rows)
    if df.empty:
        raise SystemExit("No labeled rows. Did you fill labels.csv?")
    return df


# ---------- stats helpers ----------
def cramers_v(table):
    stat, _, _, _ = chi2_contingency(table)
    n = table.values.sum()
    r, k = table.shape
    return float(np.sqrt((stat / n) / max(min(r - 1, k - 1), 1))), stat, (r - 1) * (k - 1)


def perm_pvalue(rows_company, rows_macro, weights, observed_stat, n_perm, rng):
    """Monte-Carlo permutation test of independence: shuffle company labels, recompute chi2.
    Valid at any sample size (no expected-count assumption)."""
    comp = np.array(rows_company)
    mac = np.array(rows_macro)
    wt = np.array(weights)
    macs = sorted(set(mac)); comps = sorted(set(comp))
    mi = {m: i for i, m in enumerate(macs)}; ci = {c: i for i, c in enumerate(comps)}
    mac_idx = np.array([mi[m] for m in mac]); n = len(comp)
    ge = 0
    for _ in range(n_perm):
        perm = rng.permutation(n)
        tab = np.zeros((len(comps), len(macs)))
        comp_idx = np.array([ci[c] for c in comp[perm]])
        np.add.at(tab, (comp_idx, mac_idx), wt)
        with np.errstate(all="ignore"):
            try:
                s = chi2_contingency(tab)[0]
            except ValueError:
                s = 0.0
        if s >= observed_stat - 1e-9:
            ge += 1
    return (ge + 1) / (n_perm + 1)


def contingency(df, value="weight"):
    return df.pivot_table(index="company", columns="macro", values=value,
                          aggfunc="sum", fill_value=0.0)


def std_residuals(table):
    obs = table.values.astype(float); n = obs.sum()
    exp = obs.sum(1, keepdims=True) @ obs.sum(0, keepdims=True) / n
    with np.errstate(all="ignore"):
        r = (obs - exp) / np.sqrt(exp)
    return pd.DataFrame(np.nan_to_num(r), index=table.index, columns=table.columns)


def power_for_V(table, V=0.1, alpha=0.05):
    n = table.values.sum(); r, k = table.shape
    df = (r - 1) * (k - 1)
    lam = n * V * V * max(min(r - 1, k - 1), 1)
    crit = chi2dist.ppf(1 - alpha, df)
    return float(1 - ncx2.cdf(crit, df, lam))


def interp_V(v):
    return ("negligible", "small", "moderate", "large")[
        0 if v < .1 else 1 if v < .3 else 2 if v < .5 else 3]


# ---------- analyses ----------
def per_window(df, floor, n_perm, rng):
    print("\n=== Q1/Q2  Per-window company x macro-family independence ===")
    out = []
    for w in WINDOWS:
        sub = df[df.window == w]
        sizes = sub.groupby("company").num.nunique()
        keep = sizes[sizes >= floor].index.tolist()
        drop = sizes[sizes < floor]
        s = sub[sub.company.isin(keep)]
        if s.company.nunique() < 2:
            print(f"\n[{w}] too few viable companies (>= {floor}); skipped. underpowered: {dict(drop)}")
            continue
        tab = contingency(s)
        V, stat, dof = cramers_v(tab)
        asymp_p = chi2_contingency(tab.values)[1]
        pp = perm_pvalue(list(s.company), list(s.macro), list(s.weight), stat, n_perm, rng)
        pw = power_for_V(tab)
        out.append(dict(window=w, companies=len(keep), V=round(V, 3),
                        size=interp_V(V), perm_p=pp, power_V0_1=round(pw, 2)))
        print(f"\n[{w}]  companies={len(keep)}  N={int(tab.values.sum())}")
        print(f"   chi2={stat:.1f} df={dof}  asymptotic_p={asymp_p:.2e}  permutation_p={pp:.4f}")
        print(f"   Cramer's V = {V:.3f} ({interp_V(V)})   power(V=0.1)={pw:.2f}")
        if len(drop):
            print(f"   excluded (underpowered, <{floor}): {dict(drop)}")
    return pd.DataFrame(out)


def longitudinal_fig(summary, out):
    if summary.empty:
        return
    order = {w: i for i, w in enumerate(WINDOWS)}
    s = summary.sort_values("window", key=lambda c: c.map(order))
    fig, ax = plt.subplots(figsize=(7, 4))
    ax.plot(s.window, s.V, marker="o")
    ax.set_ylabel("Cramer's V (between-company divergence)")
    ax.set_xlabel("time window")
    ax.set_title("Q2 Longitudinal: do companies converge (all-time) or specialize (recent)?")
    ax.tick_params(axis="x", rotation=20)
    fig.tight_layout(); fig.savefig(f"{out}/longitudinal_V.png", dpi=150); plt.close(fig)


def residuals_and_pairwise(df, window, floor, out):
    print(f"\n=== Q3  Drivers & pairwise comparisons  (window={window}) ===")
    sub = df[df.window == window]
    sizes = sub.groupby("company").num.nunique()
    keep = sizes[sizes >= floor].index.tolist()
    s = sub[sub.company.isin(keep)]
    tab = contingency(s)
    res = std_residuals(tab)
    flat = res.stack().sort_values(key=np.abs, ascending=False).head(12)
    print("Top |residual| cells (company x macro): + = over-represented")
    for (c, m), v in flat.items():
        print(f"   {c:>10} x {m:<14} {v:+.2f}")
    # heatmap
    fig, ax = plt.subplots(figsize=(max(7, tab.shape[1] * .6), max(3, tab.shape[0] * .5)))
    vmax = np.abs(res.values).max() or 1
    im = ax.imshow(res.values, cmap="RdBu_r", vmin=-vmax, vmax=vmax, aspect="auto")
    ax.set_xticks(range(tab.shape[1])); ax.set_xticklabels(tab.columns, rotation=90, fontsize=8)
    ax.set_yticks(range(tab.shape[0])); ax.set_yticklabels(tab.index, fontsize=9)
    fig.colorbar(im, ax=ax, label="std residual")
    ax.set_title(f"Q3 residuals — {window}")
    fig.tight_layout(); fig.savefig(f"{out}/residual_heatmap.png", dpi=150); plt.close(fig)
    # pairwise
    pairs, pvals = [], []
    for a, b in combinations(keep, 2):
        t = contingency(s[s.company.isin([a, b])])
        t = t.loc[:, (t != 0).any(axis=0)]
        _, p, _, _ = chi2_contingency(t.values)
        pairs.append((a, b)); pvals.append(p)
    if pvals:
        rej, padj, _, _ = multipletests(pvals, method="fdr_bh")
        print("\nPairwise (BH-corrected):")
        for (a, b), p, pa, r in sorted(zip(pairs, pvals, padj, rej), key=lambda x: x[2]):
            print(f"   {a:>10} vs {b:<10} p={p:.2e} p_adj={pa:.2e} [{'DIFF' if r else 'n.s.'}]")


def core_periphery(df, window, floor):
    print(f"\n=== Q4  Core (algebraic) vs periphery axis  (window={window}) ===")
    sub = df[(df.window == window) & (df.klass != "trivial")]
    sizes = sub.groupby("company").num.nunique()
    keep = sizes[sizes >= floor].index.tolist()
    s = sub[sub.company.isin(keep)]
    tab = s.pivot_table(index="company", columns="klass", values="weight", aggfunc="sum", fill_value=0)
    stat, p, dof, _ = chi2_contingency(tab.values)
    share = (tab["core"] / tab.sum(1)).sort_values(ascending=False)
    print(f"   chi2={stat:.1f} df={dof} p={p:.2e}  ({'differ' if p < .05 else 'no difference'})")
    print("   algebraic-core share by company:")
    for c, v in share.items():
        print(f"     {c:>10}: {v:5.1%}")


def pca_fig(df, window, floor, out):
    try:
        from sklearn.decomposition import PCA
    except ImportError:
        return
    sub = df[df.window == window]
    sizes = sub.groupby("company").num.nunique()
    keep = sizes[sizes >= floor].index.tolist()
    tab = contingency(sub[sub.company.isin(keep)])
    prof = tab.div(tab.sum(1), axis=0).fillna(0)
    if prof.shape[0] < 3:
        return
    xy = PCA(n_components=2).fit_transform(prof.values)
    fig, ax = plt.subplots(figsize=(6, 5))
    ax.scatter(xy[:, 0], xy[:, 1])
    for i, name in enumerate(prof.index):
        ax.annotate(name, (xy[i, 0], xy[i, 1]), fontsize=9)
    ax.set_xlabel("PC1"); ax.set_ylabel("PC2")
    ax.set_title(f"Company macro-profiles (PCA) — {window}")
    fig.tight_layout(); fig.savefig(f"{out}/pca.png", dpi=150); plt.close(fig)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--window", default="six-months", help="primary window for Q3/Q4/PCA")
    ap.add_argument("--weight", choices=["count", "rank"], default="count")
    ap.add_argument("--perm", type=int, default=5000)
    ap.add_argument("--floor", type=int, default=40)
    ap.add_argument("--out", default=os.path.join(HERE, "figs"))
    ap.add_argument("--seed", type=int, default=12345)
    args = ap.parse_args()
    os.makedirs(args.out, exist_ok=True)
    rng = np.random.default_rng(args.seed)

    df = build_long(args.weight)
    df.to_csv(os.path.join(HERE, "frequencies.csv"), index=False)
    print(f"weight={args.weight}  rows={len(df)}  problems={df.num.nunique()}  "
          f"companies={df.company.nunique()}  macro-families={df.macro.nunique()}")

    summary = per_window(df, args.floor, args.perm, rng)
    longitudinal_fig(summary, args.out)
    residuals_and_pairwise(df, args.window, args.floor, args.out)
    core_periphery(df, args.window, args.floor)
    pca_fig(df, args.window, args.floor, args.out)
    print(f"\nfigures -> {args.out}/   tidy table -> frequencies.csv")


if __name__ == "__main__":
    main()
