# shellcheck shell=sh
# shellcheck disable=SC2034 # SHERPA_* servent à qui lit ce fichier.
# La bibliothèque C de sherpa-onnx, sur le même ONNX Runtime que Kokoro. À lire
# par `.` depuis la racine du dépôt, après scripts/paquet-commun.sh, avec PYTHON
# posé (Python 3).
#
# sherpa_fetch prépare $SHERPA_LIB_DIR : la bibliothèque C de sherpa-onnx
# $SHERPA_VERSION ($SHERPA_LIB ; sous Windows aussi sa bibliothèque
# d'importation), tirée de l'archive publiée par k2-fsa, empreinte épinglée.
# L'ONNX Runtime 1.28.2 de l'archive n'est jamais pris : à l'exécution, la
# bibliothèque C charge celui qui est à côté d'elle, l'ONNX Runtime
# $ORT_VERSION de ort_fetch (paquet NuGet de Microsoft), sous le nom qu'elle
# demande (libonnxruntime.so, libonnxruntime.dylib, onnxruntime.dll). Sous
# Linux, elle demande OrtGetApiBase@VERS_1.28.2 : scripts/elf-version-ref.py
# renomme la requête en VERS_$ORT_VERSION.
#
# Puis SHERPA_CONFIG_SEARCH et SHERPA_CONFIG_LIB, à passer à cargo par
# --config : ils remplacent le script de build de sherpa-onnx-sys (clé
# `links = "sherpa-onnx"`). Le binaire est lié à la bibliothèque C seule :
# pas à ONNX Runtime, dont il n'appelle rien (le script de build l'y lierait,
# sous un autre nom que celui du fichier livré), sans téléchargement ni
# chemin absolu vers l'arbre de compilation.

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
    # Un chemin absolu, lisible par cargo : sous Windows, en C:/… . Le nom
    # porte les deux versions (la requête renommée sous Linux dépend d'ORT).
    root=$PWD
    case "$ORT_RID" in win-*) root=$(cygpath -m "$PWD") ;; esac
    SHERPA_LIB_DIR="$root/target/sherpa-onnx/$SHERPA_VERSION-ort$ORT_VERSION-$ORT_RID"

    # La cible de cargo : CARGO_BUILD_TARGET (posé par la CI macOS), sinon
    # celle de rustc.
    triple=${CARGO_BUILD_TARGET:-$(rustc -vV | sed -n 's/^host: //p')}
    SHERPA_CONFIG_SEARCH="target.$triple.sherpa-onnx.rustc-link-search=['native=$SHERPA_LIB_DIR']"
    SHERPA_CONFIG_LIB="target.$triple.sherpa-onnx.rustc-link-lib=['dylib=sherpa-onnx-c-api']"

    if [ -f "$SHERPA_LIB_DIR/$SHERPA_LIB" ] \
        && { [ -z "$SHERPA_IMPLIB" ] || [ -f "$SHERPA_LIB_DIR/$SHERPA_IMPLIB" ]; } \
        && { [ "${ORT_RID#linux-}" = "$ORT_RID" ] || [ -f "$SHERPA_LIB_DIR/$ORT_DEST" ]; }; then
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
    case "$ORT_RID" in
    linux-*)
        "$PYTHON" -I scripts/elf-version-ref.py "target/sherpa-onnx/extraction/$SHERPA_LIB" \
            libonnxruntime.so VERS_1.28.2 "VERS_$ORT_VERSION"
        # L'éditeur de liens de GNU résout aussi les dépendances de la
        # bibliothèque C (par son RPATH $ORIGIN) : sans ORT à côté d'elle,
        # « undefined reference to OrtGetApiBase@VERS_1.31.0 » (vu sous
        # ARM64). Le binaire n'y gagne aucune dépendance directe.
        cp "$ORT_DIR/$ORT_LIB" "target/sherpa-onnx/extraction/$ORT_DEST" ;;
    esac
    rm -rf "$SHERPA_LIB_DIR"
    mv target/sherpa-onnx/extraction "$SHERPA_LIB_DIR"
}
