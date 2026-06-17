"""make_scheme_fig.py — scheme-level (company x recursion-scheme) standardized-residual heatmap.

The on-thesis figure for the company result: at the four-scheme level Uber's row (relaxation-heavy,
fold-poor) is the lone outlier and the other six firms are flat — the visual form of the V = 0.091,
"six indistinguishable, only Uber" headline. Reuses analyze.build_long / contingency / std_residuals,
so it reproduces from frequencies.csv (no raw scrape). Run: python make_scheme_fig.py
"""
import csv, os
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
from analyze import build_long, contingency, std_residuals, _find, HERE

SCHEME_ORDER = ["fold", "dp", "relaxation", "bisection", "tail"]


def main():
    rule = {r["surface_family"]: r["scheme"]
            for r in csv.DictReader(open(_find("family_scheme_rule.csv")))}
    df = build_long("count")
    df = df.assign(macro=df["family"].map(lambda f: rule.get(f, "tail")))
    tab = contingency(df[df.window == "all"])
    cols = [c for c in SCHEME_ORDER if c in tab.columns]
    rows = [r for r in tab.index if r != "uber"] + (["uber"] if "uber" in tab.index else [])
    tab = tab.loc[rows, cols]
    res = std_residuals(tab)

    vmax = float(np.abs(res.values).max())
    fig, ax = plt.subplots(figsize=(6, 4))
    im = ax.imshow(res.values, cmap="RdBu_r", vmin=-vmax, vmax=vmax, aspect="auto")
    ax.set_xticks(range(len(cols))); ax.set_xticklabels(cols, rotation=30, ha="right")
    ax.set_yticks(range(len(rows))); ax.set_yticklabels(rows)
    for i in range(len(rows)):
        for j in range(len(cols)):
            ax.text(j, i, f"{res.values[i, j]:+.1f}", ha="center", va="center", fontsize=8)
    fig.colorbar(im, ax=ax, label="standardized residual")
    ax.set_title("Company × recursion scheme — standardized residuals")
    fig.tight_layout()
    out = os.path.join(HERE, "figs")
    os.makedirs(out, exist_ok=True)
    fig.savefig(os.path.join(out, "residual_scheme.png"), dpi=150)
    plt.close(fig)
    print("wrote figs/residual_scheme.png\n")
    print(res.round(2).to_string())


if __name__ == "__main__":
    main()
