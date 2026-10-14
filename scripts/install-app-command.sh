#!/bin/sh
# Installe la commande `sillon` qui ouvre l'app (scripts/sillon-app.sh) à la
# place du lecteur au terminal.
#
#   ./scripts/install-app-command.sh              # ~/.local/bin
#   PREFIX=/usr/local sudo -E ./scripts/install-app-command.sh
#   REMPLACER=1 ./scripts/install-app-command.sh  # remplace sans question le lecteur au terminal
#
# Pour désinstaller : rm "$PREFIX/bin/sillon"
set -eu
cd "$(dirname "$0")/.."

PREFIX="${PREFIX:-$HOME/.local}"
BIN="$PREFIX/bin/sillon"

# bin/sillon est-il le lecteur au terminal (install.sh : un lien vers
# ../lib/sillon/sillon, ou le binaire lui-même pour une installation plus
# ancienne) ? Il est remplacé seulement après avertissement.
terminal=""
if [ -L "$BIN" ]; then
    case "$(readlink "$BIN")" in
    *lib/sillon/sillon) terminal=1 ;;
    esac
elif [ -f "$BIN" ] && ! head -c 200 "$BIN" | grep -q "ouvre l'app"; then
    terminal=1
fi
if [ -n "$terminal" ]; then
    echo "Attention : $BIN est le lecteur au terminal (install.sh)." >&2
    echo "La commande de l'app le remplace ; $PREFIX/lib/sillon reste en place (rm -r pour le retirer)." >&2
    if [ "${REMPLACER:-}" != 1 ]; then
        answer=""
        if [ -t 0 ]; then
            printf 'Remplacer ? [o/N] ' >&2
            read -r answer || answer=""
        fi
        case "$answer" in
        o | O | oui | Oui) ;;
        *)
            echo "Rien n'est installé (REMPLACER=1 pour remplacer sans question)." >&2
            exit 1
            ;;
        esac
    fi
    # `install` écrirait à travers le lien, dans lib/sillon/sillon.
    rm -f "$BIN"
fi

mkdir -p "$PREFIX/bin"
install -m 755 scripts/sillon-app.sh "$BIN"
echo "Installé : $BIN (ouvre /Applications/Sillon.app)"
