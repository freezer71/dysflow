#!/bin/sh
# La parité des ports MLX (procédure de port, P2 et P3) : les essais ignorés
# `<module>::parity` comparent le Rust à la fixture figée par le Swift, étage
# par étage, jetons et texte compris, sans tolérance qui cache un écart.
#
# Ils tournent dans un build à part, avec les noyaux Metal du Swift : mlx-sys
# aux noyaux JIT (`--features mlx,mlx-rs/metal-jit`, dans target/mlx-parite),
# comme mlx-swift, et pour les noyaux que MLX ne compile pas à l'exécution, le
# `default.metallib` qu'Xcode compile pour mlx-swift (même commit de MLX).
# Avec le mlx.metallib de mlx-sys (CMake), la Conv2d par canal
# (`depthwise_conv_2d`) et `layer_norm` arrondissent autrement d'une ulp, et
# un décodage glouton finit par changer de jeton (tâche 14). Ce build ne sert
# qu'à prouver la traduction : le produit garde le mlx.metallib de mlx-sys
# (le choix des noyaux livrés est celui de la tâche 26).
#
#   DYSFLOW_MODELS=/chemin/des/modeles ./scripts/mlx-parite.sh [FILTRE NOMBRE]…
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

if [ "$#" -eq 0 ]; then
    set -- stt::mlx::parakeet::tests::parity 2
fi

# Les sources épinglées de MLX, puis un target et des noyaux à part : le build
# précompilé du dépôt (target/, target/mlx-metallib) n'est jamais touché.
# shellcheck source=scripts/mlx-sources.sh
. scripts/mlx-sources.sh
export CARGO_TARGET_DIR="$PWD/target/mlx-parite"
export MLX_RS_METAL_PATH="$CARGO_TARGET_DIR/mlx-metallib"
export DYSFLOW_MLX_METALLIB="$MLX_RS_METAL_PATH/mlx.metallib"
mkdir -p "$MLX_RS_METAL_PATH"
swift_metallib=${DYSFLOW_SWIFT_METALLIB:-$PWD/macos/build/derived/Build/Products/Debug/mlx-swift_Cmlx.bundle/Contents/Resources/default.metallib}
if [ ! -f "$swift_metallib" ]; then
    echo "Erreur : noyaux du Swift introuvables : $swift_metallib (./scripts/test-macos.sh les construit)" >&2
    exit 1
fi

FEATURES=mlx,mlx-rs/metal-jit
cargo test --release -p dysflow --features "$FEATURES" --lib --no-run -j "${CARGO_BUILD_JOBS:-4}"
# Lu à l'exécution par compute::mlx (metallib.rs), à la place de celui de mlx-sys.
export DYSFLOW_MLX_METALLIB="$swift_metallib"

log="$CARGO_TARGET_DIR/parite.log"
while [ "$#" -ge 2 ]; do
    filter=$1 count=$2
    shift 2
    if ! cargo test --release -p dysflow --features "$FEATURES" --lib -j "${CARGO_BUILD_JOBS:-4}" \
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
echo "Parité : tout passe"
