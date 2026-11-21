#!/usr/bin/env bash
# Compile le moteur pour l'app macOS et génère sa liaison Swift.
#
#   ./scripts/build-ffi.sh                       # ocr
#   FEATURES= ./scripts/build-ffi.sh             # voix du système seule
#   FEATURES=ocr,mlx ./scripts/build-ffi.sh      # build de test de la CI
#
# mlx (MLX en Rust, mlx_rust_smoke) n'entre pas dans l'app livrée avant le
# lot 3 : seul le build de test de la CI l'active, pour MLXCoexistenceTests.
#
# Produit macos/Generated (Swift, en-tête, module, Link.xcconfig) et
# macos/Libraries (libdysflow_ffi.a).
set -euo pipefail
cd "$(dirname "$0")/.."

FEATURES=${FEATURES-ocr}

# L'app calcule Kokoro en MLX : ONNX Runtime n'y entre jamais.
case ",$FEATURES," in
*kokoro-onnx*)
    echo "Erreur : kokoro-onnx est réservé au terminal, pas à l'app" >&2
    exit 1
    ;;
esac
# Ni par une option du crate ni par une dépendance : `ort` absent du graphe.
# La garde échoue fermée : seule l'erreur exacte « did not match any packages »
# prouve l'absence ; tout autre échec de cargo tree refuse la construction.
if ort_tree=$(cargo tree -p dysflow-ffi ${FEATURES:+--features "$FEATURES"} -e normal -i ort 2>&1); then
    echo "Erreur : ONNX Runtime (ort) entrerait dans l'app :" >&2
    echo "$ort_tree" >&2
    exit 1
elif ! grep -q "package ID specification \`ort\` did not match any packages" <<<"$ort_tree"; then
    echo "Erreur : impossible de vérifier l'absence d'ONNX Runtime (cargo tree a échoué) :" >&2
    echo "$ort_tree" >&2
    exit 1
fi

GEN=macos/Generated
LIB=macos/Libraries
mkdir -p "$GEN" "$LIB"

# L'app vise macOS 14 : rustc et cc lisent cette variable.
export MACOSX_DEPLOYMENT_TARGET=14.0

# Un seul build du moteur : MLX (mlx-sys, CMake) se compile une fois. La
# liaison se lit dans la bibliothèque finale ; l'outil (ffi/bindgen) est un
# paquet à part, qui ne recompile ni le moteur ni MLX.
cargo build --profile app -p dysflow-ffi ${FEATURES:+--features "$FEATURES"}
cp target/app/libdysflow_ffi.a "$LIB/"
cargo run --profile app -p dysflow-uniffi-bindgen -- \
    generate --library target/app/libdysflow_ffi.dylib --language swift --out-dir "$GEN"
mv "$GEN/dysflow_ffiFFI.modulemap" "$GEN/module.modulemap"

FLAGS="-ldysflow_ffi -lc++ -framework CoreAudio -framework AudioToolbox -framework AudioUnit -framework CoreFoundation -framework Security -framework Metal -framework Accelerate"
# DYSFLOW_FFI_CONDITIONS : les conditions Swift des tests qui suivent les
# features du moteur (DYSFLOW_MLX_RUST : mlxRustSmoke existe).
CONDITIONS=
case ",$FEATURES," in
*,mlx,*) CONDITIONS=DYSFLOW_MLX_RUST ;;
esac
{
    echo "OTHER_LDFLAGS = $FLAGS"
    echo "DYSFLOW_FFI_CONDITIONS = $CONDITIONS"
} > "$GEN/Link.xcconfig"
echo "Liaison prête dans $GEN et $LIB"
