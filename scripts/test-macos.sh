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
set -euo pipefail
cd "$(dirname "$0")/../macos"

if [ ! -f Generated/Link.xcconfig ]; then
    echo "Erreur : macos/Generated absent — ./scripts/build-ffi.sh d'abord" >&2
    exit 2
fi

cibles=()
if [ "$#" -eq 0 ]; then
    cibles=(-only-testing:DysflowTests)
else
    for classe in "$@"; do
        case "$classe" in
        "" | -* | */*/*)
            echo "Erreur : « $classe » n'est pas une classe de DysflowTests (Classe ou Classe/testNom)" >&2
            exit 2
            ;;
        esac
        cibles+=("-only-testing:DysflowTests/$classe")
    done
fi

xcodegen generate --quiet
# exec : le processus de ce script devient xcodebuild (un seul PID à suivre).
exec xcodebuild -project Dysflow.xcodeproj -scheme Dysflow \
    -destination 'platform=macOS,arch=arm64' -derivedDataPath build/derived \
    test "${cibles[@]}"
