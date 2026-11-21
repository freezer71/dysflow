#!/bin/sh
# La porte du lot 1 (spec des moteurs, « Une seule copie d'ONNX Runtime ») :
# compile examples/ort_single_copy.rs avec sherpa-onnx lié à l'ONNX Runtime
# livré (scripts/sherpa-onnx.sh), assemble $OUT comme un paquet (le binaire,
# ONNX Runtime, la bibliothèque C de sherpa-onnx, models/), puis le lance
# depuis /.
#
#   ./scripts/ort-gate.sh
#
# Le binaire cherche ses bibliothèques à côté de lui seulement (build.rs) :
# $OUT sert comme sur un PC qui n'a pas l'arbre de compilation. Sous Windows,
# $OUT reçoit aussi le runtime Visual C++, et ses imports sont vérifiés
# (scripts/windows-imports.py).
#
# $OUT.tar.gz en est l'archive, à lancer sur un autre PC :
#   tar xzf ort-gate.tar.gz && ./ort-gate/ort_single_copy
set -eu
cd "$(dirname "$0")/.."

case "$(uname -s)" in
MINGW* | MSYS* | CYGWIN*) WINDOWS=1 EXE=.exe ;;
*) WINDOWS="" EXE="" ;;
esac
if python3 -c "" >/dev/null 2>&1; then
    PYTHON=python3
elif python -c "import sys; sys.exit(sys.version_info[0] != 3)" >/dev/null 2>&1; then
    PYTHON=python
else
    echo "Erreur : Python 3 introuvable (python3 ou python)" >&2
    exit 1
fi

# shellcheck source=scripts/paquet-commun.sh
. scripts/paquet-commun.sh
# shellcheck source=scripts/sherpa-onnx.sh
. scripts/sherpa-onnx.sh

OUT="${OUT:-target/ort-gate}"
MODELS="${MODELS:-target/ort-gate-models}"

sherpa_fetch
export SHERPA_ONNX_LIB_DIR
# En CI, les étapes suivantes (Clippy) prennent le même dossier.
if [ -n "${GITHUB_ENV:-}" ]; then
    echo "SHERPA_ONNX_LIB_DIR=$SHERPA_ONNX_LIB_DIR" >> "$GITHUB_ENV"
fi
echo "sherpa-onnx $SHERPA_VERSION sur ONNX Runtime $ORT_VERSION : $SHERPA_ONNX_LIB_DIR"
cargo build --release --locked --example ort_single_copy --features stt-sherpa

# Les modèles, empreintes épinglées. Silero VAD : publié par k2-fsa avec
# sherpa-onnx. Kokoro int8 et sa configuration : les fichiers et révisions du
# catalogue (src/models.rs, kokoro_82m_int8).
fetch_verified https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad.onnx \
    9e2449e1087496d8d4caba907f23e0bd3f78d91fa552479bb9c23ac09cbb1fd6 "$MODELS/silero_vad.onnx"
fetch_verified https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX-timestamped/resolve/dd4401a9add81ac692d20e240d22ec9dda82cc29/onnx/model_quantized.onnx \
    c0c02b3299fd97c34ea92a98e6d41eaa1a739c8f77bf685aac34bd7b34c1132c "$MODELS/kokoro/model_quantized.onnx"
fetch_verified https://huggingface.co/mlx-community/Kokoro-82M-bf16/resolve/a71e4d38b236d968966a2002c4c895dbd12b1c3c/config.json \
    5abb01e2403b072bf03d04fde160443e209d7a0dad49a423be15196b9b43c17f "$MODELS/kokoro/config.json"

rm -rf "$OUT" "$OUT.tar.gz"
mkdir -p "$OUT/models/kokoro"
# CARGO_BUILD_TARGET (posé par la CI macOS) range le binaire sous sa cible.
cp "target/${CARGO_BUILD_TARGET:+$CARGO_BUILD_TARGET/}release/examples/ort_single_copy$EXE" "$OUT/"
cp "$SHERPA_ONNX_LIB_DIR/$SHERPA_LIB" "$SHERPA_ONNX_LIB_DIR/$ORT_DEST" "$OUT/"
cp "$MODELS/silero_vad.onnx" "$OUT/models/"
cp "$MODELS/kokoro/model_quantized.onnx" "$MODELS/kokoro/config.json" "$OUT/models/kokoro/"
if [ -n "$WINDOWS" ]; then
    vc_crt_deploy "$OUT"
    "$PYTHON" -I scripts/windows-imports.py check "$OUT"
fi
ls -l "$OUT"
# Ce que le binaire et sherpa-onnx demandent au chargement.
case "$ORT_RID" in
osx-*) otool -L "$OUT/ort_single_copy" "$OUT/$SHERPA_LIB" ;;
linux-*) readelf -d "$OUT/ort_single_copy" "$OUT/$SHERPA_LIB" | grep -E 'Fichier|File|NEEDED|RPATH|RUNPATH' ;;
esac
( cd "$(dirname "$OUT")" && tar czf "$(basename "$OUT").tar.gz" "$(basename "$OUT")" )

# Le lancement : depuis /, sans variable qui désigne un autre ONNX Runtime ;
# sous Windows, PATH réduit à Windows.
gate="$PWD/$OUT/ort_single_copy$EXE"
if [ -n "$WINDOWS" ]; then
    ( cd / && env -u DYSFLOW_ORT_LIB -u DYSFLOW_KOKORO_DIR -u DYSFLOW_SILERO_VAD \
        PATH="/c/Windows/System32:/c/Windows" "$gate" )
else
    ( cd / && env -u DYSFLOW_ORT_LIB -u DYSFLOW_KOKORO_DIR -u DYSFLOW_SILERO_VAD "$gate" )
fi
