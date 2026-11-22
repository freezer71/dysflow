# shellcheck shell=sh
# shellcheck disable=SC2034 # MLX_SOURCES_* servent à qui lit ce fichier.
# Sources épinglées de la copie Rust de MLX (mlx-rs 0.34 / mlx-sys 0.7.0), à
# lire par `.` depuis la racine du dépôt, avant tout build ou test qui lie
# mlx-sys (feature mlx) :
#
#   . scripts/mlx-sources.sh
#
# mlx-sys compile MLX par CMake, qui téléchargerait cinq sources sans
# empreinte (étiquettes Git, URL sans somme). Ce fichier les range dans
# target/mlx-sources, chacune à son commit complet ou à son empreinte SHA-256,
# et scripts/mlx-sources.cmake les désigne à CMake (FetchContent) en lui
# interdisant tout autre téléchargement.
#
# Il exporte :
#   CMAKE_TOOLCHAIN_FILE_aarch64_apple_darwin : le fichier CMake ci-dessus ;
#   MLX_RS_METAL_PATH : target/mlx-metallib, où mlx-sys écrit mlx.metallib
#     (les noyaux Metal), pour que le cache de la CI le garde avec target/ ;
#   et prépare target/mlx-sources/licences (préfixe mlx-rs-), que l'app joint.
#
# Sans effet hors d'un Mac Apple Silicon (MLX n'y est pas compilé). Sûr à lire
# deux fois, et sous `set -eu` : un dossier déjà là n'est repris que s'il est
# complet et juste ; sinon il est refait, jamais complété.

# shellcheck source=scripts/paquet-commun.sh
. scripts/paquet-commun.sh

MLX_SOURCES_DIR=$PWD/target/mlx-sources

# Les cinq épingles (spec des moteurs, décision « Lot 0 »).
MLX_COMMIT=1f8e74e3f12f31365464a6867c6579f0e9b29d85          # v0.32.2
FMT_COMMIT=407c905e45ad75fc29bf0f9bb7c5c2fd3475976f          # 12.1.0
GGUFLIB_COMMIT=8fa6eb65236618e28fd7710a0fba565f7faa1848
JSON_URL=https://github.com/nlohmann/json/releases/download/v3.11.3/json.tar.xz
JSON_SHA256=d6c65aca6b1ed68e7a182f4757257b107ae403032760ed6ef121c9d55e81757d
METAL_CPP_URL=https://developer.apple.com/metal/cpp/files/metal-cpp_26.zip
METAL_CPP_SHA256=4df3c078b9aadcb516212e9cb03004cbc5ce9a3e9c068fa3144d021db585a3a4

# Empreinte de la liste des fichiers d'un dossier (noms seulement) : un dossier
# vidé en partie, comme le rend parfois un cache, n'a plus la même.
mlx_tree_listing() {
    (cd "$1" && find . -type f | LC_ALL=C sort | shasum -a 256 | cut -d' ' -f1)
}

# mlx_git_ok DOSSIER COMMIT : DOSSIER est le dépôt Git de ce commit exact, sans
# fichier suivi manquant ni modifié (un checkout interrompu en laisse).
mlx_git_ok() {
    [ -e "$1/.git" ] || return 1
    [ "$(git -C "$1" rev-parse HEAD 2>/dev/null)" = "$2" ] || return 1
    [ -z "$(git -C "$1" status --porcelain --untracked-files=no 2>/dev/null)" ] || return 1
}

# mlx_fetch_git NOM URL COMMIT : le commit complet COMMIT de URL, dans
# $MLX_SOURCES_DIR/NOM. Récupéré dans un dossier provisoire, mis en place
# seulement une fois le checkout fini et son commit relu.
mlx_fetch_git() {
    if mlx_git_ok "$MLX_SOURCES_DIR/$1" "$3"; then
        return 0
    fi
    echo "MLX : récupération de $1 ($3)" >&2
    rm -rf "${MLX_SOURCES_DIR:?}/$1" "$MLX_SOURCES_DIR/$1.part"
    mkdir -p "$MLX_SOURCES_DIR/$1.part"
    git -C "$MLX_SOURCES_DIR/$1.part" init -q
    git -C "$MLX_SOURCES_DIR/$1.part" fetch -q --depth 1 "$2" "$3"
    git -C "$MLX_SOURCES_DIR/$1.part" -c advice.detachedHead=false checkout -q FETCH_HEAD
    if ! mlx_git_ok "$MLX_SOURCES_DIR/$1.part" "$3"; then
        rm -rf "$MLX_SOURCES_DIR/$1.part"
        echo "Erreur : $1 n'est pas au commit $3 / $1 is not at commit $3" >&2
        exit 1
    fi
    mv "$MLX_SOURCES_DIR/$1.part" "$MLX_SOURCES_DIR/$1"
}

# mlx_archive_ok NOM SHA256 : le dossier de NOM vient de l'archive de cette
# empreinte, et a toujours les fichiers qu'on y avait mis.
mlx_archive_ok() {
    [ -d "$MLX_SOURCES_DIR/$1" ] || return 1
    [ -f "$MLX_SOURCES_DIR/$1.pin" ] || return 1
    [ "$(cat "$MLX_SOURCES_DIR/$1.pin")" = "$2 $(mlx_tree_listing "$MLX_SOURCES_DIR/$1")" ]
}

# mlx_fetch_archive NOM URL SHA256 TYPE : l'archive (tar ou zip) de URL, vérifiée
# par SHA-256, extraite dans $MLX_SOURCES_DIR/NOM sans son dossier de tête (comme
# le fait FetchContent). fetch_verified ne garde qu'une archive à la bonne
# empreinte ; le dossier n'est mis en place qu'extrait en entier.
mlx_fetch_archive() {
    mlx_archive_ok "$1" "$3" && return 0
    echo "MLX : récupération de $1 ($3)" >&2
    fetch_verified "$2" "$3" "$MLX_SOURCES_DIR/downloads/$1.$4"
    rm -rf "${MLX_SOURCES_DIR:?}/$1" "$MLX_SOURCES_DIR/$1.pin" "$MLX_SOURCES_DIR/$1.part"
    mkdir -p "$MLX_SOURCES_DIR/$1.part/extrait"
    case "$4" in
    zip) unzip -q "$MLX_SOURCES_DIR/downloads/$1.$4" -d "$MLX_SOURCES_DIR/$1.part/extrait" ;;
    *) tar -xf "$MLX_SOURCES_DIR/downloads/$1.$4" -C "$MLX_SOURCES_DIR/$1.part/extrait" ;;
    esac
    # Un seul dossier de tête, et rien d'autre.
    set -- "$1" "$3" "$MLX_SOURCES_DIR/$1.part"/extrait/*
    if [ "$#" -ne 3 ] || [ ! -d "$3" ]; then
        rm -rf "$MLX_SOURCES_DIR/$1.part"
        echo "Erreur : l'archive de $1 n'a pas un dossier unique / archive of $1 has not a single top directory" >&2
        exit 1
    fi
    mv "$3" "$MLX_SOURCES_DIR/$1"
    rm -rf "$MLX_SOURCES_DIR/$1.part"
    echo "$2 $(mlx_tree_listing "$MLX_SOURCES_DIR/$1")" > "$MLX_SOURCES_DIR/$1.pin"
}

# mlx_licence SOURCE NOM : copie SOURCE en target/mlx-sources/licences/NOM ; un
# fichier absent ou vide arrête.
mlx_licence() {
    if [ ! -s "$1" ]; then
        echo "Erreur : licence introuvable ou vide : $1 / licence not found or empty" >&2
        exit 1
    fi
    cp "$1" "$MLX_SOURCES_DIR/licences/$2"
}

# mlx_licences : target/mlx-sources/licences/mlx-rs-*, les licences de la copie
# Rust, prises dans les sources ci-dessus et dans mlx-c, que mlx-sys porte.
mlx_licences() {
    mlx_c=$(cargo metadata --format-version 1 --locked --filter-platform aarch64-apple-darwin --all-features |
        python3 -I -c 'import json, os, sys
for p in json.load(sys.stdin)["packages"]:
    if p["name"] == "mlx-sys":
        print(os.path.join(os.path.dirname(p["manifest_path"]), "src", "mlx-c", "LICENSE"))
        break')
    mkdir -p "$MLX_SOURCES_DIR/licences"
    mlx_licence "$MLX_SOURCES_DIR/mlx/LICENSE" mlx-rs-mlx-LICENSE
    mlx_licence "$mlx_c" mlx-rs-mlx-c-LICENSE
    mlx_licence "$MLX_SOURCES_DIR/metal_cpp/LICENSE.txt" mlx-rs-metal-cpp-LICENSE
    mlx_licence "$MLX_SOURCES_DIR/json/LICENSE.MIT" mlx-rs-nlohmann-json-LICENSE
    mlx_licence "$MLX_SOURCES_DIR/fmt/LICENSE" mlx-rs-fmt-LICENSE
    mlx_licence "$MLX_SOURCES_DIR/gguflib/LICENSE" mlx-rs-gguflib-LICENSE
}

# Les noyaux Metal : si mlx.metallib manque, un build de mlx-sys restauré (cache
# de la CI) ne les réécrirait pas, et MLX ne trouverait pas ses noyaux à
# l'exécution. On retire alors ce build : cargo le refait, noyaux compris.
mlx_metallib_garde() {
    [ -s "$MLX_RS_METAL_PATH/mlx.metallib" ] && return 0
    mlx_restes=$(find target -maxdepth 5 -type d \( -path '*/build/mlx-sys-*' -o -path '*/.fingerprint/mlx-sys-*' \) 2>/dev/null)
    [ -n "$mlx_restes" ] || return 0
    echo "MLX : mlx.metallib manque, le build de mlx-sys sera refait / mlx.metallib is missing, mlx-sys will be rebuilt" >&2
    echo "$mlx_restes" | while IFS= read -r mlx_reste; do rm -rf "$mlx_reste"; done
}

case "$(uname -s)-$(uname -m)" in
Darwin-arm64)
    mkdir -p "$MLX_SOURCES_DIR"
    mlx_fetch_git mlx https://github.com/ml-explore/mlx.git "$MLX_COMMIT"
    mlx_fetch_git fmt https://github.com/fmtlib/fmt.git "$FMT_COMMIT"
    mlx_fetch_git gguflib https://github.com/antirez/gguf-tools.git "$GGUFLIB_COMMIT"
    mlx_fetch_archive json "$JSON_URL" "$JSON_SHA256" tar.xz
    mlx_fetch_archive metal_cpp "$METAL_CPP_URL" "$METAL_CPP_SHA256" zip
    mlx_licences
    export CMAKE_TOOLCHAIN_FILE_aarch64_apple_darwin="$PWD/scripts/mlx-sources.cmake"
    export MLX_RS_METAL_PATH="$PWD/target/mlx-metallib"
    mkdir -p "$MLX_RS_METAL_PATH"
    mlx_metallib_garde
    ;;
esac
