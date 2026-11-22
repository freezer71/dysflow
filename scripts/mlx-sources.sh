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
#     avec MACOSX_DEPLOYMENT_TARGET (build-ffi.sh : 14.0), son sous-dossier
#     macos-<version>. Le jeu de noyaux suit cette version (NAX dès 26.2) et
#     ne va qu'avec le MLX bâti pour elle : dans un seul dossier, le dernier
#     build effaçait l'autre, et `matmul` ne trouvait plus ses noyaux. À relire
#     après tout changement de cette variable ou de branche : ce qui n'est pas
#     prouvé bâti par ce script (cible, épingles, noyaux) est retiré, et refait ;
#   DYSFLOW_MLX_METALLIB : ce fichier, que compute::mlx lit à l'exécution
#     (tests de ffi/, exemples : rien n'est à côté de leur exécutable) ;
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

# mlx_cible_de DOSSIER : la cible des noyaux de ce dossier de
# target/mlx-metallib (macos-14.0 : 14.0 ; le dossier lui-même : vide, l'hôte).
mlx_cible_de() {
    case "${1##*/}" in
    macos-*) echo "${1##*/macos-}" ;;
    *) echo ;;
    esac
}

# mlx_noyaux_justes DOSSIER : sa mlx.metallib est entière et vise la cible du
# dossier. L'en-tête porte l'une et l'autre : octets 12-13 la majeure (petit
# boutiste), 14 la mineure, 16-23 la taille du fichier. Pour l'hôte, MLX prend
# la version de ce macOS.
mlx_noyaux_justes() {
    [ -s "$1/mlx.metallib" ] || return 1
    mlx_voulue=$(mlx_cible_de "$1")
    if [ -z "$mlx_voulue" ]; then mlx_voulue=$(sw_vers -productVersion); fi
    mlx_voulue=$(echo "$mlx_voulue" | awk -F. '{ printf "%d.%d", $1, $2 }')
    mlx_ecrite=$(od -An -tu1 -j12 -N3 "$1/mlx.metallib" | awk 'NF == 3 { printf "%d.%d", $1 + 256 * $2, $3 }')
    [ "$mlx_ecrite" = "$mlx_voulue" ] || return 1
    mlx_annoncee=$(od -An -tu8 -j16 -N8 "$1/mlx.metallib" | tr -d ' ')
    [ "$mlx_annoncee" = "$(wc -c < "$1/mlx.metallib" | tr -d ' ')" ]
}

# mlx_build_prouve BUILD : ce build de mlx-sys a fini, et son cache porte la
# preuve qu'écrit mlx-sources.cmake (cette chaîne d'outils, ces épingles, ce
# dossier de noyaux), la cible de ce dossier, et les noyaux y sont justes.
mlx_build_prouve() {
    mlx_cache=$1/out/build/CMakeCache.txt
    [ -f "$1/output" ] && [ -r "$mlx_cache" ] || return 1
    mlx_sortie=$(sed -n "s/^DYSFLOW_MLX_PREUVE:INTERNAL=$mlx_empreinte //p" "$mlx_cache") || return 1
    case "$mlx_sortie" in
    "$mlx_racine" | "$mlx_racine"/macos-*) ;;
    *) return 1 ;;
    esac
    grep -qxF "CMAKE_OSX_DEPLOYMENT_TARGET:STRING=$(mlx_cible_de "$mlx_sortie")" "$mlx_cache" || return 1
    mlx_noyaux_justes "$mlx_sortie"
}

# La garde échoue fermée : elle ne garde que ce qui est prouvé. CMake ignore la
# chaîne d'outils dans un dossier déjà configuré sans elle (un build de main) :
# le relancer ne répare rien, il faut le retirer. Et les noyaux sont partagés
# entre builds, sans dire qui les a écrits : un seul doute retire donc tous les
# builds de mlx-sys de target/ et tous les noyaux, que cargo refait (~3 min).
# Rien d'autre n'est effacé : seulement ce que les deux `find` ont listé.
mlx_metallib_garde() {
    mlx_racine=$PWD/target/mlx-metallib
    mlx_empreinte="$(shasum -a 256 < scripts/mlx-sources.sh | cut -c1-64) $(shasum -a 256 < scripts/mlx-sources.cmake | cut -c1-64)"
    mlx_builds=$(find target -maxdepth 5 -type d -path '*/build/mlx-sys-*' -prune -print) || return 1
    mlx_noyaux=$(find "$mlx_racine" -maxdepth 2 -name mlx.metallib) || return 1

    mlx_doute=
    while IFS= read -r mlx_un; do
        [ -n "$mlx_un" ] || continue
        # Le script de build compilé, qui n'a encore rien bâti.
        if [ -f "$mlx_un/build-script-build" ] && [ ! -e "$mlx_un/out" ]; then continue; fi
        mlx_build_prouve "$mlx_un" || mlx_doute=$mlx_un
    done <<EOF
$mlx_builds
EOF
    while IFS= read -r mlx_un; do
        [ -n "$mlx_un" ] || continue
        mlx_noyaux_justes "${mlx_un%/*}" || mlx_doute=$mlx_un
    done <<EOF
$mlx_noyaux
EOF
    [ -n "$mlx_doute" ] || return 0

    echo "MLX : $mlx_doute n'est pas prouvé, mlx-sys et ses noyaux seront refaits / is not proven, mlx-sys and its kernels will be rebuilt" >&2
    while IFS= read -r mlx_un; do
        [ -n "$mlx_un" ] || continue
        rm -rf "$mlx_un" "${mlx_un%/build/*}/.fingerprint/${mlx_un##*/}" || return 1
    done <<EOF
$mlx_builds
EOF
    while IFS= read -r mlx_un; do
        [ -n "$mlx_un" ] || continue
        rm -f "$mlx_un" || return 1
    done <<EOF
$mlx_noyaux
EOF
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
    export MLX_RS_METAL_PATH="$PWD/target/mlx-metallib${MACOSX_DEPLOYMENT_TARGET:+/macos-$MACOSX_DEPLOYMENT_TARGET}"
    export DYSFLOW_MLX_METALLIB="$MLX_RS_METAL_PATH/mlx.metallib"
    mkdir -p "$MLX_RS_METAL_PATH"
    mlx_metallib_garde
    ;;
esac
