#!/bin/sh
# Assemble un dossier distribuable : le binaire et la bibliothèque qu'il charge.
#
# Le shim audio.cpp est une bibliothèque partagée — c'est un choix assumé, qui
# laisse CMake résoudre les dépendances transitives. Le binaire la cherche donc
# à côté de lui, puis dans un `lib/` voisin. Ce script met le premier en place.
set -e
cd "$(dirname "$0")/.."

FEATURES="${FEATURES:-audio-cpp,ocr}"
OUT="${OUT:-dist}"

echo "Compilation (features : $FEATURES)…"
cargo build --release --features "$FEATURES"

rm -rf "$OUT"
mkdir -p "$OUT"
cp target/release/reader "$OUT/"

# Le nom du répertoire de build porte une empreinte, il faut le retrouver —
# et chaque jeu de features laisse le sien : on prend le plus récent, sans
# quoi un shim d'un build antérieur pouvait être livré à la place de celui
# qu'on vient de compiler.
for ext in dylib so; do
    shim=$(find target/release/build -path "*out/lib/libaudiocpp_shim.$ext" -print0 2>/dev/null \
        | xargs -0 ls -t 2>/dev/null | head -1)
    [ -n "$shim" ] && cp "$shim" "$OUT/" && break
done

if [ -z "$shim" ]; then
    echo "Aucun shim trouvé : binaire compilé sans audio-cpp, rien à joindre." >&2
else
    echo "Joint : $(basename "$shim")"
fi

echo "Vérification hors de l'arbre de compilation…"
( cd "$OUT" && ./reader --show-config >/dev/null )
echo "Prêt : $OUT/"
ls -lh "$OUT"
