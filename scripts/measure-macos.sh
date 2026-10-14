#!/usr/bin/env bash
# Mesure les seuils de la spec sur Sillon.app construite en Release.
set -euo pipefail
cd "$(dirname "$0")/.."
APP=macos/build/Release/Sillon.app
# Toujours reconstruire (incrémental), sinon on mesure une app périmée.
CHARGE=0
CONSTRUIRE=1
SECOND=0
usage() {
    echo "usage : $0 [--sans-construire] [--charge | --second-lancement]" >&2
    echo "  --charge            filtre de charge (pilote du banc, 7 verdicts)" >&2
    echo "  --second-lancement  juge froid_premier_son (1er) <= 300 ms : à lancer au SECOND lancement de la version" >&2
    exit 2
}
for arg in "$@"; do
    case "$arg" in
    --charge) CHARGE=1 ;;
    --sans-construire) CONSTRUIRE=0 ;;
    --second-lancement) SECOND=1 ;;
    *) echo "argument inconnu : $arg" >&2; usage ;;
    esac
done
[ "$CHARGE" = 1 ] && [ "$SECOND" = 1 ] && usage
if [ "$CONSTRUIRE" = 1 ]; then
    ./scripts/build-macos.sh
fi
# Filtre de charge : le pilote du banc échantillonne l'app pendant son scénario.
if [ "$CHARGE" = 1 ]; then
    PY=${SILLON_BENCH_PY:-$HOME/Library/Caches/sillon-bench/light/venv-mlx/bin/python}
    exec "$PY" -I scripts/mesure-charge.py app --app "$APP" --out /tmp/sillon-charge.json
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
# Le tout premier démarrage à froid du lancement : la règle « une fois par
# version » (noyaux Metal compilés à la volée) se juge au SECOND lancement de
# la même version. Au-delà de 0,3 s, préparer à chaque activation de Kokoro
# avec une phrase réaliste.
PREMIER=$(grep ' froid_premier_son ' "$SORTIE" | head -n 1 | awk '{print $3}')
echo "froid_premier_son (1er de ce lancement) ${PREMIER:-?} ms"
SORTIE_CODE=0
if [ "$SECOND" = 1 ]; then
    if [ -n "$PREMIER" ] && [ "$PREMIER" -le 300 ]; then
        echo "OK  second lancement : froid_premier_son ${PREMIER} ms <= 300 ms"
    else
        echo "NON second lancement : froid_premier_son ${PREMIER:-?} ms > 300 ms : préparer à chaque activation de Kokoro avec une phrase réaliste"
        SORTIE_CODE=1
    fi
fi
grep memoire_repos "$SORTIE" | awk '{print $2, $3, $4}'
exit $SORTIE_CODE
