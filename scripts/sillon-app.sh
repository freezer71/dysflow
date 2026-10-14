#!/bin/sh
# La commande `sillon` du Mac : ouvre l'app, pas le terminal.
#
#   sillon                 # la page de notes
#   sillon document.pdf    # le panneau, sur ce document (texte, PDF, image)
#
# Installée par ./scripts/install-app-command.sh dans ~/.local/bin/sillon.
set -e

# Le chemin, pas l'identifiant : `open -b` peut choisir une copie de
# développement (macos/build/…) au lieu de l'app installée.
APP="${SILLON_APP:-/Applications/Sillon.app}"
if [ ! -d "$APP" ]; then
    echo "sillon : $APP introuvable (./scripts/build-macos.sh, puis copie dans /Applications)" >&2
    exit 1
fi

if [ $# -eq 0 ]; then
    exec open -a "$APP"
fi

for file in "$@"; do
    if [ ! -e "$file" ]; then
        echo "sillon : « $file » introuvable" >&2
        exit 1
    fi
done
# Un seul document à la fois : l'app lit le dernier donné.
exec open -a "$APP" -- "$@"
