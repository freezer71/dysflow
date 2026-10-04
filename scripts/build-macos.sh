#!/usr/bin/env bash
# Construit Sillon.app : moteur, liaison, projet, application.
#
#   ./scripts/build-macos.sh                  # dans macos/build/Release/Sillon.app
#   ./scripts/build-macos.sh --verifier-seulement   # ne refait que le contrôle du shim
set -euo pipefail
cd "$(dirname "$0")/.."

# L'app doit trouver son shim hors de l'arbre de compilation.
verifier_shim() {
    local app="$1" liens
    liens=$(otool -L "$app/Contents/MacOS/Sillon")
    if [[ "$liens" == *"@rpath/libaudiocpp_shim"* ]]; then
        if [ ! -f "$app/Contents/Frameworks/libaudiocpp_shim.dylib" ]; then
            echo "Erreur : l'app référence libaudiocpp_shim mais Contents/Frameworks ne le contient pas." >&2
            return 1
        fi
    else
        echo "Remarque : app construite sans audio-cpp, voix système seule."
    fi
}

if [ "${1:-}" = "--verifier-seulement" ]; then
    verifier_shim macos/build/Release/Sillon.app
    exit
fi

./scripts/build-ffi.sh
cd macos
xcodegen generate
xcodebuild -project Sillon.xcodeproj -scheme Sillon -configuration Release \
    -destination 'platform=macOS,arch=arm64' -derivedDataPath build/derived build
APP=$(find build/derived -name Sillon.app -path "*Release*" -maxdepth 6 | head -1)
rm -rf build/Release && mkdir -p build/Release && cp -R "$APP" build/Release/
cd ..
verifier_shim macos/build/Release/Sillon.app
echo "Prête : macos/build/Release/Sillon.app"
