#!/bin/sh
# Assemble un dossier distribuable : le binaire et les bibliothèques et
# programmes qu'il charge (pour Kokoro, ONNX Runtime et dysflow-espeak avec ses
# données et ses sources GPL).
#
# Sous Windows (bash de Git for Windows ; dysflow.exe par MSVC,
# dysflow-espeak.exe par MinGW-w64, voir build-espeak.sh) : dysflow.exe,
# dysflow-espeak.exe et onnxruntime.dll, et en plus du dossier une archive
# $OUT/dysflow-windows-x86_64.zip de son contenu, à décompresser n'importe où.
set -eu
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

# ort_fetch (ONNX Runtime du paquet NuGet) et vc_crt_deploy (Windows).
# shellcheck source=scripts/paquet-commun.sh
. scripts/paquet-commun.sh

FEATURES="${FEATURES:-ocr,kokoro-onnx}"
OUT="${OUT:-dist}"

# Les licences des crates Rust liées : THIRD-PARTY-RUST.txt doit suivre
# Cargo.lock (python3 scripts/third-party-rust.py pour le régénérer) ; le
# script refuse une licence GPL, AGPL ou inconnue dans le graphe lié. Il
# décrit les features par défaut ci-dessous : avec d'autres, le régénérer.
"$PYTHON" -I scripts/third-party-rust.py --check

echo "Compilation (features : $FEATURES)…"
cargo build --release --features "$FEATURES"

rm -rf "$OUT"
mkdir -p "$OUT"
cp "target/release/dysflow$EXE" "$OUT/"

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

# Windows : le runtime Visual C++ qu'importent dysflow.exe et onnxruntime.dll,
# joint à côté d'eux (vc_crt_deploy, scripts/paquet-commun.sh).
if [ -n "$WINDOWS" ]; then
    vc_crt_deploy "$OUT"

    # Un mot d'accueil, en français puis en anglais.
    cat > "$OUT/LISEZMOI.txt" <<'LISEZMOI'
Dysflow pour Windows — lecteur au terminal
==========================================

Contenu
  dysflow.exe            le lecteur (lancez-le depuis PowerShell ou cmd)
  dysflow-espeak.exe     la phonétisation de la voix Kokoro (espeak-ng, GPL-3.0),
  espeak-ng-data\        ses données ; sources dans dysflow-espeak-source\
  onnxruntime.dll        ONNX Runtime de Microsoft, le calcul de la voix Kokoro
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

-----------------------------------------------------------------------------

Dysflow for Windows — terminal reader

Contents
  dysflow.exe            the reader (run it from PowerShell or cmd)
  dysflow-espeak.exe     phonemization for the Kokoro voice (espeak-ng, GPL-3.0),
  espeak-ng-data\        its data; sources in dysflow-espeak-source\
  onnxruntime.dll        Microsoft ONNX Runtime, which computes the Kokoro voice
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
