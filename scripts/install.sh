#!/bin/sh
# Installe le lecteur : `sillon` dans $PREFIX/bin, le shim audio.cpp dans
# $PREFIX/lib — c'est là que le binaire le cherche (build.rs pose les rpath
# `@executable_path/../lib` et `$ORIGIN/../lib`). Rien d'autre à copier : les
# modèles et la configuration vivent déjà dans le répertoire de données.
#
#   ./scripts/install.sh                          # ~/.local, sans droits
#   PREFIX=/usr/local sudo -E ./scripts/install.sh
#   FEATURES=audio-cpp ./scripts/install.sh       # sans reconnaissance de texte
#
# Pour désinstaller : rm $PREFIX/bin/sillon $PREFIX/lib/libaudiocpp_shim.*
set -e
cd "$(dirname "$0")/.."

PREFIX="${PREFIX:-$HOME/.local}"

./scripts/package.sh

mkdir -p "$PREFIX/bin" "$PREFIX/lib"
install -m 755 dist/sillon "$PREFIX/bin/sillon"
for shim in dist/libaudiocpp_shim.*; do
    [ -f "$shim" ] && install -m 755 "$shim" "$PREFIX/lib/"
done

# Depuis la racine : si le shim ne se trouve pas par le rpath, on le sait ici.
echo "Vérification hors de l'arbre de compilation…"
( cd / && "$PREFIX/bin/sillon" --show-config >/dev/null )

echo "Installé : $PREFIX/bin/sillon"
case ":$PATH:" in
    *":$PREFIX/bin:"*) ;;
    *)
        echo "Ajoutez $PREFIX/bin à votre PATH, par exemple dans ~/.zshrc :"
        echo "  export PATH=\"$PREFIX/bin:\$PATH\""
        ;;
esac
