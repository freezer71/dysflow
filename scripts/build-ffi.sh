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
GEN=macos/Generated
LIB=macos/Libraries
mkdir -p "$GEN" "$LIB"

# La liaison d'abord : le binaire bindgen recompile le crate sans les options
# lourdes, et écraserait sinon la bibliothèque finale.
cargo build --release -p sillon-ffi
cargo run --release -p sillon-ffi --features bindgen --bin uniffi-bindgen -- \
    generate --library target/release/libsillon_ffi.dylib --language swift --out-dir "$GEN"
mv "$GEN/sillon_ffiFFI.modulemap" "$GEN/module.modulemap"

cargo build --release -p sillon-ffi ${FEATURES:+--features "$FEATURES"}
cp target/release/libsillon_ffi.a "$LIB/"

FLAGS="-lsillon_ffi -lc++ -framework CoreAudio -framework AudioToolbox -framework AudioUnit -framework CoreFoundation -framework Security -framework Metal -framework Accelerate"
rm -f "$LIB/libaudiocpp_shim.dylib"
shim=$(ls -t target/release/build/sillon-*/out/lib/libaudiocpp_shim.dylib 2>/dev/null | head -1 || true)
if [ -n "$shim" ]; then
    cp "$shim" "$LIB/"
    # Le binaire de l'app le cherchera dans Contents/Frameworks.
    install_name_tool -id @rpath/libaudiocpp_shim.dylib "$LIB/libaudiocpp_shim.dylib"
    FLAGS="$FLAGS -laudiocpp_shim"
    echo "Shim joint : $shim"
fi
echo "OTHER_LDFLAGS = $FLAGS" > "$GEN/Link.xcconfig"
echo "Liaison prête dans $GEN et $LIB"
