#!/bin/sh
# Installe le lecteur : `sillon` dans $PREFIX/bin. Les modèles et la
# configuration vivent déjà dans le répertoire de données.
#
#   ./scripts/install.sh                          # ~/.local, sans droits
#   PREFIX=/usr/local sudo -E ./scripts/install.sh
#   FEATURES=kokoro-onnx ./scripts/install.sh     # sans reconnaissance de texte
#
# Pour désinstaller : rm $PREFIX/bin/sillon
set -e
cd "$(dirname "$0")/.."

PREFIX="${PREFIX:-$HOME/.local}"

./scripts/package.sh

mkdir -p "$PREFIX/bin"
install -m 755 dist/sillon "$PREFIX/bin/sillon"

# Depuis la racine : le binaire ne dépend pas du dossier où il a été construit.
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
