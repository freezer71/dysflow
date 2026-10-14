#!/bin/sh
# Installe la commande `sillon` qui ouvre l'app (scripts/sillon-app.sh) à la
# place du lecteur au terminal.
#
#   ./scripts/install-app-command.sh              # ~/.local/bin
#   PREFIX=/usr/local sudo -E ./scripts/install-app-command.sh
set -e
cd "$(dirname "$0")/.."

PREFIX="${PREFIX:-$HOME/.local}"
mkdir -p "$PREFIX/bin"
install -m 755 scripts/sillon-app.sh "$PREFIX/bin/sillon"
echo "Installé : $PREFIX/bin/sillon (ouvre /Applications/Sillon.app)"
