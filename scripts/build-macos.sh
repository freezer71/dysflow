#!/usr/bin/env bash
# Construit Sillon.app : moteur, liaison, projet, application.
#
#   ./scripts/build-macos.sh            # dans macos/build/Release/Sillon.app
set -euo pipefail
cd "$(dirname "$0")/.."
./scripts/build-ffi.sh
cd macos
xcodegen generate
xcodebuild -project Sillon.xcodeproj -scheme Sillon -configuration Release \
    -destination 'platform=macOS,arch=arm64' -derivedDataPath build/derived build
APP=$(find build/derived -name Sillon.app -path "*Release*" -maxdepth 6 | head -1)
rm -rf build/Release && mkdir -p build/Release && cp -R "$APP" build/Release/
# L'app doit trouver son shim hors de l'arbre de compilation.
otool -L build/Release/Sillon.app/Contents/MacOS/Sillon | grep -q "@rpath/libaudiocpp_shim" \
    && test -f build/Release/Sillon.app/Contents/Frameworks/libaudiocpp_shim.dylib \
    || echo "Attention : shim absent — app compilée sans audio-cpp ?"
echo "Prête : macos/build/Release/Sillon.app"
