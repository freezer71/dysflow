#!/bin/sh
# La parité des ports MLX (procédure de port, P2 et P3) : les essais ignorés
# `<module>::parity` comparent le Rust à la fixture figée par le Swift, étage
# par étage, jetons et texte compris, sans tolérance qui cache un écart.
#
# Ils tournent dans un build à part, avec les noyaux Metal du Swift : mlx-sys
# aux noyaux JIT (`--features mlx,mlx-rs/metal-jit`, sous target/mlx-parite),
# comme mlx-swift, et pour les noyaux que MLX ne compile pas à l'exécution, le
# `default.metallib` qu'Xcode compile pour mlx-swift (même commit de MLX).
# Avec le mlx.metallib de mlx-sys (CMake), la Conv2d par canal
# (`depthwise_conv_2d`) et `layer_norm` arrondissent autrement d'une ulp, et
# un décodage glouton finit par changer de jeton (tâche 14). Ce build ne sert
# qu'à prouver la traduction : le produit garde le mlx.metallib de mlx-sys
# (le choix des noyaux livrés est celui de la tâche 26).
#
# `--livre` : la garde du build livré, au lieu de la parité. Le build standard
# du dépôt (target/, le mlx.metallib de mlx-sys, sans metal-jit), et par défaut
# stt::mlx::parakeet::tests::aot_short_inputs, 2 essais : sur les entrées
# courtes, pas, jetons et texte de la fixture ; la divergence connue
# (douze_secondes) bornée en nombre de pas.
#
#   DYSFLOW_MODELS=/chemin/des/modeles ./scripts/mlx-parite.sh [--livre] [FILTRE NOMBRE]…
#
# DYSFLOW_MODELS : le dossier qui contient un sous-dossier par modèle
# (parakeet_tdt_v3/, parakeet_tdt_v2/…). Les noyaux du Swift sont ceux de
# macos/build/derived, que ./scripts/test-macos.sh construit
# (DYSFLOW_SWIFT_METALLIB pour un autre fichier). Par défaut :
# stt::mlx::parakeet::tests::parity, 2 essais. Un essai sauté (« ignored »)
# ou un nombre d'essais passés différent fait échouer le script.
set -eu
cd "$(dirname "$0")/.."

: "${DYSFLOW_MODELS:?DYSFLOW_MODELS : le dossier des modèles}"
export DYSFLOW_MODELS

LIVRE=
if [ "${1:-}" = --livre ]; then
    LIVRE=1
    shift
fi
if [ "$#" -eq 0 ]; then
    if [ -n "$LIVRE" ]; then
        set -- stt::mlx::parakeet::tests::aot_short_inputs 2
    else
        set -- stt::mlx::parakeet::tests::parity 2
    fi
fi

# Les sources épinglées de MLX, puis un target et des noyaux à part : le build
# précompilé du dépôt (target/, target/mlx-metallib) n'est jamais touché.
#
# Le target de cargo est à 3 niveaux sous target/ : la garde de mlx-sources.sh
# (mlx_metallib_garde) cherche les builds de mlx-sys jusqu'à
# target/*/*/build/mlx-sys-* (`find target -maxdepth 5`), et un build dont les
# noyaux ne sont pas sous target/mlx-metallib lui paraîtrait non prouvé : elle
# retirerait alors TOUS les builds de mlx-sys de target/ et leurs noyaux, à
# refaire (~3 min) au prochain `. scripts/mlx-sources.sh`. Hors de sa portée,
# ce build-ci ne déclenche rien ; vérifié plus bas.
# shellcheck source=scripts/mlx-sources.sh
. scripts/mlx-sources.sh
PARITE="$PWD/target/mlx-parite"
mkdir -p "$PARITE"
if [ -n "$LIVRE" ]; then
    # Le build livré tel quel : target/ et les noyaux de mlx-sources.sh.
    FEATURES=mlx
    cargo test --release --locked -p dysflow --features "$FEATURES" --lib --no-run -j "${CARGO_BUILD_JOBS:-4}"
else
    export CARGO_TARGET_DIR="$PARITE/jit/target"
    export MLX_RS_METAL_PATH="$PARITE/mlx-metallib"
    export DYSFLOW_MLX_METALLIB="$MLX_RS_METAL_PATH/mlx.metallib"
    mkdir -p "$MLX_RS_METAL_PATH"
    swift_metallib=${DYSFLOW_SWIFT_METALLIB:-$PWD/macos/build/derived/Build/Products/Debug/mlx-swift_Cmlx.bundle/Contents/Resources/default.metallib}
    if [ ! -f "$swift_metallib" ]; then
        echo "Erreur : noyaux du Swift introuvables : $swift_metallib (./scripts/test-macos.sh les construit)" >&2
        exit 1
    fi

    FEATURES=mlx,mlx-rs/metal-jit
    cargo test --release --locked -p dysflow --features "$FEATURES" --lib --no-run -j "${CARGO_BUILD_JOBS:-4}"
    if find target -maxdepth 5 -type d -path '*/build/mlx-sys-*' | grep -q '^target/mlx-parite/'; then
        echo "Erreur : un build de mlx-sys sous target/mlx-parite est à portée de la garde de mlx-sources.sh" >&2
        exit 1
    fi
    # Lu à l'exécution par compute::mlx (metallib.rs), à la place de celui de mlx-sys.
    export DYSFLOW_MLX_METALLIB="$swift_metallib"
fi

log="$PARITE/parite.log"
while [ "$#" -ge 2 ]; do
    filter=$1 count=$2
    shift 2
    if ! cargo test --release --locked -p dysflow --features "$FEATURES" --lib -j "${CARGO_BUILD_JOBS:-4}" \
        "$filter" -- --ignored --test-threads=1 >"$log" 2>&1; then
        cat "$log"
        echo "Erreur : parité de $filter en échec" >&2
        exit 1
    fi
    cat "$log"
    if grep -q '\.\.\. ignored' "$log" || ! grep -q "test result: ok\. $count passed; 0 failed; 0 ignored" "$log"; then
        echo "Erreur : $filter : $count essai(s) attendu(s), tous passés" >&2
        exit 1
    fi
done
if [ -n "$LIVRE" ]; then echo "Build livré : tout passe"; else echo "Parité : tout passe"; fi
