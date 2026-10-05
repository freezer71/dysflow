#!/usr/bin/env bash
# Mesure les seuils de la spec sur Sillon.app construite en Release.
set -euo pipefail
cd "$(dirname "$0")/.."
APP=macos/build/Release/Sillon.app
# Toujours reconstruire (incrémental), sinon on mesure une app périmée.
if [ "${1:-}" != "--sans-construire" ]; then
    ./scripts/build-macos.sh
fi
SORTIE=/tmp/sillon-mesures.txt
JOURNAL=/tmp/sillon-mesures.err
# grep sans correspondance ne doit pas masquer le vrai diagnostic (|| true).
"$APP/Contents/MacOS/Sillon" --measure 2>"$JOURNAL" | { grep '^MESURE' || true; } | tee "$SORTIE"
if [ ! -s "$SORTIE" ]; then
    echo "aucune ligne MESURE : l'app a planté ou n'a rien produit. Fin du journal d'erreurs :" >&2
    tail -n 20 "$JOURNAL" >&2
    exit 1
fi
if grep -q '^MESURE erreur' "$SORTIE"; then
    echo "mesure interrompue : voir la ligne « MESURE erreur » ci-dessus" >&2
    exit 1
fi
echo "--- médianes ---"
for name in froid_premier_son chaud_premier_son saut_phrase saut_mot; do
    grep " $name " "$SORTIE" | awk '{print $3}' | sort -n \
        | awk -v n="$name" '{a[NR]=$1} END {print n, a[int((NR+1)/2)], "ms"}'
done
grep memoire_repos "$SORTIE" | awk '{print $2, $3, $4}'
