#!/usr/bin/env bash
# Le banc Swift de Kokoro (P5 du port vers mlx-rs), pendant de
# examples/voice_bench.rs : construit KokoroMLX.framework en Release dans un
# dossier à part, y lie scripts/voice-bench-swift.swift, et le lance une fois.
# Sans fenêtre ni son : jamais Dysflow.app.
#
#   DYSFLOW_MODELS=… ./scripts/voice-bench-swift.sh <dossier de travail>
#
# Le dossier de travail reçoit tout (produits, objets, paquets Swift copiés
# depuis macos/build/derived/SourcePackages, sans réseau) : rien n'est écrit
# dans les dossiers de build partagés. Machine au calme pour une vraie mesure
# (charge < 2, pas de swap).
set -euo pipefail
cd "$(dirname "$0")/.."

if [ "$#" -ne 1 ] || [ -z "${DYSFLOW_MODELS:-}" ]; then
    echo "usage : DYSFLOW_MODELS=… $0 <dossier de travail>" >&2
    exit 2
fi
work=$(mkdir -p "$1" && cd "$1" && pwd)
packages=macos/build/derived/SourcePackages
if [ ! -d "$work/packages" ]; then
    if [ ! -d "$packages/checkouts/mlx-swift" ]; then
        echo "Erreur : $packages absent — ./scripts/test-macos.sh une fois d'abord" >&2
        exit 2
    fi
    # Copie APFS (clone) : les paquets partagés ne sont jamais modifiés.
    cp -cR "$packages" "$work/packages"
fi

(cd macos && xcodegen generate --quiet)
xcodebuild -project macos/Dysflow.xcodeproj -scheme KokoroMLX -configuration Release \
    -destination 'platform=macOS,arch=arm64' -derivedDataPath "$work/derived" \
    -clonedSourcePackagesDirPath "$work/packages" -disableAutomaticPackageResolution \
    -quiet build
products=$work/derived/Build/Products/Release

# Les modules C sous MLX, que `import MLX` demande : Cmlx et _NumericsShims
# par leurs dossiers d'en-têtes, les autres (cLogSupport…) par les cartes
# qu'Xcode a écrites.
maps=(-I "$work/packages/checkouts/swift-numerics/Sources/_NumericsShims/include"
    -I "$work/packages/checkouts/mlx-swift/Source/Cmlx/include")
for map in "$work"/derived/Build/Intermediates.noindex/GeneratedModuleMaps/*.modulemap; do
    maps+=(-Xcc "-fmodule-map-file=$map")
done
swiftc -O -F "$products" -I "$products" -framework KokoroMLX "${maps[@]}" \
    -Xlinker -rpath -Xlinker "$products" \
    scripts/voice-bench-swift.swift -o "$work/voice-bench-swift"
# Les noyaux Metal de mlx-swift se cherchent à côté de l'exécutable.
rm -rf "$work/mlx-swift_Cmlx.bundle"
cp -cR "$products/mlx-swift_Cmlx.bundle" "$work/"

"$work/voice-bench-swift" macos/Tests/Fixtures/kokoro-parite.json "$DYSFLOW_MODELS/kokoro"
