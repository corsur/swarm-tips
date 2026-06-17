"""
sensitivity.py — does the headline survive relabeling the judgment calls?

The empirical results rest on a single-rater labeling. This Monte-Carlo perturbs only the
*genuinely ambiguous* problems (each has >=1 defensible alternative family), redrawing each
uniformly over its plausible set, and re-runs the key statistics every draw:

  (A) between-company effect (Cramer's V, 6-month, count weight)
  (B) FAANG-core indistinguishability (# of the 10 Google/Amazon/MS/Meta/Bloomberg pairs
      that turn significant, BH-corrected over all 21 pairs)
  (C) Uber as the dominant outlier (does the top residual cell stay Uber x graph-search)
  (D) scheme concentration (frequency-weighted four-recursion-scheme coverage)

If V stays small, the FAANG-core stays n.s., Uber stays the outlier, and four-scheme coverage
stays high across draws, the conclusions are robust to labeling subjectivity.

Usage: python sensitivity.py [--draws 2000]
"""
import argparse, csv, glob, json, os
from collections import defaultdict, Counter
import numpy as np
from scipy.stats import chi2_contingency
from statsmodels.stats.multitest import multipletests

HERE = os.path.dirname(__file__)
RAW = os.path.join(HERE, "data", "raw")
FAANG5 = ["google", "amazon", "microsoft", "facebook", "bloomberg"]

# macro -> generator supergroup (same mapping used for the concentration result)
GEN = {
 "dp": "recursive-fold", "backtracking": "recursive-fold", "tree": "recursive-fold",
 "graph-search": "relaxation", "dfs-flood": "relaxation",
 "two-pointers": "monotone-seq", "sliding-window": "monotone-seq",
 "binary-search": "monotone-seq", "stack": "monotone-seq",
 "hashing": "hash-running", "prefix-sum": "hash-running", "intervals": "hash-running",
}

# Genuinely ambiguous problems -> plausible ALTERNATIVE families (primary added at runtime).
# Curated toward CROSS-macro calls (the ones that can actually move a result); within-macro
# ambiguities are included too and simply demonstrate stability.
ALT = {
 "121": ["greedy"], "53": ["greedy"], "122": ["dp-linear"], "55": ["dp-linear"],
 "45": ["bfs"], "5": ["dp-interval"], "647": ["dp-interval"], "169": ["greedy"],
 "229": ["greedy"], "42": ["two-pointers"], "134": ["two-pointers"], "763": ["hashing"],
 "300": ["binary-search"], "354": ["dp-linear"], "128": ["union-find"], "287": ["binary-search"],
 "202": ["fast-slow"], "380": ["hashing"], "528": ["design"], "1146": ["binary-search"],
 "981": ["binary-search"], "79": ["dfs-flood"], "139": ["backtracking"], "140": ["dp-linear"],
 "131": ["dp-interval"], "560": ["hashing"], "525": ["hashing"], "974": ["hashing"],
 "155": ["pairing-stack"], "232": ["pairing-stack"], "1438": ["monotonic-stack"],
 "239": ["sliding-window"], "75": ["two-pointers"], "31": ["greedy"], "189": ["matrix-sim"],
 "73": ["hashing"], "347": ["hashing"], "23": ["linked-list"], "295": ["design"],
 "480": ["sliding-window"], "218": ["design"], "332": ["backtracking"], "778": ["binary-search"],
 "41": ["dutch-flag"], "238": ["matrix-sim"], "135": ["dp-linear"], "678": ["pairing-stack"],
 "763b": [], "215": ["binary-search"], "912": ["binary-search"], "451": ["hashing"],
 "692": ["hashing"], "1044": [], "11": ["greedy"], "16": ["greedy"], "844": ["pairing-stack"],
 "20": ["design"], "394": ["string-other"], "150": ["string-other"], "227": ["string-other"],
 "224": ["string-other"], "1249": ["greedy"], "921": ["greedy"], "739": ["heap-topk"],
 "84": ["dp-grid"], "85": ["dp-grid"], "907": ["dp-linear"], "402": ["greedy"],
 "316": [], "496": ["hashing"], "1": ["two-pointers"], "49": ["string-other"],
 "242": ["string-other"], "205": ["string-other"], "290": ["string-other"], "207": ["dfs-flood"],
 "210": ["dfs-flood"], "269": ["dfs-flood"], "133": ["bfs"], "994": ["dfs-flood"],
 "286": ["dfs-flood"], "1091": ["dfs-flood"], "127": ["dfs-flood"], "547": ["dfs-flood"],
 "684": [], "323": [], "1584": ["graph-other"], "684b": [], "62": ["dp-linear"],
 "70": ["math-bit"], "509": ["math-bit"], "198": ["greedy"], "152": ["greedy"],
 "918": ["greedy"], "152b": [], "322": ["dp-grid"], "518": ["dp-grid"], "416": ["dp-grid"],
 "1": ["two-pointers"],
}


def load():
    labels = {r["num"]: r["family"] for r in csv.DictReader(open(os.path.join(HERE, "labels.csv")))}
    tax = json.load(open(os.path.join(HERE, "taxonomy.json")))["families"]
    macro = {f: tax[f]["macro"] for f in tax}
    # 6-month stratum: company -> [nums]; importance weight per num across all strata
    sixmo = defaultdict(list)
    imp = defaultdict(float)
    for f in glob.glob(os.path.join(RAW, "*__*.json")):
        if "EXCLUDED" in f:
            continue
        d = json.load(open(f)); n = d["count"]
        for i, num in enumerate(d["order"]):
            imp[str(num)] += (n - i) / n
            if d["window"] == "six-months":
                sixmo[d["company"]].append(str(num))
    # only keep companies with >=40 in 6mo (the inclusion floor)
    sixmo = {c: v for c, v in sixmo.items() if len(v) >= 40}
    return labels, macro, sixmo, imp


def cramers_v_and_table(sixmo, lab, macro):
    macros = sorted({macro[lab[n]] for c in sixmo for n in sixmo[c]})
    mi = {m: i for i, m in enumerate(macros)}
    comps = sorted(sixmo)
    tab = np.zeros((len(comps), len(macros)))
    for ci, c in enumerate(comps):
        for n in sixmo[c]:
            tab[ci, mi[macro[lab[n]]]] += 1
    stat, _, _, _ = chi2_contingency(tab)
    N = tab.sum(); r, k = tab.shape
    V = np.sqrt((stat / N) / max(min(r - 1, k - 1), 1))
    # residuals
    exp = tab.sum(1, keepdims=True) @ tab.sum(0, keepdims=True) / N
    with np.errstate(all="ignore"):
        res = np.nan_to_num((tab - exp) / np.sqrt(exp))
    # top residual cell
    fi = np.unravel_index(np.argmax(np.abs(res)), res.shape)
    top = (comps[fi[0]], macros[fi[1]], res[fi])
    return V, comps, macros, tab, top


def faang5_sig(sixmo, lab, macro):
    from itertools import combinations
    comps = sorted(sixmo)
    pairs, pv = [], []
    for a, b in combinations(comps, 2):
        ms = sorted({macro[lab[n]] for n in sixmo[a] + sixmo[b]})
        mi = {m: i for i, m in enumerate(ms)}
        t = np.zeros((2, len(ms)))
        for r, c in enumerate((a, b)):
            for n in sixmo[c]:
                t[r, mi[macro[lab[n]]]] += 1
        t = t[:, t.sum(0) > 0]
        pairs.append((a, b)); pv.append(chi2_contingency(t)[1])
    rej = multipletests(pv, method="fdr_bh")[0]
    return sum(1 for (a, b), r in zip(pairs, rej)
               if r and a in FAANG5 and b in FAANG5)


NAMED_GEN = {"recursive-fold", "relaxation", "monotone-seq", "hash-running"}

def gen_named4(lab, macro, imp):
    """Coverage by the FOUR named generators (excludes the heterogeneous 'other' residue)."""
    g = defaultdict(float)
    for n, fam in lab.items():
        g[GEN.get(macro[fam], "other")] += imp.get(n, 0.0)
    tot = sum(g.values())
    named = sum(v for k, v in g.items() if k in NAMED_GEN)
    return named / tot * 100


# family -> recursion SCHEME (the paper's single, machine-checked taxonomy). Keyed by FAMILY
# (not macro) so mixed macros split honestly: math-bit family folds but its macro-siblings
# number-theory/geometry do not; string-other (scan) folds but string-matching (KMP) does not.
# Each fold family has a representative `IsFold` certificate in lproofs/Lproofs/Patterns.lean;
# DP families a catamorphism cert; relaxation a least-fixpoint cert; bisection a Nat.find cert.
SCHEME = {
    # streaming fold (one pass with a carried accumulator)
    "two-pointers": "fold", "dutch-flag": "fold", "fast-slow": "fold",
    "hashing": "fold", "monotonic-stack": "fold", "pairing-stack": "fold",
    "string-other": "fold", "linked-list": "fold", "math-bit": "fold",
    "sliding-window": "fold", "merge-intervals": "fold", "diff-array": "fold",
    "prefix-sum": "fold",
    # recursive decomposition / DP (catamorphism / memoized recurrence)
    "dp-bitmask": "dp", "dp-digit": "dp", "dp-grid": "dp", "dp-interval": "dp",
    "dp-knapsack": "dp", "dp-linear": "dp", "dp-tree": "dp", "backtracking": "dp",
    "bst": "dp", "tree-aggregate": "dp", "tree-construct": "dp",
    "tree-traversal": "dp", "trie": "dp",
    # graph relaxation (least fixpoint of a monotone operator)
    "bellman-ford": "relaxation", "bfs": "relaxation", "dfs-flood": "relaxation",
    "dijkstra": "relaxation", "graph-other": "relaxation", "topo-sort": "relaxation",
    "union-find": "relaxation",
    # bisection / divide-and-conquer (monotone-predicate search)
    "binary-search": "bisection",
}
NAMED_SCHEME = ("fold", "dp", "relaxation", "bisection")

def scheme_named4(lab, imp):
    """Frequency-weighted coverage of the four recursion schemes (everything else -> 'tail').
    Returns (total covered %, {scheme: share %}). Denominator is all labeled load."""
    g = defaultdict(float)
    for n, fam in lab.items():
        g[SCHEME.get(fam, "tail")] += imp.get(n, 0.0)
    tot = sum(g.values())
    shares = {s: g[s] / tot * 100 for s in NAMED_SCHEME}
    return sum(shares.values()), shares


def certified_coverage(imp):
    """Frequency-weighted % of load whose problem carries a machine-checked cls+corr certificate.
    Reads certs.csv (generated by build_manifest.py from the Lean files). This is the
    κ-free coverage: the proven mass, which grows toward the labeled four-scheme ~82% as the
    per-problem certification program (lproofs/Lproofs/Problems/) lands. Returns (covered%, n_done)."""
    path = os.path.join(HERE, "certs.csv")
    if not os.path.exists(path):
        return 0.0, 0
    done = {r["num"] for r in csv.DictReader(open(path)) if r["done"] == "True"}
    return sum(imp.get(n, 0.0) for n in done) / sum(imp.values()) * 100, len(done)


def main():
    ap = argparse.ArgumentParser(); ap.add_argument("--draws", type=int, default=2000)
    ap.add_argument("--seed", type=int, default=7); args = ap.parse_args()
    rng = np.random.default_rng(args.seed)
    labels, macro, sixmo, imp = load()

    # build choice sets (primary + alternatives, dedup, valid families only)
    valid = set(macro)
    choices = {}
    for num, prim in labels.items():
        alts = [a for a in ALT.get(num, []) if a in valid and a != prim]
        if alts:
            choices[num] = [prim] + alts
    amb = len(choices)
    amb_wt = sum(imp.get(n, 0) for n in choices) / sum(imp.values()) * 100
    print(f"ambiguous problems perturbed: {amb}/{len(labels)} "
          f"({amb_wt:.0f}% of frequency-weighted load carries a defensible alternative)\n")

    # baseline
    V0, comps0, _, _, top0 = cramers_v_and_table(sixmo, labels, macro)
    f0 = faang5_sig(sixmo, labels, macro)
    g0 = gen_named4(labels, macro, imp)
    s0, sh0 = scheme_named4(labels, imp)
    print(f"baseline:  V={V0:.3f}   FAANG-5 sig pairs={f0}/10   "
          f"top-residual={top0[0]}x{top0[1]} ({top0[2]:+.1f})\n")
    print(f"four-scheme coverage: {s0:.1f}%  "
          f"(fold {sh0['fold']:.1f} · dp {sh0['dp']:.1f} · relaxation {sh0['relaxation']:.1f} "
          f"· bisection {sh0['bisection']:.1f})   [cross-check four-generator cov={g0:.1f}%]")
    cc, ncc = certified_coverage(imp)
    print(f"certified (κ-free) coverage: {cc:.2f}%  ({ncc} problems machine-checked cls+corr, "
          f"target {s0:.1f}%)\n")

    Vs, fs, ubers, ss = [], [], [], []
    for _ in range(args.draws):
        lab = dict(labels)
        for num, opts in choices.items():
            lab[num] = opts[rng.integers(len(opts))]
        V, comps, _, _, top = cramers_v_and_table(sixmo, lab, macro)
        Vs.append(V)
        fs.append(faang5_sig(sixmo, lab, macro))
        ubers.append(top[0] == "uber")
        ss.append(scheme_named4(lab, imp)[0])
    Vs, fs, ss = np.array(Vs), np.array(fs), np.array(ss)

    def band(x): return f"median {np.median(x):.3f}  [5-95%: {np.percentile(x,5):.3f}-{np.percentile(x,95):.3f}]"
    print(f"=== over {args.draws} relabelings ===")
    print(f"(A) Cramer's V (6mo):            {band(Vs)}")
    print(f"    stays 'small' (V<0.25):      {(Vs<0.25).mean()*100:.1f}% of draws")
    print(f"(B) FAANG-5 significant pairs:   median {int(np.median(fs))}  max {int(fs.max())}  "
          f"of 10")
    print(f"    FAANG-5 fully indistinct (0):{ (fs==0).mean()*100:.1f}% of draws")
    print(f"(C) Uber is top outlier cell:    {np.mean(ubers)*100:.1f}% of draws")
    print(f"(D) four-scheme coverage:        median {np.median(ss):.0f}%  "
          f"[5-95%: {np.percentile(ss,5):.0f}-{np.percentile(ss,95):.0f}%]")
    print(f"    stays >= 80%:                {(ss>=80).mean()*100:.1f}% of draws")


if __name__ == "__main__":
    main()
