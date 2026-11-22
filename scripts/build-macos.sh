#!/usr/bin/env bash
# Construit Dysflow.app : moteur, liaison, projet, application.
#
#   ./scripts/build-macos.sh                  # dans macos/build/Release/Dysflow.app
#   ./scripts/build-macos.sh --mesure         # dans macos/build/Mesure/Dysflow.app
#
# --mesure : la même app Release, avec le code de mesure (`--measure`,
# `--measure-charge`, condition DYSFLOW_MESURE) que l'app livrée n'a pas.
set -euo pipefail
cd "$(dirname "$0")/.."

SORTIE=Release
CONDITIONS=
case "${1:-}" in
--mesure)
    SORTIE=Mesure
    CONDITIONS=DYSFLOW_MESURE
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
# dysflow-espeak (GPL-3.0) : programme séparé, joint à l'app par le projet.
if [ ! -x target/espeak/bin/dysflow-espeak ] || [ scripts/dysflow-espeak.c -nt target/espeak/bin/dysflow-espeak ]; then
    ./scripts/build-espeak.sh
fi
# La copie Rust de MLX (feature mlx du moteur) n'entre pas dans l'app livrée
# avant le lot 3 (spec des moteurs, décision « Lot 0 ») : seul le build de
# test de la CI l'active.
case ",${FEATURES:-}," in
*,mlx,*)
    echo "Erreur : la feature mlx (MLX en Rust) est réservée au build de test, pas à l'app livrée" >&2
    exit 1
    ;;
esac
./scripts/build-ffi.sh
# Et rien de MLX dans le moteur lié : aucun symbole de mlx-c (`_mlx_…`, objets
# natifs de CMake). Le nm d'Apple sort en erreur sur les objets Rust en
# bitcode d'un LLVM plus récent : son code de sortie ne dit rien, mais une
# sortie vide refuse la construction.
symbols=$(nm macos/Libraries/libdysflow_ffi.a 2>/dev/null || true)
if [ -z "$symbols" ]; then
    echo "Erreur : nm ne lit aucun symbole dans macos/Libraries/libdysflow_ffi.a" >&2
    exit 1
fi
mlx_symbols=$(grep -c ' _mlx_' <<<"$symbols" || true)
if [ "$mlx_symbols" != 0 ]; then
    echo "Erreur : $mlx_symbols symboles _mlx_ dans libdysflow_ffi.a (MLX en Rust dans l'app livrée)" >&2
    exit 1
fi
# Les licences des crates Rust liées : THIRD-PARTY-RUST.txt doit suivre
# Cargo.lock (python3 scripts/third-party-rust.py pour le régénérer). Le
# script refuse une licence GPL, AGPL ou inconnue dans le graphe lié.
python3 -I scripts/third-party-rust.py --check
cd macos
xcodegen generate
# DYSFLOW_CONDITIONS n'est lu que par la cible Dysflow : les paquets (MLX)
# ne se recompilent pas d'une sortie à l'autre.
xcodebuild -project Dysflow.xcodeproj -scheme Dysflow -configuration Release \
    -destination 'platform=macOS,arch=arm64' -derivedDataPath build/derived \
    DYSFLOW_CONDITIONS="$CONDITIONS" build
APP=$(find build/derived -name Dysflow.app -path "*Release*" -maxdepth 6 | head -1)
rm -rf "build/$SORTIE" && mkdir -p "build/$SORTIE" && cp -R "$APP" "build/$SORTIE/"
cd ..
APP_BUILT=macos/build/$SORTIE/Dysflow.app
if [ ! -x "$APP_BUILT/Contents/Helpers/dysflow-espeak" ]; then
    echo "Erreur : dysflow-espeak manque dans l'app (Contents/Helpers)" >&2
    exit 1
fi
for f in COPYING SOURCE.txt dysflow-espeak.c build-espeak.sh; do
    if [ ! -f "$APP_BUILT/Contents/Resources/espeak-ng/$f" ]; then
        echo "Erreur : $f manque dans l'app (Contents/Resources/espeak-ng, GPL)" >&2
        exit 1
    fi
done
licences=(NOTICE sonic-LICENSE mlx-audio-swift-LICENSE mlx-swift-lm-LICENSE mlx-swift-LICENSE mlx-LICENSE mlx-c-LICENSE
    metal-cpp-LICENSE nlohmann-json-LICENSE fmt-LICENSE KeyboardShortcuts-LICENSE
    THIRD-PARTY-RUST.txt tesseract-LICENSE leptonica-LICENSE speech-swift-LICENSE cmudict-LICENSE lobe-icons-LICENSE)
# La copie Rust de MLX : ses licences (scripts/mlx-sources.sh) quand le moteur
# lié l'a (DYSFLOW_MLX_RUST dans Link.xcconfig). Refusée plus haut avant le
# lot 3 ; la vérification suit déjà la liaison, pour ne pas l'oublier alors.
if grep -q 'DYSFLOW_MLX_RUST' macos/Generated/Link.xcconfig; then
    for nom in mlx mlx-c metal-cpp nlohmann-json fmt gguflib; do
        licences+=("mlx-rs-$nom-LICENSE")
    done
fi
for f in "${licences[@]}"; do
    if [ ! -s "$APP_BUILT/Contents/Resources/licences/$f" ]; then
        echo "Erreur : $f manque dans l'app (Contents/Resources/licences)" >&2
        exit 1
    fi
done
# dysflow-espeak embarqué, sur ses données embarquées (trames : u32
# petit-boutiste puis octets ; la voix, puis un lot d'une entrée).
phonemes=$(printf '\005\000\000\000fr-fr\001\000\000\000\010\000\000\000Bonjour.' \
    | "$APP_BUILT/Contents/Helpers/dysflow-espeak" "$APP_BUILT/Contents/Resources" | LC_ALL=C tr -d '\000-\037')
case "$phonemes" in
    *"bɔ̃ʒˈuʁ"*) echo "dysflow-espeak embarqué : bɔ̃ʒˈuʁ" ;;
    *) echo "Erreur : dysflow-espeak embarqué ne phonétise pas « Bonjour. »" >&2; exit 1 ;;
esac
if [ -z "$(find "$APP_BUILT" -name '*.metallib' | head -1)" ]; then
    echo "Erreur : aucun metallib dans l'app (noyaux MLX)" >&2
    exit 1
fi
# Signature. En ad hoc, macOS reconnaît l'app à l'empreinte de son binaire :
# chaque construction est une app nouvelle, et l'Accessibilité et le micro
# accordés ne valent plus. Signée d'un certificat, l'app est reconnue par
# son identifiant et ce certificat : ses autorisations restent. Le
# certificat : $DYSFLOW_SIGNATURE, sinon la première ligne de
# .dysflow-signature (hors du dépôt : propre à chaque Mac), sinon ad hoc.
SIGNATURE=${DYSFLOW_SIGNATURE:-}
if [ -z "$SIGNATURE" ] && [ -f .dysflow-signature ]; then
    SIGNATURE=$(head -1 .dysflow-signature)
fi
if [ -n "$SIGNATURE" ]; then
    # Runtime durci, avec les droits de macos/Dysflow.entitlements (le micro) :
    # la notarisation l'exige. L'horodatage aussi, mais il demande le réseau :
    # seulement pour un certificat Developer ID, celui qu'on distribue.
    options=(--force --options runtime --sign "$SIGNATURE")
    case "$SIGNATURE" in "Developer ID"*) options+=(--timestamp) ;; esac
    # Du plus profond au plus haut : l'aide, le framework, puis l'app.
    codesign "${options[@]}" "$APP_BUILT/Contents/Helpers/dysflow-espeak"
    codesign "${options[@]}" "$APP_BUILT/Contents/Frameworks/KokoroMLX.framework"
    codesign "${options[@]}" --entitlements macos/Dysflow.entitlements "$APP_BUILT"
    codesign --verify --deep --strict "$APP_BUILT"
    echo "Signée : $(codesign -dvv "$APP_BUILT" 2>&1 | sed -n 's/^Authority=//p' | head -1)"
else
    echo "Avertissement : signature ad hoc — macOS oubliera l'Accessibilité et le micro à chaque construction (DYSFLOW_SIGNATURE ou .dysflow-signature)" >&2
fi
echo "--- taille de l'app ---"
du -sh "$APP_BUILT"
find "$APP_BUILT" -name '*.metallib' -exec du -h {} +
echo "Prête : $APP_BUILT"
