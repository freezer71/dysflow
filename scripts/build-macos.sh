#!/usr/bin/env bash
# Construit Sillon.app : moteur, liaison, projet, application.
#
#   ./scripts/build-macos.sh                  # dans macos/build/Release/Sillon.app
#   ./scripts/build-macos.sh --mesure         # dans macos/build/Mesure/Sillon.app
#   ./scripts/build-macos.sh --verifier-seulement   # ne refait que le contrôle des liens
#
# --mesure : la même app Release, avec le code de mesure (`--measure`,
# `--measure-charge`, condition SILLON_MESURE) que l'app livrée n'a pas.
set -euo pipefail
cd "$(dirname "$0")/.."

# audio.cpp est sorti du projet (14 oct. 2026) : l'app ne doit plus lier son
# shim, qu'un ancien macos/Libraries pourrait encore fournir.
verifier_sans_shim() {
    if otool -L "$1/Contents/MacOS/Sillon" | grep -q libaudiocpp_shim; then
        echo "Erreur : l'app lie encore libaudiocpp_shim (relancer ./scripts/build-ffi.sh)" >&2
        return 1
    fi
    if [ -e "$1/Contents/Frameworks/libaudiocpp_shim.dylib" ]; then
        echo "Erreur : un ancien libaudiocpp_shim traîne dans Contents/Frameworks" >&2
        return 1
    fi
}

SORTIE=Release
CONDITIONS=
case "${1:-}" in
--verifier-seulement)
    verifier_sans_shim macos/build/Release/Sillon.app
    exit
    ;;
--mesure)
    SORTIE=Mesure
    CONDITIONS=SILLON_MESURE
    ;;
"") ;;
*)
    echo "argument inconnu : $1" >&2
    exit 2
    ;;
esac

# mlx-swift compile ses noyaux Metal : il faut le composant Metal Toolchain.
if ! xcrun -sdk macosx metal -v >/dev/null 2>&1; then
    echo "Erreur : Metal Toolchain absent — xcodebuild -downloadComponent MetalToolchain" >&2
    exit 1
fi
# sillon-espeak (GPL-3.0) : programme séparé, joint à l'app par le projet.
if [ ! -x target/espeak/bin/sillon-espeak ] || [ scripts/sillon-espeak.c -nt target/espeak/bin/sillon-espeak ]; then
    ./scripts/build-espeak.sh
fi
./scripts/build-ffi.sh
# Les licences des crates Rust liées : THIRD-PARTY-RUST.txt doit suivre
# Cargo.lock (python3 scripts/third-party-rust.py pour le régénérer). Le
# script refuse une licence GPL, AGPL ou inconnue dans le graphe lié.
python3 -I scripts/third-party-rust.py --check
cd macos
xcodegen generate
# Une construction antérieure à la sortie d'audio.cpp a laissé son shim dans
# le dossier de produits : xcodebuild ne le retire pas.
find build/derived -path "*Sillon.app/Contents/Frameworks/libaudiocpp_shim.dylib" -delete 2>/dev/null || true
# SILLON_CONDITIONS n'est lu que par la cible Sillon : les paquets (MLX)
# ne se recompilent pas d'une sortie à l'autre.
xcodebuild -project Sillon.xcodeproj -scheme Sillon -configuration Release \
    -destination 'platform=macOS,arch=arm64' -derivedDataPath build/derived \
    SILLON_CONDITIONS="$CONDITIONS" build
APP=$(find build/derived -name Sillon.app -path "*Release*" -maxdepth 6 | head -1)
rm -rf "build/$SORTIE" && mkdir -p "build/$SORTIE" && cp -R "$APP" "build/$SORTIE/"
cd ..
APP_BUILT=macos/build/$SORTIE/Sillon.app
verifier_sans_shim "$APP_BUILT"
if [ ! -x "$APP_BUILT/Contents/Helpers/sillon-espeak" ]; then
    echo "Erreur : sillon-espeak manque dans l'app (Contents/Helpers)" >&2
    exit 1
fi
for f in COPYING SOURCE.txt sillon-espeak.c build-espeak.sh; do
    if [ ! -f "$APP_BUILT/Contents/Resources/espeak-ng/$f" ]; then
        echo "Erreur : $f manque dans l'app (Contents/Resources/espeak-ng, GPL)" >&2
        exit 1
    fi
done
licences=(NOTICE sonic-LICENSE mlx-audio-swift-LICENSE mlx-swift-LICENSE mlx-LICENSE mlx-c-LICENSE
    metal-cpp-LICENSE nlohmann-json-LICENSE fmt-LICENSE KeyboardShortcuts-LICENSE
    THIRD-PARTY-RUST.txt tesseract-LICENSE leptonica-LICENSE)
for f in "${licences[@]}"; do
    if [ ! -s "$APP_BUILT/Contents/Resources/licences/$f" ]; then
        echo "Erreur : $f manque dans l'app (Contents/Resources/licences)" >&2
        exit 1
    fi
done
# sillon-espeak embarqué, sur ses données embarquées (trames : u32
# petit-boutiste puis octets ; la voix, puis un lot d'une entrée).
phonemes=$(printf '\005\000\000\000fr-fr\001\000\000\000\010\000\000\000Bonjour.' \
    | "$APP_BUILT/Contents/Helpers/sillon-espeak" "$APP_BUILT/Contents/Resources" | LC_ALL=C tr -d '\000-\037')
case "$phonemes" in
    *"bɔ̃ʒˈuʁ"*) echo "sillon-espeak embarqué : bɔ̃ʒˈuʁ" ;;
    *) echo "Erreur : sillon-espeak embarqué ne phonétise pas « Bonjour. »" >&2; exit 1 ;;
esac
if [ -z "$(find "$APP_BUILT" -name '*.metallib' | head -1)" ]; then
    echo "Erreur : aucun metallib dans l'app (noyaux MLX)" >&2
    exit 1
fi
echo "--- taille de l'app ---"
du -sh "$APP_BUILT"
find "$APP_BUILT" -name '*.metallib' -exec du -h {} +
echo "Prête : $APP_BUILT"
