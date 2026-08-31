#!/bin/sh
# Rend la première page de spec.pdf en PNG, pour servir de capture d'écran de
# test. Dépend de macOS ; la fixture étant versionnée, ce script ne sert qu'à
# la régénérer.
set -e
cd "$(dirname "$0")"
qlmanage -t -s 1100 -o . spec.pdf >/dev/null
sips -s format png -Z 900 spec.pdf.png --out capture.png >/dev/null
rm -f spec.pdf.png
echo "écrit capture.png"
