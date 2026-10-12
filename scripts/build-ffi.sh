#!/usr/bin/env bash
# Compile le moteur pour l'app macOS et génère sa liaison Swift.
#
#   ./scripts/build-ffi.sh                       # audio-cpp + ocr
#   FEATURES= ./scripts/build-ffi.sh             # voix du système seule
#
# Produit macos/Generated (Swift, en-tête, module, Link.xcconfig) et
# macos/Libraries (libsillon_ffi.a, et le shim audio.cpp s'il est compilé).
set -euo pipefail
cd "$(dirname "$0")/.."

FEATURES=${FEATURES-audio-cpp,ocr}

# L'app calcule Kokoro en MLX : ONNX Runtime n'y entre jamais.
case ",$FEATURES," in
*kokoro-onnx*)
    echo "Erreur : kokoro-onnx est réservé au terminal, pas à l'app" >&2
    exit 1
    ;;
esac
# Ni par une option du crate ni par une dépendance : `ort` absent du graphe.
if cargo tree -p sillon-ffi ${FEATURES:+--features "$FEATURES"} -e normal -i ort 2>/dev/null | grep -q '^ort '; then
    echo "Erreur : ONNX Runtime (ort) entrerait dans l'app" >&2
    exit 1
fi

GEN=macos/Generated
LIB=macos/Libraries
mkdir -p "$GEN" "$LIB"

# L'app vise macOS 14 : rustc et CMake (crate `cmake`) lisent cette variable.
export MACOSX_DEPLOYMENT_TARGET=14.0

# La liaison d'abord : le binaire bindgen recompile le crate sans les options
# lourdes, et écraserait sinon la bibliothèque finale.
cargo build --release -p sillon-ffi
cargo run --release -p sillon-ffi --features bindgen --bin uniffi-bindgen -- \
    generate --library target/release/libsillon_ffi.dylib --language swift --out-dir "$GEN"
mv "$GEN/sillon_ffiFFI.modulemap" "$GEN/module.modulemap"

BUILD_LOG=$(mktemp)
trap 'rm -f "$BUILD_LOG"' EXIT
cargo build --release -p sillon-ffi ${FEATURES:+--features "$FEATURES"} \
    --message-format=json-render-diagnostics > "$BUILD_LOG"
cp target/release/libsillon_ffi.a "$LIB/"

FLAGS="-lsillon_ffi -lc++ -framework CoreAudio -framework AudioToolbox -framework AudioUnit -framework CoreFoundation -framework Security -framework Metal -framework Accelerate"
rm -f "$LIB/libaudiocpp_shim.dylib"
# Le shim vient de CE build : le out_dir du script de construction du crate
# `sillon`, jamais d'un ancien dossier target/release/build/sillon-*.
case ",$FEATURES," in
*,audio-cpp,*)
    out_dir=$(jq -r 'select(.reason=="build-script-executed" and (.package_id | test("#sillon@"))) | .out_dir' "$BUILD_LOG" | head -1)
    shim="$out_dir/lib/libaudiocpp_shim.dylib"
    if [ -z "$out_dir" ] || [ ! -f "$shim" ]; then
        echo "Erreur : audio-cpp demandé mais libaudiocpp_shim.dylib introuvable (out_dir='$out_dir')" >&2
        exit 1
    fi
    cp "$shim" "$LIB/"
    # Le binaire de l'app le cherchera dans Contents/Frameworks.
    install_name_tool -id @rpath/libaudiocpp_shim.dylib "$LIB/libaudiocpp_shim.dylib"
    codesign -f -s - "$LIB/libaudiocpp_shim.dylib"
    FLAGS="$FLAGS -laudiocpp_shim"
    echo "Shim joint : $shim"
    ;;
esac
echo "OTHER_LDFLAGS = $FLAGS" > "$GEN/Link.xcconfig"
echo "Liaison prête dans $GEN et $LIB"
