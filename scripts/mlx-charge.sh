#!/usr/bin/env bash
# La porte du lot 3, en une commande : les deux copies de MLX sous charge
# dans un même processus, en Release (macos/Tests/MLXLoadTests.swift).
#
#   DYSFLOW_CHARGE_MODELES=<dossier> ./scripts/mlx-charge.sh           # la porte : 200 tours + 10 min alternées
#   DYSFLOW_CHARGE_MODELES=<dossier> ./scripts/mlx-charge.sh --court   # essai : 10 tours + 1 min, indicatif
#   ./scripts/mlx-charge.sh --verifier-calme                           # la machine est-elle au calme ? (0 / 3)
#   ./scripts/mlx-charge.sh --juger <xcodebuild.log>                   # le verdict d'un journal déjà là
#
# <dossier> contient kokoro/ et parakeet_tdt_v3/ : une copie (clone APFS,
# `cp -c`), jamais la bibliothèque de l'utilisateur.
#
# Au calme : charge sur 1 min < 2 et aucun swap. Ordre :
#   1. swap au départ : la porte refuse s'il y en a (attendre n'y change rien) ;
#   2. construction (moteur FEATURES=ocr,mlx, puis build-for-testing) ;
#   3. attente du calme, DYSFLOW_CHARGE_ATTENTE_S au plus (900 s) : la
#      construction a chargé la machine ; toujours pas au calme : refusée ;
#   4. test sans reconstruire (test-without-building), HOME temporaire (ni
#      ~/.mlx ni la bibliothèque de l'utilisateur), durée bornée ;
#   5. jugement : la mesure n'est valide que si la machine était au calme au
#      départ, au premier relevé du test (`MESURE charge debut`) et à la fin
#      (aucun swap apparu).
# `--court` n'attend ni ne refuse : son résumé dit « indicatif ».
# Le moteur par défaut (./scripts/build-ffi.sh) est remis à la fin, échec
# compris. Rien n'est joué, rien ne s'affiche.
#
# Verdict : celui de testBothMLXCopiesUnderLoad seul. Les mesures R15
# (testR15RacesMeasuredApart) sont rapportées à part et n'y entrent pas.
#
# Sortie : target/runs/mlx-charge-<date>/ (journaux, resume.txt) ; le résumé
# est aussi écrit à l'écran. Codes : 0 porte passée, mesure valide ;
# 1 porte échouée ; 2 mauvais usage ; 3 machine pas au calme (refusée) ;
# 4 porte passée mais mesure invalide (machine pas au calme pendant la
# mesure : à refaire). `--court` passé rend 0 même indicatif.
#
# Essais seulement : DYSFLOW_CHARGE_LECTURE="c1 c5 c15 swap_mo" remplace les
# relevés de la machine (DYSFLOW_CHARGE_LECTURE_FIN ceux de la fin). Dans une
# vraie mesure, une lecture injectée la rend invalide.
set -euo pipefail
cd "$(dirname "$0")/.."

# La machine : « c1 c5 c15 swap_mo ».
lecture() {
    if [ -n "${DYSFLOW_CHARGE_LECTURE:-}" ]; then
        echo "$DYSFLOW_CHARGE_LECTURE"
        return
    fi
    local swap
    swap=$(LC_ALL=C sysctl -n vm.swapusage | sed -E 's/.*used = ([0-9.]+)M.*/\1/')
    echo "$(LC_ALL=C sysctl -n vm.loadavg | tr -d '{}' | awk '{print $1, $2, $3}') $swap"
}
lecture_fin() {
    if [ -n "${DYSFLOW_CHARGE_LECTURE_FIN:-}" ]; then
        echo "$DYSFLOW_CHARGE_LECTURE_FIN"
    else
        lecture
    fi
}
# au_calme <charge 1 min> <swap Mo>
au_calme() {
    awk -v c="$1" -v s="$2" 'BEGIN { exit !(c < 2 && s == 0) }'
}
# Lecture « c1 c5 c15 swap » → texte.
dire() {
    # shellcheck disable=SC2086 # découpage voulu
    set -- $1
    echo "charge $1 $2 $3, swap $4 Mo"
}

# juger <journal> <tours attendus ou vide> <lecture du départ> <lecture de la fin> <mode> [essai]
# Écrit le résumé et rend le code de sortie de la porte. `essai` (--juger) :
# une lecture injectée y est voulue, elle ne rend pas la mesure invalide.
juger() {
    local journal=$1 tours=$2 depart=$3 fin=$4 mode=$5 essai=${6:-}
    local verdict valide="oui" raisons=() debut c1 swap
    if grep -q "Test Case .*testBothMLXCopiesUnderLoad.* passed" "$journal" \
        && grep -q "MESURE charge configuration=Release tours=${tours:-[0-9]*} " "$journal"; then
        verdict="PASSÉE"
    else
        verdict="ÉCHOUÉE"
    fi
    # shellcheck disable=SC2086 # découpage voulu
    set -- $depart
    au_calme "$1" "$4" || raisons+=("au départ : $(dire "$depart")")
    debut=$(grep -m1 "MESURE charge debut " "$journal" || true)
    if [ -n "$debut" ]; then
        c1=$(sed -E 's/.*charge=([0-9.]+),.*/\1/' <<<"$debut")
        swap=$(sed -E 's/.*swap_mo=([0-9]+).*/\1/' <<<"$debut")
        au_calme "$c1" "$swap" || raisons+=("au premier relevé du test : charge $c1, swap $swap Mo")
    else
        raisons+=("pas de premier relevé du test")
    fi
    # shellcheck disable=SC2086
    set -- $fin
    awk -v s="$4" 'BEGIN { exit !(s == 0) }' || raisons+=("à la fin : swap $4 Mo")
    if [ -z "$essai" ] && [ -n "${DYSFLOW_CHARGE_LECTURE:-}${DYSFLOW_CHARGE_LECTURE_FIN:-}" ]; then
        raisons+=("lecture de la machine injectée (essai)")
    fi
    if [ "${#raisons[@]}" -gt 0 ]; then
        valide="non ($(IFS=';'; echo "${raisons[*]}")) : indicatif"
    fi
    local r15
    if grep -q "Test Case .*testR15RacesMeasuredApart.* passed" "$journal"; then
        r15="relevées"
    elif grep -q "Test Case .*testR15RacesMeasuredApart.* failed" "$journal"; then
        r15="en échec (hors verdict)"
    else
        r15="absentes ou interrompues (hors verdict)"
    fi
    echo "Porte du lot 3 : $verdict"
    echo "mode $mode ; mesure valide : $valide"
    echo "départ : $(dire "$depart")"
    echo "fin    : $(dire "$fin")"
    grep -E "MESURE charge|error:|Test Case .*(passed|failed)|skipped" "$journal" \
        | grep -v "MESURE r15" | sed 's/^.*MESURE/MESURE/' || true
    echo "--- R15, à part du verdict : $r15 ---"
    grep "MESURE r15" "$journal" | sed 's/^.*MESURE/MESURE/' || true
    if [ "$verdict" != PASSÉE ]; then
        return 1
    elif [ "$valide" != oui ] && [ "$mode" = porte ]; then
        return 4
    fi
    return 0
}

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
--verifier-calme)
    l=$(lecture)
    # shellcheck disable=SC2086
    set -- $l
    if au_calme "$1" "$4"; then
        echo "au calme : $(dire "$l")"
        exit 0
    fi
    echo "pas au calme : $(dire "$l") ; il faut charge < 2 et aucun swap"
    exit 3
    ;;
--juger)
    if [ ! -f "${2:-}" ]; then
        echo "Erreur : --juger <xcodebuild.log>" >&2
        exit 2
    fi
    code=0
    juger "$2" "" "$(lecture)" "$(lecture_fin)" porte essai || code=$?
    exit "$code"
    ;;
*)
    echo "argument inconnu : $1 (--court, --verifier-calme, --juger ou rien)" >&2
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

DEPART=$(lecture)
# shellcheck disable=SC2086
set -- $DEPART
if [ "$MODE" = porte ] && ! awk -v s="$4" 'BEGIN { exit !(s == 0) }'; then
    echo "Refusé : swap utilisé ($(dire "$DEPART")) ; il faut aucun swap au départ" >&2
    exit 3
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
echo "Construction Release (build-for-testing), journal $SORTIE/build.log…"
DYSFLOW_TESTS_RELEASE=1 DYSFLOW_TESTS_ETAPE=build-for-testing \
    ./scripts/test-macos.sh MLXLoadTests >"$SORTIE/build.log" 2>&1

# La construction a chargé la machine : la mesure attend le calme.
if [ "$MODE" = porte ]; then
    attente=${DYSFLOW_CHARGE_ATTENTE_S:-900}
    depuis=$(date +%s)
    DEPART=$(lecture)
    # shellcheck disable=SC2086
    set -- $DEPART
    while ! au_calme "$1" "$4"; do
        if [ $(($(date +%s) - depuis)) -ge "$attente" ]; then
            echo "Refusé : pas au calme après la construction ($(dire "$DEPART"), $attente s d'attente)" >&2
            exit 3
        fi
        sleep 15
        DEPART=$(lecture)
        # shellcheck disable=SC2086
        set -- $DEPART
    done
else
    DEPART=$(lecture)
fi

# La durée de la porte, bornée : 30 s par tour, l'alternance et sa marge,
# 10 min pour le chargement et les mesures R15 (par test, en minutes pour
# xcodebuild).
DUREE_MAX_S=$((TOURS * 30 + ALTERNANCE_S * 2 + 600))
echo "Porte ($MODE : $TOURS tours, $ALTERNANCE_S s alternées, $DUREE_MAX_S s au plus), journal $SORTIE/xcodebuild.log…"
code=0
DYSFLOW_TESTS_RELEASE=1 DYSFLOW_TESTS_ETAPE=test-without-building DYSFLOW_TESTS_DUREE_MAX_S=$DUREE_MAX_S \
    TEST_RUNNER_HOME="$MAISON" \
    TEST_RUNNER_DYSFLOW_KOKORO_DIR="$MODELES/kokoro" \
    TEST_RUNNER_DYSFLOW_STT_MODELS="$MODELES" \
    TEST_RUNNER_DYSFLOW_CHARGE_TOURS="$TOURS" \
    TEST_RUNNER_DYSFLOW_CHARGE_ALTERNANCE_S="$ALTERNANCE_S" \
    ./scripts/test-macos.sh MLXLoadTests >"$SORTIE/xcodebuild.log" 2>&1 || code=$?
FIN=$(lecture_fin)

resultat=0
{
    echo "xcodebuild : code $code"
    juger "$SORTIE/xcodebuild.log" "$TOURS" "$DEPART" "$FIN" "$MODE" || resultat=$?
    echo "$resultat" >"$SORTIE/.code"
} | tee "$SORTIE/resume.txt"
resultat=$(cat "$SORTIE/.code")
exit "$resultat"
