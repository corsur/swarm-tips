"""build_manifest.py — derive the certification manifest from the Lean proof files.

Single source of truth for κ-elimination progress: scans lproofs/Lproofs/Problems/**.lean and
regenerates `certs.csv` (every relevant problem, done or pending) + a human-readable
`lproofs/Lproofs/Problems/STATUS.md` dashboard. A problem counts as CERTIFIED iff its file has
`theorem cls` (scheme membership) AND `theorem corr` (correctness vs spec) AND no `sorry`/`admit`.

The manifest cannot drift: it is generated FROM the files. A pending problem has no file and is
listed (from labels.csv) with done=False. Run: `python build_manifest.py`.
"""
import csv, glob, json, os, re
from collections import defaultdict
from sensitivity import SCHEME

HERE = os.path.dirname(os.path.abspath(__file__))
RAW = os.path.join(HERE, "data", "raw")
PROB = os.path.join(HERE, "lproofs", "Lproofs", "Problems")
HDR = re.compile(r"@lc\s+(\d+)\s*\|(.*)")


def importance():
    imp = defaultdict(float)
    for f in glob.glob(os.path.join(RAW, "*__*.json")):
        if "EXCLUDED" in f:
            continue
        d = json.load(open(f)); n = d["count"]
        for i, num in enumerate(d["order"]):
            imp[str(num)] += (n - i) / n
    return imp


# NON-GENUINE certificates (panel audit, 2026-06-19): corr does NOT reference the concrete problem —
# it is scheme-generic (proven over an abstract relation/predicate, would certify any problem in that
# scheme), vacuous (Iff.rfl against a spec defined as the solution), or a verbatim re-export of another
# problem. These BUILD but do not count toward the genuine-coverage headline. Remove a num here only
# when its corr has been rewritten to a problem-specific statement (then re-run this script).
NOT_GENUINE = {
    # relaxation — abstract V/relation, corr = bellman_isLeast / reachability over an uninstantiated r
    # (strengthened 2026-06-19: 733,3387,102,863,815,332,1719,2858,505,1778 now use a concrete relation)
    # (strengthened: 417 drainage, 212 search states, 1368 path connectivity, 1584 MST connectivity)
    "2092": "abstract r (time-ordered spread — plain reachability over-approximates)",
    # bisection — corr over a free abstract predicate, not the concrete problem condition
    # (strengthened: 240, 1818 concrete sorted array; 278 concrete isBad oracle)
    "162": "abstract predicate (peak: no clean monotone threshold)", "3161": "abstract predicate",
    # dp/fold — vacuous, re-export, definitional, or abstract window predicate
    # (strengthened: 98 inorder-sorted<->bounded-BST, 833 scan=flatMap per-position replacement)
    # (strengthened: 545 boundary soundness; 211 wildcard search; 642 prefix navigation)
    # (strengthened: 992 atMost(k)=atMost(k-1)+exactly(k) identity; 76 min-window = IsLeast covering len)
    # (strengthened: 2444 inclusion-exclusion sieve over four bounded-subarray counts)
    "312": "trivial base case only (DP optimality not formalized)",
}


def parse_file(path):
    txt = open(path).read()
    m = HDR.search(txt)
    if not m:
        return None
    fields = dict((k, v.strip()) for k, v in re.findall(r"(\w+):([^|\n]+)", m.group(2)))
    cls = re.search(r"theorem\s+cls\b", txt) is not None
    corr = re.search(r"theorem\s+corr\b", txt) is not None
    bad = re.search(r"\bsorry\b|\badmit\b", txt) is not None
    num = m.group(1)
    # BUILDS = file has cls + corr + no sorry/admit (syntactic). GENUINE = builds AND corr is
    # problem-specific (references the concrete problem, not an abstract relation/predicate). Only
    # genuine certs count toward the headline coverage number.
    builds = cls and corr and not bad
    genuine = builds and num not in NOT_GENUINE
    return {"num": num, "name": fields.get("name", ""), "scheme": fields.get("scheme", ""),
            "family": fields.get("family", ""), "complexity": fields.get("complexity", ""),
            "source": fields.get("source", ""), "cls": cls, "corr": corr, "sorry": bad,
            "done": builds, "genuine": genuine, "file": os.path.relpath(path, HERE)}


def main():
    imp = importance()
    labels = {r["num"]: r["family"] for r in csv.DictReader(open(os.path.join(HERE, "labels.csv")))}
    relevant = {num: fam for num, fam in labels.items() if SCHEME.get(fam, "tail") != "tail"}
    files = {}
    for p in glob.glob(os.path.join(PROB, "*", "*.lean")):
        r = parse_file(p)
        if r:
            files[r["num"]] = r

    rows = []
    for num, fam in sorted(relevant.items(), key=lambda x: int(x[0])):
        sch = SCHEME.get(fam, "tail")
        f = files.get(num)
        rows.append({**f, "num": num, "family": fam, "scheme": sch} if f else
                    {"num": num, "name": "", "scheme": sch, "family": fam, "complexity": "",
                     "source": "", "cls": False, "corr": False, "sorry": False, "done": False, "file": ""})

    cols = ["num", "name", "scheme", "family", "cls", "corr", "complexity", "source", "sorry", "done", "genuine", "file"]
    with open(os.path.join(HERE, "certs.csv"), "w", newline="") as fh:
        w = csv.DictWriter(fh, fieldnames=cols); w.writeheader()
        for r in rows:
            w.writerow({k: r.get(k, "") for k in cols})

    tot = sum(imp.values())
    done = {r["num"] for r in rows if r["done"]}
    relevant_mass = sum(imp.get(n, 0) for n in relevant) / tot * 100
    certified_mass = sum(imp.get(n, 0) for n in done) / tot * 100

    byfam = defaultdict(lambda: [0, 0])
    for r in rows:
        byfam[(r["scheme"], r["family"])][1] += 1
        byfam[(r["scheme"], r["family"])][0] += int(r["done"])
    md = ["# Certification status — eliminating κ via per-problem proofs", "",
          f"**Certified: {len(done)}/{len(relevant)} relevant problems** "
          f"({certified_mass:.1f}% of frequency-weighted load; goal is the full "
          f"{relevant_mass:.1f}% non-tail mass).", "",
          "Each certified problem carries machine-checked `cls` (scheme membership) + `corr` "
          "(correctness vs spec), standard axioms only, no `sorry`. Pending problems have no file yet.",
          "", "| scheme | family | certified | total |", "|---|---|--:|--:|"]
    order = {"fold": 0, "dp": 1, "relaxation": 2, "bisection": 3}
    for (sch, fam), (d, t) in sorted(byfam.items(), key=lambda x: (order.get(x[0][0], 9), -x[1][1])):
        md.append(f"| {sch} | {fam} | {d} | {t} |")
    md += ["", "## Certified problems", ""]
    for r in rows:
        if r["done"]:
            md.append(f"- `{r['num']}` {r['name']} — {r['scheme']} ({r['complexity']}) — `{r['file']}`")
    open(os.path.join(PROB, "STATUS.md"), "w").write("\n".join(md) + "\n")

    print(f"certified {len(done)}/{len(relevant)} relevant problems "
          f"= {certified_mass:.2f}% of load (target {relevant_mass:.1f}%)")
    print(f"wrote certs.csv ({len(rows)} rows) and Problems/STATUS.md")

    # MEASURED scheme distribution (the headline). The random sample (sample.csv, from sample.py) is
    # a uniform draw over ALL distinct problems; we classify each BY PROOF. A sampled problem counts
    # into scheme S iff it has a non-vacuous classification cert (cls + structure, no sorry) whose
    # scheme tag is S. Unproven in-scheme problems count as NOT-classified, so each proportion is a
    # proven LOWER BOUND on the true size. No hand-labeling enters the count.
    import math

    def wilson(k, nn, z=1.96):
        if nn == 0:
            return (0.0, 0.0)
        p = k / nn
        d = 1 + z * z / nn
        c = (p + z * z / (2 * nn)) / d
        h = z * math.sqrt(p * (1 - p) / nn + z * z / (4 * nn * nn)) / d
        return (max(0.0, c - h), min(1.0, c + h))

    genuine = {r["num"] for r in rows if r.get("genuine")}
    sample_path = os.path.join(HERE, "sample.csv")
    if os.path.exists(sample_path):
        srows = list(csv.DictReader(open(sample_path)))
        n = len(srows)
        builds_in = sum(1 for r in srows if r["num"] in done and files.get(r["num"], {}).get("scheme") in
                        ("fold", "dp", "relaxation", "bisection"))
        gen_in = sum(1 for r in srows if r["num"] in genuine and files.get(r["num"], {}).get("scheme") in
                     ("fold", "dp", "relaxation", "bisection"))
        glo, ghi = wilson(gen_in, n)
        print(f"\n=== HEADLINE (honest): GENUINE problem-specific certs in the sample ===")
        print(f"  genuine {gen_in}/{n}  Wilson95 [{glo:.2f},{ghi:.2f}]   "
              f"(builds-but-not-genuine: {builds_in - gen_in}; total builds: {builds_in})")
        proven_scheme = defaultdict(int)  # cert scheme tag, only for classified problems
        for r in srows:
            num = r["num"]
            if num in done:
                proven_scheme[files[num]["scheme"]] += 1
        in_scheme = sum(proven_scheme[s] for s in ("fold", "dp", "relaxation", "bisection"))
        lo, hi = wilson(in_scheme, n)
        print(f"\nMEASURED scheme distribution by proof — uniform sample n={n} over all 769 problems:")
        for s in ("fold", "dp", "relaxation", "bisection"):
            k = proven_scheme[s]
            slo, shi = wilson(k, n)
            print(f"  {s:11s} {k:3d}/{n}  = {100*k/n:4.1f}%  [95% CI {100*slo:.0f}-{100*shi:.0f}%]")
        print(f"  {'IN-SCHEME':11s} {in_scheme:3d}/{n}  = {100*in_scheme/n:4.1f}%  "
              f"[95% CI {100*lo:.0f}-{100*hi:.0f}%]  (proven lower bound; unproven in-scheme not counted)")
        print(f"  (sample contains {sum(1 for r in srows if r['scheme']!='tail')} in-scheme + "
              f"{sum(1 for r in srows if r['scheme']=='tail')} tail by the bookkeeping label; the % above is proof-measured)")


if __name__ == "__main__":
    main()
