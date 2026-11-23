#!/bin/sh
# Assemble un dossier distribuable : le binaire et les bibliothèques et
# programmes qu'il charge (pour Kokoro, ONNX Runtime et dysflow-espeak avec ses
# données et ses sources GPL ; pour la dictée, la bibliothèque C de sherpa-onnx
# et Silero VAD ; sur Mac, les noyaux de MLX).
#
#   ./scripts/package.sh                          # features de default_features
#   FEATURES=ocr,kokoro-onnx ./scripts/package.sh # sans la dictée
#
# Les features par défaut (default_features, scripts/paquet-commun.sh) :
# ocr,kokoro-onnx,stt-sherpa, plus mlx sur un Mac Apple Silicon.
#
# Sous Windows (bash de Git for Windows ; dysflow.exe par MSVC,
# dysflow-espeak.exe par MinGW-w64, voir build-espeak.sh) : dysflow.exe,
# dysflow-espeak.exe, onnxruntime.dll et sherpa-onnx-c-api.dll, et en plus du
# dossier une archive $OUT/dysflow-windows-x86_64.zip de son contenu, à
# décompresser n'importe où.
set -eu
# Défense en profondeur : aucun ONNX Runtime lancé d'ici (le binaire du
# paquet : --version, --show-config, --check-kokoro) n'écrit ni n'envoie sa
# télémétrie (onnx::NO_TELEMETRY).
export ORT_DISABLE_TELEMETRY=1
cd "$(dirname "$0")/.."

case "$(uname -s)" in
MINGW* | MSYS* | CYGWIN*) WINDOWS=1 EXE=.exe ;;
*) WINDOWS="" EXE="" ;;
esac

# Python 3 : sous Windows, la commande s'appelle souvent python, et python3
# peut être le raccourci du Microsoft Store, qui ne lance rien.
if python3 -c "" >/dev/null 2>&1; then
    PYTHON=python3
elif python -c "import sys; sys.exit(sys.version_info[0] != 3)" >/dev/null 2>&1; then
    PYTHON=python
else
    echo "Erreur : Python 3 introuvable (python3 ou python)" >&2
    exit 1
fi

# default_features, ort_fetch (ONNX Runtime du paquet NuGet), silero_fetch et
# vc_crt_deploy (Windows).
# shellcheck source=scripts/paquet-commun.sh
. scripts/paquet-commun.sh

FEATURES="${FEATURES:-$(default_features)}"
OUT="${OUT:-dist}"

# Les licences des crates Rust liées : THIRD-PARTY-RUST.txt doit suivre
# Cargo.lock (python3 scripts/third-party-rust.py pour le régénérer) ; le
# script refuse une licence GPL, AGPL ou inconnue dans le graphe lié. Il
# décrit les features par défaut ci-dessous : avec d'autres, le régénérer.
"$PYTHON" -I scripts/third-party-rust.py --check

# La dictée : la bibliothèque C de sherpa-onnx (scripts/sherpa-onnx.sh), et
# les --config qui remplacent le script de build de sherpa-onnx-sys (sans
# eux, il téléchargerait son propre ONNX Runtime 1.28.2).
set --
case ",$FEATURES," in
*,stt-sherpa,*)
    # shellcheck source=scripts/sherpa-onnx.sh
    . scripts/sherpa-onnx.sh
    sherpa_fetch
    set -- --config "$SHERPA_CONFIG_SEARCH" --config "$SHERPA_CONFIG_LIB"
    ;;
esac
# MLX en Rust (Mac) : ses sources épinglées, et l'emplacement de ses noyaux
# (MLX_RS_METAL_PATH), avant cargo.
case ",$FEATURES," in
*,mlx,*)
    # shellcheck source=scripts/mlx-sources.sh
    . scripts/mlx-sources.sh
    ;;
esac

echo "Compilation (features : $FEATURES)…"
# DYSFLOW_PACKAGE_BUILD : le binaire se sait livré en paquet, et refuse de
# dicter si Silero VAD manque à côté de lui (stt::engine_for).
DYSFLOW_PACKAGE_BUILD=1 cargo build --release --locked --features "$FEATURES" "$@"

rm -rf "$OUT"
mkdir -p "$OUT"
# CARGO_BUILD_TARGET (posé par la CI macOS) range le binaire sous sa cible.
cp "target/${CARGO_BUILD_TARGET:+$CARGO_BUILD_TARGET/}release/dysflow$EXE" "$OUT/"

# Kokoro au terminal : ONNX Runtime 1.31 à côté du binaire, et la
# phonétisation dysflow-espeak (GPL-3.0, programme séparé que Dysflow lance) avec
# ses données, sa licence et ses sources correspondantes.
case ",$FEATURES," in
*,kokoro-onnx,*)
    espeak="target/espeak/bin/dysflow-espeak$EXE"
    if [ ! -x "$espeak" ] || [ -n "$(find scripts/dysflow-espeak.c scripts/build-espeak.sh -newer "$espeak")" ]; then
        ./scripts/build-espeak.sh
    fi
    cp "$espeak" "$OUT/"
    cp -R target/espeak/share/espeak-ng-data "$OUT/"
    mkdir -p "$OUT/dysflow-espeak-source"
    cp target/espeak/COPYING target/espeak/SOURCE.txt target/espeak/dysflow-espeak.c \
        scripts/build-espeak.sh "$OUT/dysflow-espeak-source/"
    echo "Joint : dysflow-espeak, espeak-ng-data et ses sources (dysflow-espeak-source/)"

    # ONNX Runtime 1.31 à côté du binaire (scripts/paquet-commun.sh).
    ort_fetch
    if [ -n "$ORT_RID" ]; then
        cp "$ORT_DIR/$ORT_LIB" "$OUT/$ORT_DEST"
        cp "$ORT_DIR/LICENSE" "$OUT/LICENSE-onnxruntime"
        cp "$ORT_DIR/ThirdPartyNotices.txt" "$OUT/ThirdPartyNotices-onnxruntime.txt"
        echo "Joint : ONNX Runtime $ORT_VERSION"
    else
        echo "ONNX Runtime non joint sur cette plateforme : DYSFLOW_ORT_LIB=<libonnxruntime> au lancement." >&2
    fi
    ;;
esac

# Les avis : NOTICE, les crates Rust, Sonic, et Tesseract et Leptonica
# (feature ocr). ONNX
# Runtime (LICENSE-onnxruntime, ThirdPartyNotices-onnxruntime.txt) et
# dysflow-espeak (dysflow-espeak-source/) ont été joints plus haut.
cp NOTICE THIRD-PARTY-RUST.txt "$OUT/"
mkdir -p "$OUT/licences"
cp third_party/sonic/LICENSE "$OUT/licences/sonic-LICENSE"
case ",$FEATURES," in
*,ocr,*)
    cp third_party/licences/tesseract-LICENSE third_party/licences/leptonica-LICENSE "$OUT/licences/" ;;
esac
echo "Joint : NOTICE, THIRD-PARTY-RUST.txt et licences/"

# La dictée : la bibliothèque C de sherpa-onnx (Apache-2.0), qui charge
# l'ONNX Runtime joint plus haut, et Silero VAD (MIT), le détecteur de parole,
# à côté du binaire, chacun avec sa licence. Le binaire refuse de dicter sans
# Silero (installation abîmée) ; sans la bibliothèque C, il ne démarre pas du
# tout (README, LISEZMOI.txt : réinstaller le paquet entier).
case ",$FEATURES," in
*,stt-sherpa,*)
    cp "$SHERPA_LIB_DIR/$SHERPA_LIB" "$OUT/"
    silero_fetch target/paquet/silero_vad.onnx
    cp target/paquet/silero_vad.onnx "$OUT/"
    cp third_party/licences/sherpa-onnx-LICENSE third_party/licences/silero-vad-LICENSE "$OUT/licences/"
    echo "Joint : sherpa-onnx $SHERPA_VERSION ($SHERPA_LIB) et Silero VAD (silero_vad.onnx)"
    ;;
esac

# MLX en Rust (Mac) : ses noyaux Metal (mlx.metallib, vérifiés : entiers et
# pour ce macOS), que compute::mlx cherche à côté du binaire, et les licences
# de ses sources (scripts/mlx-sources.sh).
case ",$FEATURES," in
*,mlx,*)
    if [ -n "${MLX_RS_METAL_PATH:-}" ]; then
        if ! mlx_noyaux_justes "$MLX_RS_METAL_PATH"; then
            echo "Erreur : $MLX_RS_METAL_PATH/mlx.metallib absent, tronqué ou pour un autre macOS" >&2
            exit 1
        fi
        cp "$MLX_RS_METAL_PATH/mlx.metallib" "$OUT/"
        cp target/mlx-sources/licences/mlx-rs-* "$OUT/licences/"
        echo "Joint : mlx.metallib et les licences de MLX (licences/mlx-rs-*)"
    fi
    ;;
esac

# Windows : dysflow-espeak.exe est lié statiquement au runtime de MinGW-w64
# (démarrage du CRT, libmingwex, winpthreads) et à libgcc : leurs avis,
# tirés de l'installation MSYS2 qui l'a compilé (MINGW_BIN, comme
# build-espeak.sh). Celui du runtime de MinGW-w64 est exigé ; ceux de
# winpthreads et l'exception de libgcc sont joints quand MSYS2 les porte.
if [ -n "$WINDOWS" ]; then
    mingw=$(cygpath -u "${MINGW_BIN:-C:\msys64\ucrt64\bin}")/../share/licenses
    runtime=$(find "$mingw" -type f -iname 'COPYING.MinGW-w64-runtime*' 2>/dev/null | head -1)
    if [ -z "$runtime" ]; then
        echo "Erreur : l'avis du runtime de MinGW-w64 (COPYING.MinGW-w64-runtime.txt) introuvable sous $mingw" >&2
        ls "$mingw" >&2 || true
        exit 1
    fi
    cp "$runtime" "$OUT/licences/mingw-w64-runtime-COPYING.txt"
    winpthreads=$(find "$mingw" -type f -ipath '*winpthread*' -iname 'COPYING*' 2>/dev/null | head -1)
    if [ -n "$winpthreads" ]; then cp "$winpthreads" "$OUT/licences/mingw-w64-winpthreads-COPYING.txt"; fi
    exception=$(find "$mingw" -type f -iname 'RUNTIME.LIBRARY.EXCEPTION*' 2>/dev/null | head -1)
    if [ -n "$exception" ]; then cp "$exception" "$OUT/licences/gcc-RUNTIME.LIBRARY.EXCEPTION.txt"; fi
    echo "Joint : avis de MinGW-w64 pour dysflow-espeak.exe (runtime${winpthreads:+, winpthreads}${exception:+, exception de libgcc})"
fi

# Windows : le runtime Visual C++ qu'importent dysflow.exe et onnxruntime.dll,
# joint à côté d'eux (vc_crt_deploy, scripts/paquet-commun.sh).
if [ -n "$WINDOWS" ]; then
    vc_crt_deploy "$OUT"

    # Un mot d'accueil, en français puis en anglais.
    cat > "$OUT/LISEZMOI.txt" <<'LISEZMOI'
Dysflow pour Windows — lecteur et dictée au terminal
====================================================

Contenu
  dysflow.exe            le lecteur (lancez-le depuis PowerShell ou cmd)
  dysflow-espeak.exe     la phonétisation de la voix Kokoro (espeak-ng, GPL-3.0),
  espeak-ng-data\        ses données ; sources dans dysflow-espeak-source\
  onnxruntime.dll        ONNX Runtime de Microsoft, le calcul de la voix Kokoro
                         et de la dictée
  sherpa-onnx-c-api.dll  sherpa-onnx (k2-fsa, Apache-2.0), la dictée
  silero_vad.onnx        Silero VAD (MIT), qui entend où la parole commence et finit
  vcruntime140*.dll, msvcp140*.dll
                         le runtime Visual C++ de Microsoft, joint pour les PC
                         qui ne l'ont pas
  NOTICE, THIRD-PARTY-RUST.txt, licences\, LICENSE-onnxruntime…  les licences

Utilisation
  Gardez tous ces fichiers ensemble, dans n'importe quel dossier. Puis :
    .\dysflow.exe --check-kokoro        vérifie ONNX Runtime et la phonétisation
    .\dysflow.exe fichier.txt           lit le fichier à voix haute
    .\dysflow.exe -o lecture.wav fichier.txt   l'écrit en WAV
  Au premier lancement du lecteur, Dysflow télécharge la voix Kokoro (une fois).

  Dicter un fichier WAV (16 kHz, mono) :
    .\dysflow.exe --install-model parakeet_tdt_v3_int8   une fois : le modèle
    .\dysflow.exe --transcribe note.wav > note.txt       le texte seul

Installation abîmée
  Si dysflow.exe ne démarre pas (« sherpa-onnx-c-api.dll » ou une autre DLL
  introuvable), ou s'il dit qu'un fichier du paquet manque (silero_vad.onnx…),
  ne copiez pas un fichier seul : réinstallez le paquet entier. Décompressez
  de nouveau toute l'archive dans un dossier vide. Vos modèles et vos réglages
  ne sont pas dans ce dossier : ils restent.

-----------------------------------------------------------------------------

Dysflow for Windows — terminal reader and dictation

Contents
  dysflow.exe            the reader (run it from PowerShell or cmd)
  dysflow-espeak.exe     phonemization for the Kokoro voice (espeak-ng, GPL-3.0),
  espeak-ng-data\        its data; sources in dysflow-espeak-source\
  onnxruntime.dll        Microsoft ONNX Runtime, which computes the Kokoro voice
                         and dictation
  sherpa-onnx-c-api.dll  sherpa-onnx (k2-fsa, Apache-2.0), dictation
  silero_vad.onnx        Silero VAD (MIT), which hears where speech starts and ends
  vcruntime140*.dll, msvcp140*.dll
                         Microsoft's Visual C++ runtime, included for PCs that
                         lack it
  NOTICE, THIRD-PARTY-RUST.txt, licences\, LICENSE-onnxruntime…  the licenses

Usage
  Keep all these files together, in any folder. Then:
    .\dysflow.exe --check-kokoro        checks ONNX Runtime and phonemization
    .\dysflow.exe file.txt              reads the file aloud
    .\dysflow.exe -o reading.wav file.txt      writes it as WAV
  On the reader's first launch, Dysflow downloads the Kokoro voice (once).

  Dictating a WAV file (16 kHz, mono):
    .\dysflow.exe --install-model parakeet_tdt_v3_int8   once: the model
    .\dysflow.exe --transcribe note.wav > note.txt       the text only

Damaged installation
  If dysflow.exe does not start ("sherpa-onnx-c-api.dll" or another DLL not
  found), or says that a file of the package is missing (silero_vad.onnx…),
  do not copy a single file: reinstall the whole package. Unzip the whole
  archive again into an empty folder. Your models and settings are not in
  this folder: they stay.
LISEZMOI
fi

echo "Vérification hors de l'arbre de compilation…"
( cd "$OUT" && "./dysflow$EXE" --show-config >/dev/null )

# Windows : l'archive à distribuer, faite par le zipfile de Python (les
# entrées en « / », lisibles par l'Explorateur comme par unzip), à côté puis
# mise en place d'un coup. Les fichiers sont à sa racine.
if [ -n "$WINDOWS" ]; then
    zip="dysflow-windows-x86_64.zip"
    rm -f "$OUT/$zip" "$OUT.zip.part"
    "$PYTHON" -I -c '
import pathlib, sys, zipfile
root = pathlib.Path(sys.argv[1])
with zipfile.ZipFile(sys.argv[2], "w", zipfile.ZIP_DEFLATED) as archive:
    for path in sorted(root.rglob("*")):
        if path.is_file():
            archive.write(path, path.relative_to(root).as_posix())
' "$OUT" "$OUT.zip.part"
    mv "$OUT.zip.part" "$OUT/$zip"
    echo "Archive : $OUT/$zip"
fi
echo "Prêt : $OUT/"
ls -lh "$OUT"
