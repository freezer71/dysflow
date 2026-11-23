#!/bin/sh
# La porte du lot 1 (spec des moteurs, « Une seule copie d'ONNX Runtime ») :
# compile examples/ort_single_copy.rs (et le passe à Clippy) avec la
# bibliothèque C de sherpa-onnx de scripts/sherpa-onnx.sh, assemble $OUT comme
# un paquet (le binaire, l'ONNX Runtime livré, la bibliothèque C de
# sherpa-onnx, models/), puis le lance depuis /. Les tests unitaires de
# stt::sherpa (check_same_runtime) passent d'abord, par le même chemin.
#
#   ./scripts/ort-gate.sh
#
# Le binaire cherche ses bibliothèques à côté de lui seulement (build.rs) :
# $OUT sert comme sur un PC qui n'a pas l'arbre de compilation. Sous Windows,
# $OUT reçoit aussi le runtime Visual C++, et ses imports sont vérifiés
# (scripts/windows-imports.py). Partout, la porte repasse avec ses modèles
# sous un chemin non ASCII (« Élodie Martin », « Łukasz Gaweł » ; ailleurs
# que sous Windows, Élodie aussi en NFD).
#
# Les essais ignorés de stt::sherpa passent aussi, depuis chacun de ces
# chemins : Silero VAD toujours (son modèle est celui de la porte), avec
# l'essai qui vérifie qu'ONNX Runtime n'écrit rien de sa télémétrie, par
# sherpa-onnx (Silero) puis par `ort` (Kokoro, celui de la porte), Parakeet
# quand DYSFLOW_MODELS désigne un dossier de modèles qui contient
# parakeet_tdt_v3_int8/ : les quatre fichiers du paquet et test_wavs/fr.wav,
# tirés du dépôt csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8 à la
# révision ed5d745623472f97cfc2e6c2916ee248798fc5a8
# (https://huggingface.co/<dépôt>/resolve/<révision>/<fichier>), empreintes
# vérifiées plus bas. Un essai sauté (« ignored ») fait échouer la porte.
#
#   DYSFLOW_MODELS=/chemin/des/modeles ./scripts/ort-gate.sh
#
# $OUT.tar.gz en est l'archive, à lancer sur un autre PC :
#   tar xzf ort-gate.tar.gz && ./ort-gate/ort_single_copy
set -eu
# Défense en profondeur : aucun ONNX Runtime lancé d'ici n'écrit ni n'envoie sa télémétrie (onnx::NO_TELEMETRY).
export ORT_DISABLE_TELEMETRY=1
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

# Parakeet int8 : ses fichiers, et leurs empreintes (le paquet
# parakeet_tdt_v3_int8 de src/models.rs, plus l'échantillon du dépôt).
PARAKEET_FILES="encoder.int8.onnx decoder.int8.onnx joiner.int8.onnx tokens.txt test_wavs/fr.wav"
parakeet_sha256() {
    case "$1" in
    encoder.int8.onnx) echo 1afbe69c1ffdabee94755f42e4daf1d93549131a3da11356bda84fcef0f2c479 ;;
    decoder.int8.onnx) echo 0222a86147299d4dee90edf95b6d13827edb42ad8b7c9cf0b2582f030937c0b5 ;;
    joiner.int8.onnx) echo cbd02f3696e9944970111fb439d9c3fd365a80374565f13f2b9c7007d253fc98 ;;
    tokens.txt) echo d58544679ea4bc6ac563d1f545eb7d474bd6cfa467f0a6e2c1dc1c7d37e3c35d ;;
    test_wavs/fr.wav) echo b59be4349b92d344fb903677165eaf4694025d1ab119c608726ecbcb3164b528 ;;
    esac
}
PARAKEET=""
if [ -n "${DYSFLOW_MODELS:-}" ]; then
    PARAKEET="$DYSFLOW_MODELS/parakeet_tdt_v3_int8"
    if [ -n "$WINDOWS" ]; then
        PARAKEET="$(cygpath -u "$DYSFLOW_MODELS")/parakeet_tdt_v3_int8"
    fi
    for file in $PARAKEET_FILES; do
        if ! [ -f "$PARAKEET/$file" ] || ! sha256_ok "$(parakeet_sha256 "$file")" "$PARAKEET/$file"; then
            echo "Erreur : $PARAKEET/$file absent, ou d'une autre empreinte que celle du paquet" >&2
            exit 1
        fi
    done
fi

sherpa_fetch
echo "sherpa-onnx $SHERPA_VERSION ($SHERPA_LIB_DIR), ONNX Runtime $ORT_VERSION ($ORT_DIR)"
cargo clippy -p dysflow --all-targets --features stt-sherpa --locked \
    --config "$SHERPA_CONFIG_SEARCH" --config "$SHERPA_CONFIG_LIB" -- -D warnings
cargo build --release --locked --example ort_single_copy --features stt-sherpa \
    --config "$SHERPA_CONFIG_SEARCH" --config "$SHERPA_CONFIG_LIB"

# Les tests unitaires de stt::sherpa : sans modèle, mais le binaire de test est
# lié à la bibliothèque C de sherpa-onnx. On le compile avec les mêmes --config,
# puis on le lance à côté d'elle, dans $OUT (plus bas).
test_exe=$(cargo test --release --locked --lib --features stt-sherpa --no-run --message-format=json \
    --config "$SHERPA_CONFIG_SEARCH" --config "$SHERPA_CONFIG_LIB" \
    | "$PYTHON" -c 'import json,sys
for l in sys.stdin:
    try: m=json.loads(l)
    except ValueError: continue
    if m.get("reason")=="compiler-artifact" and m.get("executable") and m["target"]["kind"]==["lib"] and m["profile"]["test"]:
        print(m["executable"])' | tail -n 1)
[ -n "$test_exe" ] || { echo "Erreur : binaire des tests de la bibliothèque introuvable" >&2; exit 1; }

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
cp "$SHERPA_LIB_DIR/$SHERPA_LIB" "$OUT/"
cp "$ORT_DIR/$ORT_LIB" "$OUT/$ORT_DEST"
cp "$MODELS/silero_vad.onnx" "$OUT/models/"
cp "$MODELS/kokoro/model_quantized.onnx" "$MODELS/kokoro/config.json" "$OUT/models/kokoro/"
if [ -n "$WINDOWS" ]; then
    vc_crt_deploy "$OUT"
    "$PYTHON" -I scripts/windows-imports.py check "$OUT"
fi
cp "$test_exe" "$OUT/dysflow_lib_tests$EXE"
ls -l "$OUT"
# Ce que le binaire et sherpa-onnx demandent au chargement.
case "$ORT_RID" in
osx-*) otool -L "$OUT/ort_single_copy" "$OUT/$SHERPA_LIB" ;;
linux-*) readelf -d "$OUT/ort_single_copy" "$OUT/$SHERPA_LIB" | grep -E 'Fichier|File|NEEDED|RPATH|RUNPATH' ;;
esac
( cd "$(dirname "$OUT")" && tar czf "$(basename "$OUT").tar.gz" "$(basename "$OUT")" )

# Le lancement : depuis /, sans variable qui désigne un autre ONNX Runtime ;
# sous Windows, PATH réduit à Windows. D'abord les tests unitaires de
# stt::sherpa (check_same_runtime), puis la porte.
gate="$PWD/$OUT/ort_single_copy$EXE"
unit="$PWD/$OUT/dysflow_lib_tests$EXE"

# ignored_tests FILTRE NOMBRE VARIABLE=VALEUR… : les essais ignorés de FILTRE
# (sans `--` : le binaire est celui des tests), depuis /, comme la porte. Il
# en faut exactement NOMBRE, tous passés : un essai sauté échoue.
ignored_tests() {
    filter=$1 count=$2
    shift 2
    log="$PWD/$OUT/ignored.log"
    if [ -n "$WINDOWS" ]; then
        set -- PATH="/c/Windows/System32:/c/Windows" "$@"
    fi
    if ! ( cd / && env -u DYSFLOW_ORT_LIB "$@" "$unit" "$filter" --ignored ) >"$log" 2>&1; then
        cat "$log"
        echo "Erreur : essais ignorés de $filter en échec" >&2
        exit 1
    fi
    cat "$log"
    if grep -q '\.\.\. ignored' "$log" || ! grep -q "test result: ok\. $count passed; 0 failed; 0 ignored" "$log"; then
        echo "Erreur : $filter : $count essai(s) attendu(s), tous passés" >&2
        exit 1
    fi
}

# dictation_tests DOSSIER_DES_MODÈLES : Silero VAD depuis DOSSIER, la
# télémétrie d'ONNX Runtime (Silero et DOSSIER/kokoro), puis
# Parakeet (si DYSFLOW_MODELS est posé), ses fichiers liés ou copiés dans
# DOSSIER/parakeet_tdt_v3_int8/ quand DOSSIER n'est pas DYSFLOW_MODELS.
dictation_tests() {
    models=$1
    native=$models
    if [ -n "$WINDOWS" ]; then
        native=$(cygpath -w "$models")
    fi
    ignored_tests stt::sherpa::vad 2 DYSFLOW_SILERO_VAD="$native/silero_vad.onnx"
    ignored_tests onnx::tests::kokoro_leaves_no_telemetry_trace 1 DYSFLOW_KOKORO_DIR="$native/kokoro"
    if [ -z "$PARAKEET" ]; then
        echo "Parakeet : DYSFLOW_MODELS non posé, l'échantillon n'est pas lu depuis $models"
        return 0
    fi
    copy=""
    if [ "$models/parakeet_tdt_v3_int8" != "$PARAKEET" ]; then
        copy="$models/parakeet_tdt_v3_int8"
        mkdir -p "$copy/test_wavs"
        for file in $PARAKEET_FILES; do
            ln "$PARAKEET/$file" "$copy/$file" 2>/dev/null || cp "$PARAKEET/$file" "$copy/$file"
        done
    fi
    ignored_tests stt::sherpa::parakeet 2 DYSFLOW_MODELS="$native"
    # La dictée d'un fichier (--transcribe) : engine_for, Dictation, le vrai
    # modèle sur test_wavs/fr.wav ramené à 16 kHz.
    ignored_tests stt::file 1 DYSFLOW_MODELS="$native"
    if [ -n "$copy" ]; then
        rm -rf "$copy"
    fi
}
if [ -n "$WINDOWS" ]; then
    ( cd / && env -u DYSFLOW_ORT_LIB -u DYSFLOW_KOKORO_DIR -u DYSFLOW_SILERO_VAD \
        PATH="/c/Windows/System32:/c/Windows" "$unit" stt::sherpa )
    ( cd / && env -u DYSFLOW_ORT_LIB -u DYSFLOW_KOKORO_DIR -u DYSFLOW_SILERO_VAD \
        PATH="/c/Windows/System32:/c/Windows" "$gate" )
    dictation_tests "$PWD/$OUT/models"
    # Les modèles d'un vrai PC sont sous le profil de l'utilisateur
    # (C:\Users\Élodie Martin\AppData\Local\dysflow\models\…) : la porte une
    # seconde fois, ses modèles copiés sous un chemin qui porte une espace et
    # un accent, puis sous un autre hors de la page de code Windows-1252
    # (« Ł »). Kokoro les ouvre par `ort`, Silero VAD par la bibliothèque C de
    # sherpa-onnx, qui reçoit le chemin en UTF-8. DYSFLOW_KOKORO_DIR et
    # DYSFLOW_SILERO_VAD sont ce que lit la porte (examples/ort_single_copy.rs),
    # passés en chemins Windows.
    base=$(cygpath -u "${RUNNER_TEMP:-$PWD/target}")
    for name in "Élodie Martin" "Łukasz Gaweł"; do
        models="$base/$name/AppData/Local/dysflow/models"
        rm -rf "$models"
        mkdir -p "$models/kokoro"
        cp "$MODELS/silero_vad.onnx" "$models/"
        cp "$MODELS/kokoro/model_quantized.onnx" "$MODELS/kokoro/config.json" "$models/kokoro/"
        echo "Modèles depuis $(cygpath -w "$models") :"
        ( cd / && env -u DYSFLOW_ORT_LIB \
            DYSFLOW_KOKORO_DIR="$(cygpath -w "$models/kokoro")" \
            DYSFLOW_SILERO_VAD="$(cygpath -w "$models/silero_vad.onnx")" \
            PATH="/c/Windows/System32:/c/Windows" "$gate" )
        dictation_tests "$models"
    done
else
    ( cd / && env -u DYSFLOW_ORT_LIB -u DYSFLOW_KOKORO_DIR -u DYSFLOW_SILERO_VAD "$unit" stt::sherpa )
    ( cd / && env -u DYSFLOW_ORT_LIB -u DYSFLOW_KOKORO_DIR -u DYSFLOW_SILERO_VAD "$gate" )
    dictation_tests "$PWD/$OUT/models"
    # Les mêmes noms d'utilisateur que sous Windows, dans un dossier de
    # modèles : « Élodie Martin » composé (NFC) puis décomposé (NFD, ce que
    # rendent d'anciens volumes HFS+ et des outils qui normalisent), et
    # « Łukasz Gaweł » (un Ł n'a pas de forme décomposée). Silero VAD reçoit
    # le chemin en UTF-8, octet pour octet.
    nfc=$(printf 'Élodie Martin')
    nfd=$(printf 'E\314\201lodie Martin')
    # Le dossier précédent part d'abord : sous APFS, qui compare les noms
    # sans tenir compte de la normalisation, le NFD retrouverait sinon le
    # dossier NFC.
    for name in "$nfc" "$nfd" "Łukasz Gaweł"; do
        rm -rf target/ort-gate-paths
        models="$PWD/target/ort-gate-paths/$name/dysflow/models"
        mkdir -p "$models/kokoro"
        cp "$MODELS/silero_vad.onnx" "$models/"
        cp "$MODELS/kokoro/model_quantized.onnx" "$MODELS/kokoro/config.json" "$models/kokoro/"
        echo "Modèles depuis $models :"
        ( cd / && env -u DYSFLOW_ORT_LIB \
            DYSFLOW_KOKORO_DIR="$models/kokoro" DYSFLOW_SILERO_VAD="$models/silero_vad.onnx" "$gate" )
        dictation_tests "$models"
    done
    rm -rf target/ort-gate-paths
fi
