#!/usr/bin/env bash
# Les tests unitaires de l'app macOS (DysflowTests), sans rien montrer.
#
#   ./scripts/build-ffi.sh                       # d'abord : moteur et liaison
#   ./scripts/test-macos.sh                      # toute la suite
#   ./scripts/test-macos.sh PanelControllerTests # une classe (ou Classe/testNom)
#
# La seule commande pour ces tests, en local comme en CI. Elle ne lance
# jamais les tests d'interface (DysflowUITests) : ils pilotent l'app à
# l'écran. Chaque `-only-testing` commence par DysflowTests/.
#
# L'hôte des tests est Dysflow.app (Debug), au même identifiant que l'app
# installée ; il tourne sans fenêtre visible, sans s'activer, sans icône
# dans la barre des menus ni presse-papiers général (Tests/Headless), et
# un test échoue s'il se montre quand même.
#
#   DYSFLOW_TESTS_RELEASE=1 ./scripts/test-macos.sh MLXLoadTests
#
# En Release : la porte du lot 3 seule (cible DysflowChargeTests, schéma
# DysflowCharge ; scripts/mlx-charge.sh). L'hôte Release est rendu testable
# pour ce passage seulement (ENABLE_TESTABILITY=YES en ligne de commande),
# dans son propre dossier de build (build/derived-charge) : rien de
# build/derived, d'où scripts/build-macos.sh tire l'app livrée, n'est touché.
# En Release seulement :
# - DYSFLOW_TESTS_ETAPE = test (défaut), build-for-testing ou
#   test-without-building : construire d'abord, mesurer ensuite ;
# - DYSFLOW_TESTS_DUREE_MAX_S : la durée d'un test au plus, en secondes
#   (arrondie à la minute par xcodebuild) ; au-delà, il échoue.
set -euo pipefail
cd "$(dirname "$0")/../macos"

if [ ! -f Generated/Link.xcconfig ]; then
    echo "Erreur : macos/Generated absent — ./scripts/build-ffi.sh d'abord" >&2
    exit 2
fi

cible=DysflowTests
build=(-scheme Dysflow -derivedDataPath build/derived)
etape="test"
if [ "${DYSFLOW_TESTS_RELEASE:-}" = 1 ]; then
    cible=DysflowChargeTests
    build=(-scheme DysflowCharge -configuration Release -derivedDataPath build/derived-charge ENABLE_TESTABILITY=YES)
    etape=${DYSFLOW_TESTS_ETAPE:-test}
    case "$etape" in
    test | build-for-testing | test-without-building) ;;
    *)
        echo "Erreur : DYSFLOW_TESTS_ETAPE=$etape (test, build-for-testing ou test-without-building)" >&2
        exit 2
        ;;
    esac
    duree=${DYSFLOW_TESTS_DUREE_MAX_S:-}
    if [ -n "$duree" ] && [ "$etape" != build-for-testing ]; then
        build+=(-test-timeouts-enabled YES -default-test-execution-time-allowance "$duree"
            -maximum-test-execution-time-allowance "$duree")
    fi
    if [ "$#" -eq 0 ]; then
        echo "Erreur : en Release, nommer la classe (MLXLoadTests)" >&2
        exit 2
    fi
    # Les paquets déjà clonés (clone APFS, sans réseau) : le script des
    # licences les lit à côté du dossier de build.
    if [ ! -d build/derived-charge/SourcePackages ] && [ -d build/derived/SourcePackages ]; then
        mkdir -p build/derived-charge
        cp -c -R build/derived/SourcePackages build/derived-charge/
    fi
fi

cibles=()
if [ "$#" -eq 0 ]; then
    cibles=(-only-testing:DysflowTests)
else
    for classe in "$@"; do
        case "$classe" in
        "" | -* | */*/*)
            echo "Erreur : « $classe » n'est pas une classe de $cible (Classe ou Classe/testNom)" >&2
            exit 2
            ;;
        esac
        cibles+=("-only-testing:$cible/$classe")
    done
fi

xcodegen generate --quiet
# exec : le processus de ce script devient xcodebuild (un seul PID à suivre).
exec xcodebuild -project Dysflow.xcodeproj "${build[@]}" \
    -destination 'platform=macOS,arch=arm64' \
    "$etape" "${cibles[@]}"
