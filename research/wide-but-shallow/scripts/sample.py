"""sample.py — draw a seeded uniform random sample of ALL problems, to MEASURE the scheme
distribution by Lean proof.

Design (no labeling in the loop): draw a uniform random sample over ALL distinct problems, then
classify each sampled problem BY PROOF into fold / DP / relaxation / bisection / tail. The proven
per-scheme fractions are an unbiased estimate of the population scheme distribution. The `family` and
`scheme` columns written here are bookkeeping/cross-check only — the reported number comes from the
Lean certs (build_manifest.py), not from these labels.

Uniform over the full population (labels.csv master list, 769 distinct problems), so tail problems
are eligible and must genuinely fail to classify. Reproducible from the recorded seed.
Run: `python sample.py [--n 100] [--seed 7]`.
"""
import argparse, csv, os, random
from collections import Counter
from sensitivity import SCHEME

HERE = os.path.dirname(os.path.abspath(__file__))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--n", type=int, default=100)
    ap.add_argument("--seed", type=int, default=7)
    args = ap.parse_args()

    labels = {r["num"]: r["family"] for r in csv.DictReader(open(os.path.join(HERE, "labels.csv")))}
    population = sorted(labels.keys(), key=int)  # ALL distinct problems (incl. tail)
    n = min(args.n, len(population))
    rng = random.Random(args.seed)
    chosen = sorted(rng.sample(population, n), key=int)

    # which sampled problems are certified? Single source of truth: certs.csv `done`
    # (= cls AND corr AND no sorry, parsed from the Lean files by build_manifest.py). Deriving
    # `proven` from the manifest — not from mere file-existence — keeps this column from drifting.
    certs_path = os.path.join(HERE, "certs.csv")
    proven = {r["num"] for r in csv.DictReader(open(certs_path)) if r["done"] == "True"}

    with open(os.path.join(HERE, "sample.csv"), "w", newline="") as fh:
        w = csv.writer(fh)
        w.writerow(["num", "family", "scheme", "proven"])
        for num in chosen:
            sch = SCHEME.get(labels[num], "tail")
            w.writerow([num, labels[num], sch, num in proven])

    by = Counter(SCHEME.get(labels[n], "tail") for n in chosen)
    ndone = sum(1 for n in chosen if n in proven)
    print(f"sample: n={n} of {len(population)} all distinct problems  (seed={args.seed})")
    for s in ("fold", "dp", "relaxation", "bisection", "tail"):
        print(f"  {s:11s} {by[s]}")
    print(f"already certified within sample: {ndone}/{n}  (the in-scheme uncertified are the target)")
    print("wrote sample.csv  (family/scheme columns are bookkeeping; the count comes from Lean certs)")


if __name__ == "__main__":
    main()
