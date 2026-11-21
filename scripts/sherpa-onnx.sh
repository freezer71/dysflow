# shellcheck shell=sh
# shellcheck disable=SC2034 # SHERPA_* servent à qui lit ce fichier.
# La bibliothèque C de sherpa-onnx, sur le même ONNX Runtime que Kokoro. À lire
# par `.` depuis la racine du dépôt, après scripts/paquet-commun.sh, avec PYTHON
# posé (Python 3).
#
# sherpa_fetch prépare $SHERPA_ONNX_LIB_DIR, que sherpa-onnx-sys (feature
# `shared`) prend au build au lieu de télécharger sa propre archive :
#   - la bibliothèque C de sherpa-onnx $SHERPA_VERSION ($SHERPA_LIB ; sous
#     Windows aussi sa bibliothèque d'importation), tirée de l'archive
#     publiée par k2-fsa, empreinte épinglée ;
#   - à côté, sous le nom que cette bibliothèque demande, l'ONNX Runtime
#     $ORT_VERSION de ort_fetch (paquet NuGet de Microsoft), et non le 1.28.2
#     de l'archive, qui n'est jamais copié.
# Sous Linux, la bibliothèque C demande OrtGetApiBase@VERS_1.28.2 :
# scripts/elf-version-ref.py renomme la requête en VERS_$ORT_VERSION.

SHERPA_VERSION=2.0.2

sherpa_fetch() {
    ort_fetch
    case "$ORT_RID" in
    linux-x64)
        SHERPA_ARCHIVE=linux-x64-shared-lib
        SHERPA_SHA256=e99e08d851148b252d141714005eb5a7ad2c49b54a738281704a3dab6f70a376
        SHERPA_LIB=libsherpa-onnx-c-api.so SHERPA_IMPLIB="" ;;
    linux-arm64)
        SHERPA_ARCHIVE=linux-aarch64-shared-cpu-lib
        SHERPA_SHA256=b34335d4c3561e4dac82645c2f725ace6cede8fee5995e505d7076ae52d440b9
        SHERPA_LIB=libsherpa-onnx-c-api.so SHERPA_IMPLIB="" ;;
    osx-arm64)
        SHERPA_ARCHIVE=osx-arm64-shared-lib
        SHERPA_SHA256=1da59e61906f1c8dba86d80c799e34ef20eb89e90b49ea7d8bf7ef1257fb73ed
        SHERPA_LIB=libsherpa-onnx-c-api.dylib SHERPA_IMPLIB="" ;;
    win-x64)
        SHERPA_ARCHIVE=win-x64-shared-MT-Release-lib
        SHERPA_SHA256=14db8ff397f56e4b102e958a2f29c3a13a2eb40643a3a4d0373b65b2b82c4bb3
        SHERPA_LIB=sherpa-onnx-c-api.dll SHERPA_IMPLIB=sherpa-onnx-c-api.lib ;;
    *)
        echo "Erreur : sherpa-onnx n'est pas prévu sur $(uname -s)-$(uname -m)" >&2
        exit 1 ;;
    esac
    SHERPA_STEM="sherpa-onnx-v$SHERPA_VERSION-$SHERPA_ARCHIVE"
    SHERPA_URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/v$SHERPA_VERSION/$SHERPA_STEM.tar.bz2"
    # Le nom porte les deux versions : changer l'une refait le dossier.
    # Un chemin absolu, lisible par build.rs : sous Windows, en C:/… .
    root=$PWD
    case "$ORT_RID" in win-*) root=$(cygpath -m "$PWD") ;; esac
    SHERPA_ONNX_LIB_DIR="$root/target/sherpa-onnx/$SHERPA_VERSION-ort$ORT_VERSION-$ORT_RID"
    if [ -f "$SHERPA_ONNX_LIB_DIR/$SHERPA_LIB" ] && [ -f "$SHERPA_ONNX_LIB_DIR/$ORT_DEST" ] \
        && { [ -z "$SHERPA_IMPLIB" ] || [ -f "$SHERPA_ONNX_LIB_DIR/$SHERPA_IMPLIB" ]; } \
        && { [ "$ORT_RID" != osx-arm64 ] || [ -f "$SHERPA_ONNX_LIB_DIR/libonnxruntime.tbd" ]; }; then
        return 0
    fi
    fetch_verified "$SHERPA_URL" "$SHERPA_SHA256" "target/sherpa-onnx/$SHERPA_STEM.tar.bz2"
    # Extrait à part, mis en place seulement une fois complet. Par Python :
    # le tar de Git for Windows ne lit pas toujours le bzip2.
    rm -rf target/sherpa-onnx/extraction
    mkdir -p target/sherpa-onnx/extraction
    "$PYTHON" -I -c '
import sys, tarfile
from pathlib import Path
archive, stem, out = sys.argv[1], sys.argv[2], Path(sys.argv[3])
with tarfile.open(archive, "r:bz2") as tar:
    for name in sys.argv[4:]:
        member = tar.getmember(f"{stem}/lib/{name}")
        (out / name).write_bytes(tar.extractfile(member).read())
' "target/sherpa-onnx/$SHERPA_STEM.tar.bz2" "$SHERPA_STEM" target/sherpa-onnx/extraction \
        "$SHERPA_LIB" ${SHERPA_IMPLIB:+"$SHERPA_IMPLIB"}
    chmod +x "target/sherpa-onnx/extraction/$SHERPA_LIB"
    cp "$ORT_DIR/$ORT_LIB" "target/sherpa-onnx/extraction/$ORT_DEST"
    if [ -n "$ORT_IMPLIB" ]; then
        cp "$ORT_DIR/$ORT_IMPLIB" target/sherpa-onnx/extraction/
    fi
    case "$ORT_RID" in
    linux-*)
        "$PYTHON" -I scripts/elf-version-ref.py "target/sherpa-onnx/extraction/$SHERPA_LIB" \
            libonnxruntime.so VERS_1.28.2 "VERS_$ORT_VERSION" ;;
    osx-*)
        # sherpa-onnx-sys lie aussi le binaire à ONNX Runtime (-lonnxruntime),
        # sans qu'il en appelle rien ; l'éditeur de liens y inscrirait le nom
        # d'installation du fichier NuGet, @rpath/libonnxruntime.1.dylib, que
        # le paquet n'a pas. Ce descripteur (lu avant le .dylib) lui donne le
        # nom que demande déjà la bibliothèque C de sherpa-onnx et que porte
        # le fichier livré : @rpath/libonnxruntime.dylib. Le fichier de
        # Microsoft n'est pas modifié.
        cat > target/sherpa-onnx/extraction/libonnxruntime.tbd <<TBD
--- !tapi-tbd
tbd-version: 4
targets: [ arm64-macos ]
install-name: '@rpath/libonnxruntime.dylib'
current-version: $ORT_VERSION
exports:
  - targets: [ arm64-macos ]
    symbols: [ _OrtGetApiBase ]
...
TBD
        ;;
    esac
    rm -rf "$SHERPA_ONNX_LIB_DIR"
    mv target/sherpa-onnx/extraction "$SHERPA_ONNX_LIB_DIR"
}
