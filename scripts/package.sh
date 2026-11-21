#!/bin/sh
# Assemble un dossier distribuable : le binaire et les bibliothèques et
# programmes qu'il charge (pour Kokoro, ONNX Runtime et dysflow-espeak avec ses
# données et ses sources GPL).
set -eu
cd "$(dirname "$0")/.."

# Vrai si le fichier $2 a l'empreinte SHA-256 $1 : sha256sum (GNU, ou BSD
# sous macOS, d'où `-c -` sans `--status`), sinon shasum (Perl).
# L'empreinte épinglée doit faire exactement 64 chiffres hexadécimaux : le
# sha256sum BSD accepte une ligne mal formée (code 0), qui ne vérifierait rien.
sha256_ok() {
    case "$1" in
    *[!0-9a-f]*) echo "Erreur : empreinte épinglée mal formée : $1" >&2; exit 1 ;;
    esac
    if [ "${#1}" -ne 64 ]; then
        echo "Erreur : empreinte épinglée mal formée (${#1} caractères au lieu de 64) : $1" >&2
        exit 1
    fi
    if command -v sha256sum >/dev/null 2>&1; then
        echo "$1  $2" | sha256sum -c - >/dev/null 2>&1
    else
        echo "$1  $2" | shasum -a 256 -c --status
    fi
}

FEATURES="${FEATURES:-ocr,kokoro-onnx}"
OUT="${OUT:-dist}"

# Les licences des crates Rust liées : THIRD-PARTY-RUST.txt doit suivre
# Cargo.lock (python3 scripts/third-party-rust.py pour le régénérer) ; le
# script refuse une licence GPL, AGPL ou inconnue dans le graphe lié. Il
# décrit les features par défaut ci-dessous : avec d'autres, le régénérer.
python3 -I scripts/third-party-rust.py --check

echo "Compilation (features : $FEATURES)…"
cargo build --release --features "$FEATURES"

rm -rf "$OUT"
mkdir -p "$OUT"
cp target/release/dysflow "$OUT/"

# Kokoro au terminal : ONNX Runtime 1.31 à côté du binaire, et la
# phonétisation dysflow-espeak (GPL-3.0, programme séparé que Dysflow lance) avec
# ses données, sa licence et ses sources correspondantes.
case ",$FEATURES," in
*,kokoro-onnx,*)
    if [ ! -x target/espeak/bin/dysflow-espeak ] || [ -n "$(find scripts/dysflow-espeak.c -newer target/espeak/bin/dysflow-espeak)" ]; then
        ./scripts/build-espeak.sh
    fi
    cp target/espeak/bin/dysflow-espeak "$OUT/"
    cp -R target/espeak/share/espeak-ng-data "$OUT/"
    mkdir -p "$OUT/dysflow-espeak-source"
    cp target/espeak/COPYING target/espeak/SOURCE.txt target/espeak/dysflow-espeak.c \
        scripts/build-espeak.sh "$OUT/dysflow-espeak-source/"
    echo "Joint : dysflow-espeak, espeak-ng-data et ses sources (dysflow-espeak-source/)"

    ORT_VERSION=1.31.0
    # Une seule archive pour tous les systèmes : le paquet NuGet officiel de
    # Microsoft (le dépôt GitHub microsoft/onnxruntime a disparu, ses archives
    # de publication avec). Son empreinte est épinglée : une archive différente
    # n'est jamais utilisée.
    ORT_NUPKG="microsoft.ml.onnxruntime.$ORT_VERSION.nupkg"
    ORT_URL="https://api.nuget.org/v3-flatcontainer/microsoft.ml.onnxruntime/$ORT_VERSION/$ORT_NUPKG"
    ORT_SHA256=7e35fdb4f5adb62f01e60798e2e5ed05108f5fbbe1cca03fbcc37903cf7c5e3b
    # ORT_RID : le dossier runtimes/<RID>/native/ du paquet. ORT_LIB : la
    # bibliothèque dedans, ORT_DEST son nom à côté du binaire (celui que
    # cherche src/tts/kokoro/onnx.rs). Seule cette bibliothèque est jointe, pas
    # libonnxruntime_providers_shared.so : elle ne sert qu'à charger les
    # fournisseurs CUDA et TensorRT, pas le fournisseur CPU que Dysflow utilise.
    # Windows x64 prêt pour le paquet Windows (aucun système détecté ici).
    case "$(uname -s)-$(uname -m)" in
    Linux-x86_64)
        ORT_RID=linux-x64 ORT_LIB=libonnxruntime.so ORT_DEST=libonnxruntime.so ;;
    Linux-aarch64)
        ORT_RID=linux-arm64 ORT_LIB=libonnxruntime.so ORT_DEST=libonnxruntime.so ;;
    Darwin-arm64)
        ORT_RID=osx-arm64 ORT_LIB=libonnxruntime.dylib ORT_DEST=libonnxruntime.dylib ;;
    MINGW*-x86_64 | MSYS*-x86_64 | CYGWIN*-x86_64)
        ORT_RID=win-x64 ORT_LIB=onnxruntime.dll ORT_DEST=onnxruntime.dll ;;
    *) ORT_RID="" ;;
    esac
    if [ -n "$ORT_RID" ]; then
        ORT_DIR="target/ort/$ORT_VERSION-$ORT_RID"
        mkdir -p target/ort
        # Extrait à part, mis en place seulement une fois complet : une
        # extraction interrompue est refaite au lancement suivant.
        # Le fichier, pas le dossier : le cache de la CI (rust-cache) rend
        # target/ort vidé de ses fichiers, et un dossier présent mais vide
        # faisait échouer la copie plus bas.
        if [ ! -f "$ORT_DIR/$ORT_LIB" ] \
            || [ ! -f "$ORT_DIR/LICENSE" ] \
            || [ ! -f "$ORT_DIR/ThirdPartyNotices.txt" ]; then
            # L'archive : téléchargée sous un nom provisoire, gardée seulement
            # si son empreinte est la bonne.
            nupkg="target/ort/$ORT_NUPKG"
            if [ ! -f "$nupkg" ] || ! sha256_ok "$ORT_SHA256" "$nupkg"; then
                rm -f "$nupkg" "$nupkg.part"
                curl -fsSL "$ORT_URL" -o "$nupkg.part"
                if ! sha256_ok "$ORT_SHA256" "$nupkg.part"; then
                    rm -f "$nupkg.part"
                    echo "Erreur : empreinte SHA-256 inattendue pour $ORT_NUPKG" >&2
                    exit 1
                fi
                mv "$nupkg.part" "$nupkg"
            fi
            rm -rf target/ort/extraction
            mkdir -p target/ort/extraction
            unzip -q -j "$nupkg" "runtimes/$ORT_RID/native/$ORT_LIB" LICENSE ThirdPartyNotices.txt \
                -d target/ort/extraction
            rm -rf "$ORT_DIR"
            mv target/ort/extraction "$ORT_DIR"
        fi
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

echo "Vérification hors de l'arbre de compilation…"
( cd "$OUT" && ./dysflow --show-config >/dev/null )
echo "Prêt : $OUT/"
ls -lh "$OUT"
