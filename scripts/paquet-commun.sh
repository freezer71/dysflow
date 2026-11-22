# shellcheck shell=sh
# shellcheck disable=SC2034 # ORT_* servent à qui lit ce fichier.
# Fonctions partagées par package.sh et ort-gate.sh, à lire par `.` depuis la
# racine du dépôt, avec PYTHON posé (Python 3) : téléchargement vérifié par
# SHA-256, ONNX Runtime 1.31.0 pris dans le paquet NuGet de Microsoft, et le
# runtime Visual C++ joint sous Windows.

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

# fetch_verified URL SHA256 FICHIER : télécharge URL dans FICHIER, sous un nom
# provisoire, gardé seulement si son empreinte est la bonne. Un FICHIER déjà
# là et juste n'est pas retéléchargé.
fetch_verified() {
    if [ -f "$3" ] && sha256_ok "$2" "$3"; then
        return 0
    fi
    rm -f "$3" "$3.part"
    mkdir -p "$(dirname "$3")"
    curl -fsSL "$1" -o "$3.part"
    if ! sha256_ok "$2" "$3.part"; then
        rm -f "$3.part"
        echo "Erreur : empreinte SHA-256 inattendue pour $1" >&2
        exit 1
    fi
    mv "$3.part" "$3"
}

ORT_VERSION=1.31.0

# ort_fetch : ONNX Runtime $ORT_VERSION de ce système, extrait dans $ORT_DIR.
# Pose ORT_RID (le dossier runtimes/<RID>/native/ du paquet NuGet), ORT_LIB (la
# bibliothèque dedans) et ORT_DEST (son nom à côté du binaire, celui que
# cherche src/tts/kokoro/onnx.rs) ; ORT_RID vide si le système n'a pas d'ORT
# joint.
#
# Une seule archive pour tous les systèmes : le paquet NuGet officiel de
# Microsoft (le dépôt GitHub microsoft/onnxruntime a disparu, ses archives de
# publication avec). Son empreinte est épinglée : une archive différente n'est
# jamais utilisée. Seule la bibliothèque est extraite, pas
# libonnxruntime_providers_shared : elle ne sert qu'à charger les fournisseurs
# CUDA et TensorRT, pas le fournisseur CPU que Dysflow utilise.
ort_fetch() {
    ORT_NUPKG="microsoft.ml.onnxruntime.$ORT_VERSION.nupkg"
    ORT_URL="https://api.nuget.org/v3-flatcontainer/microsoft.ml.onnxruntime/$ORT_VERSION/$ORT_NUPKG"
    ORT_SHA256=7e35fdb4f5adb62f01e60798e2e5ed05108f5fbbe1cca03fbcc37903cf7c5e3b
    case "$(uname -s)-$(uname -m)" in
    Linux-x86_64)
        ORT_RID=linux-x64 ORT_LIB=libonnxruntime.so ORT_DEST=libonnxruntime.so ;;
    Linux-aarch64)
        ORT_RID=linux-arm64 ORT_LIB=libonnxruntime.so ORT_DEST=libonnxruntime.so ;;
    Darwin-arm64)
        ORT_RID=osx-arm64 ORT_LIB=libonnxruntime.dylib ORT_DEST=libonnxruntime.dylib ;;
    MINGW*-x86_64 | MSYS*-x86_64 | CYGWIN*-x86_64)
        ORT_RID=win-x64 ORT_LIB=onnxruntime.dll ORT_DEST=onnxruntime.dll ;;
    *) ORT_RID="" ; return 0 ;;
    esac
    ORT_DIR="target/ort/$ORT_VERSION-$ORT_RID"
    # Extrait à part, mis en place seulement une fois complet : une extraction
    # interrompue est refaite au lancement suivant. Les fichiers, pas le
    # dossier : le cache de la CI (rust-cache) rend target/ort vidé de ses
    # fichiers, et un dossier présent mais vide faisait échouer la copie.
    if [ -f "$ORT_DIR/$ORT_LIB" ] && [ -f "$ORT_DIR/LICENSE" ] && [ -f "$ORT_DIR/ThirdPartyNotices.txt" ]; then
        return 0
    fi
    fetch_verified "$ORT_URL" "$ORT_SHA256" "target/ort/$ORT_NUPKG"
    if ! command -v unzip >/dev/null 2>&1; then
        echo "Erreur : unzip manque (il ouvre le paquet NuGet d'ONNX Runtime)" >&2
        exit 1
    fi
    rm -rf target/ort/extraction
    mkdir -p target/ort/extraction
    unzip -q -j "target/ort/$ORT_NUPKG" "runtimes/$ORT_RID/native/$ORT_LIB" LICENSE ThirdPartyNotices.txt \
        -d target/ort/extraction
    rm -rf "$ORT_DIR"
    mv target/ort/extraction "$ORT_DIR"
}

# vc_crt_deploy DIST (Windows) : le runtime Visual C++ (vcruntime140,
# msvcp140…) qu'importent les binaires de DIST, joint à côté d'eux : un PC
# sans le redistribuable les lance quand même (déploiement local, permis par
# la licence de Visual Studio). Les DLL sont celles du Visual Studio qui a
# compilé, trouvé par vswhere (à son emplacement fixe, ou VSWHERE=…), dans
# VC\Redist\MSVC\<version par défaut>\x64\Microsoft.VC14x.CRT. Seules
# celles qui sont réellement importées sont jointes.
vc_crt_deploy() {
    vswhere="${VSWHERE:-/c/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe}"
    vs=$("$vswhere" -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 \
        -property installationPath | tr -d '\r')
    if [ -z "$vs" ]; then
        echo "Erreur : Visual Studio (outils C++ x64) introuvable par vswhere" >&2
        exit 1
    fi
    vs=$(cygpath -u "$vs")
    redist=$(tr -d '\r\n ' < "$vs/VC/Auxiliary/Build/Microsoft.VCRedistVersion.default.txt")
    crt=$(find "$vs/VC/Redist/MSVC/$redist/x64" -maxdepth 1 -type d -name 'Microsoft.VC14*.CRT' | head -1)
    if [ -z "$crt" ]; then
        echo "Erreur : runtime Visual C++ $redist introuvable dans $vs/VC/Redist/MSVC" >&2
        exit 1
    fi
    "$PYTHON" -I scripts/windows-imports.py deploy-crt "$crt" "$1"
}
