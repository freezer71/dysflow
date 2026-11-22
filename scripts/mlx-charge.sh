#!/usr/bin/env bash
# La porte du lot 3, en une commande : les deux copies de MLX sous charge
# dans un même processus, en Release (macos/Tests/MLXLoadTests.swift).
#
#   DYSFLOW_CHARGE_MODELES=<dossier> ./scripts/mlx-charge.sh           # la porte : 200 tours + 10 min alternées
#   DYSFLOW_CHARGE_MODELES=<dossier> ./scripts/mlx-charge.sh --court   # essai : 10 tours + 1 min, indicatif
#
# <dossier> contient kokoro/ et parakeet_tdt_v3/ : une copie (clone APFS,
# `cp -c`), jamais la bibliothèque de l'utilisateur.
#
# La porte veut une machine au calme : charge (1 min) < 2 et aucun swap au
# départ, sinon elle est refusée (code 3). `--court` passe toujours, et son
# résumé dit « indicatif » si la machine n'était pas au calme.
#
# Étapes : moteur avec MLX en Rust (FEATURES=ocr,mlx ./scripts/build-ffi.sh),
# test en Release sans rien montrer (DYSFLOW_TESTS_RELEASE=1
# ./scripts/test-macos.sh MLXLoadTests) dans un HOME temporaire (ni ~/.mlx
# ni la bibliothèque de l'utilisateur), charge et swap relevés au début et à
# la fin. Le moteur par défaut (./scripts/build-ffi.sh) est remis à la fin,
# échec compris. Rien n'est joué, rien ne s'affiche.
#
# Sortie : target/runs/mlx-charge-<date>/ (xcodebuild.log, resume.txt) ; le
# résumé est aussi écrit à l'écran. Code 0 : porte passée ; 1 : échouée ;
# 2 : mauvais usage ; 3 : machine pas au calme.
set -euo pipefail
cd "$(dirname "$0")/.."

TOURS=200
ALTERNANCE_S=600
MODE=porte
case "${1:-}" in
"") ;;
--court)
    TOURS=10
    ALTERNANCE_S=60
    MODE=court
    ;;
*)
    echo "argument inconnu : $1 (--court ou rien)" >&2
    exit 2
    ;;
esac

MODELES=${DYSFLOW_CHARGE_MODELES:-}
if [ -z "$MODELES" ] || [ ! -f "$MODELES/kokoro/config.json" ] || [ ! -f "$MODELES/parakeet_tdt_v3/model.safetensors" ]; then
    echo "Erreur : DYSFLOW_CHARGE_MODELES doit contenir kokoro/ et parakeet_tdt_v3/" >&2
    exit 2
fi
MODELES=$(cd "$MODELES" && pwd -P)
case "$MODELES/" in
"$(cd ~ && pwd -P)/Library/"*)
    echo "Erreur : $MODELES est dans la bibliothèque de l'utilisateur ; en faire une copie (cp -c)" >&2
    exit 2
    ;;
esac

# La machine : charge sur 1, 5 et 15 min ; swap utilisé, en Mo.
charge() { LC_ALL=C sysctl -n vm.loadavg | tr -d '{}' | awk '{print $1, $2, $3}'; }
swap_mo() { LC_ALL=C sysctl -n vm.swapusage | sed -E 's/.*used = ([0-9.]+)M.*/\1/'; }
au_calme() {
    awk -v c="$1" -v s="$2" 'BEGIN { exit !(c < 2 && s == 0) }'
}

CHARGE_DEBUT=$(charge)
SWAP_DEBUT=$(swap_mo)
VALIDE=oui
if ! au_calme "${CHARGE_DEBUT%% *}" "$SWAP_DEBUT"; then
    if [ "$MODE" = porte ]; then
        echo "Refusé : machine pas au calme (charge $CHARGE_DEBUT, swap $SWAP_DEBUT Mo) ; il faut charge < 2 et aucun swap" >&2
        exit 3
    fi
    VALIDE="non (charge $CHARGE_DEBUT, swap $SWAP_DEBUT Mo au départ) : indicatif"
fi

SORTIE=target/runs/mlx-charge-$(date +%Y%m%d-%H%M%S)
mkdir -p "$SORTIE"
MAISON=$(mktemp -d -t dysflow-charge-home)

# Le moteur par défaut remis, et le HOME temporaire retiré, à toute sortie.
# shellcheck disable=SC2329 # appelée par trap
remettre() {
    rm -rf "$MAISON"
    echo "Moteur par défaut remis (./scripts/build-ffi.sh)…"
    ./scripts/build-ffi.sh >"$SORTIE/build-ffi-defaut.log" 2>&1 || echo "Erreur : ./scripts/build-ffi.sh a échoué ($SORTIE/build-ffi-defaut.log)" >&2
}
trap remettre EXIT

echo "Moteur avec MLX en Rust (FEATURES=ocr,mlx)…"
FEATURES=ocr,mlx ./scripts/build-ffi.sh >"$SORTIE/build-ffi-mlx.log" 2>&1

echo "Porte ($MODE : $TOURS tours, $ALTERNANCE_S s alternées), journal $SORTIE/xcodebuild.log…"
code=0
DYSFLOW_TESTS_RELEASE=1 \
    TEST_RUNNER_HOME="$MAISON" \
    TEST_RUNNER_DYSFLOW_KOKORO_DIR="$MODELES/kokoro" \
    TEST_RUNNER_DYSFLOW_STT_MODELS="$MODELES" \
    TEST_RUNNER_DYSFLOW_CHARGE_TOURS="$TOURS" \
    TEST_RUNNER_DYSFLOW_CHARGE_ALTERNANCE_S="$ALTERNANCE_S" \
    ./scripts/test-macos.sh MLXLoadTests >"$SORTIE/xcodebuild.log" 2>&1 || code=$?

CHARGE_FIN=$(charge)
SWAP_FIN=$(swap_mo)
if [ "$code" = 0 ] && grep -q "Test Case .*testBothMLXCopiesUnderLoad.* passed" "$SORTIE/xcodebuild.log" \
    && grep -q "MESURE charge configuration=Release tours=$TOURS " "$SORTIE/xcodebuild.log"; then
    VERDICT="PASSÉE"
else
    VERDICT="ÉCHOUÉE (xcodebuild $code)"
fi
{
    echo "Porte du lot 3 : $VERDICT"
    echo "mode $MODE : $TOURS tours, $ALTERNANCE_S s alternées ; mesure valide : $VALIDE"
    echo "début : charge $CHARGE_DEBUT, swap $SWAP_DEBUT Mo"
    echo "fin   : charge $CHARGE_FIN, swap $SWAP_FIN Mo"
    grep -E "MESURE charge|error:|Test Case .*(passed|failed)|skipped" "$SORTIE/xcodebuild.log" | sed 's/^.*MESURE/MESURE/' || true
} | tee "$SORTIE/resume.txt"
case "$VERDICT" in
PASSÉE) exit 0 ;;
*) exit 1 ;;
esac
