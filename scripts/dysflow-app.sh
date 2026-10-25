#!/bin/sh
# La commande `dysflow` du Mac : ouvre l'app, pas le terminal.
#
#   dysflow                 # le lecteur
#   dysflow document.pdf    # le panneau, sur ce document (texte, PDF, image)
#   dysflow --update        # installe la dernière version publiée
#
# Installée par ./scripts/install-app-command.sh dans ~/.local/bin/dysflow.
set -eu

# Le chemin, pas l'identifiant : `open -b` peut choisir une copie de
# développement (macos/build/…) au lieu de l'app installée.
APP="${DYSFLOW_APP:-/Applications/Dysflow.app}"
# dysflow --update : la dernière version publiée, par l'installateur officiel.
if [ "${1:-}" = --update ]; then
    curl -fsSL https://sillage.koumalabs.org/install.sh | sh
    exit
fi
if [ ! -d "$APP" ]; then
    echo "dysflow : $APP introuvable (./scripts/build-macos.sh, puis copie dans /Applications)" >&2
    exit 1
fi

if [ $# -eq 0 ]; then
    exec open -a "$APP"
fi

for file in "$@"; do
    if [ ! -e "$file" ]; then
        echo "dysflow : « $file » introuvable" >&2
        exit 1
    fi
done
# Un seul document à la fois : l'app lit le dernier donné.
exec open -a "$APP" -- "$@"
