#!/usr/bin/env bash
# Build the paper for every target venue from the single source paper.tex.
set -euo pipefail
cd "$(dirname "$0")"
T="${1:-all}"

if [ "$T" = "arxiv" ] || [ "$T" = "pdf" ] || [ "$T" = "all" ]; then
  echo ">> arXiv / camera-ready PDF (tectonic)"
  tectonic paper.tex
fi

if [ "$T" = "blog" ] || [ "$T" = "all" ]; then
  echo ">> blog: standalone HTML (MathML) + GitHub-flavored Markdown"
  pandoc paper.tex --from latex --to html5 --mathml --standalone \
    --metadata title="Wide but Shallow" -o paper.html
  pandoc paper.tex --from latex --to gfm -o paper.md
fi

cat <<'NOTE'

Targets:
  ./build.sh arxiv   -> paper.pdf            (upload paper.tex + figs/*.png to arXiv; category cs.CY)
  ./build.sh blog    -> paper.html, paper.md (drop into Substack / static site)
  ./build.sh all     -> everything (default)

SIGCSE / ITiCSE short paper: trim to the page limit by compressing Sections 5-6
(formal + Lean) to one paragraph; the empirical half (Sections 3-4) is the contribution.
NOTE
