#!/usr/bin/env bash
# Compile le moteur pour l'app macOS et génère sa liaison Swift.
#
#   ./scripts/build-ffi.sh                       # ocr
#   FEATURES= ./scripts/build-ffi.sh             # voix du système seule
#
# Produit macos/Generated (Swift, en-tête, module, Link.xcconfig) et
# macos/Libraries (libsillon_ffi.a).
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
if ort_tree=$(cargo tree -p sillon-ffi ${FEATURES:+--features "$FEATURES"} -e normal -i ort 2>&1); then
    echo "Erreur : ONNX Runtime (ort) entrerait dans l'app :" >&2
    echo "$ort_tree" >&2
    exit 1
elif ! grep -q 'package ID specification `ort` did not match any packages' <<<"$ort_tree"; then
    echo "Erreur : impossible de vérifier l'absence d'ONNX Runtime (cargo tree a échoué) :" >&2
    echo "$ort_tree" >&2
    exit 1
fi

GEN=macos/Generated
LIB=macos/Libraries
mkdir -p "$GEN" "$LIB"

# L'app vise macOS 14 : rustc et cc lisent cette variable.
export MACOSX_DEPLOYMENT_TARGET=14.0

# La liaison d'abord : le binaire bindgen recompile le crate sans les options
# lourdes, et écraserait sinon la bibliothèque finale.
cargo build --profile app -p sillon-ffi
cargo run --profile app -p sillon-ffi --features bindgen --bin uniffi-bindgen -- \
    generate --library target/app/libsillon_ffi.dylib --language swift --out-dir "$GEN"
mv "$GEN/sillon_ffiFFI.modulemap" "$GEN/module.modulemap"

cargo build --profile app -p sillon-ffi ${FEATURES:+--features "$FEATURES"}
cp target/app/libsillon_ffi.a "$LIB/"

FLAGS="-lsillon_ffi -lc++ -framework CoreAudio -framework AudioToolbox -framework AudioUnit -framework CoreFoundation -framework Security -framework Metal -framework Accelerate"
# Un ancien shim audio.cpp (retiré le 14 oct. 2026) ne doit plus être joint.
rm -f "$LIB/libaudiocpp_shim.dylib"
echo "OTHER_LDFLAGS = $FLAGS" > "$GEN/Link.xcconfig"
echo "Liaison prête dans $GEN et $LIB"
