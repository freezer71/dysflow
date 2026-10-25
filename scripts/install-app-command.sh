#!/bin/sh
# Installe la commande `dysflow` qui ouvre l'app (scripts/dysflow-app.sh) à la
# place du lecteur au terminal.
#
#   ./scripts/install-app-command.sh              # ~/.local/bin
#   PREFIX=/usr/local sudo -E ./scripts/install-app-command.sh
#   REMPLACER=1 ./scripts/install-app-command.sh  # remplace sans question le lecteur au terminal
#
# Pour désinstaller : rm "$PREFIX/bin/dysflow"
set -eu
cd "$(dirname "$0")/.."

PREFIX="${PREFIX:-$HOME/.local}"
BIN="$PREFIX/bin/dysflow"

# bin/dysflow est-il le lecteur au terminal (install.sh : un lien vers
# ../lib/dysflow/dysflow, ou le binaire lui-même pour une installation plus
# ancienne) ? Il est remplacé seulement après avertissement.
terminal=""
if [ -L "$BIN" ]; then
    case "$(readlink "$BIN")" in
    *lib/dysflow/dysflow) terminal=1 ;;
    esac
elif [ -f "$BIN" ] && ! head -c 200 "$BIN" | grep -q "ouvre l'app"; then
    terminal=1
fi
if [ -n "$terminal" ]; then
    echo "Attention : $BIN est le lecteur au terminal (install.sh)." >&2
    echo "La commande de l'app le remplace ; $PREFIX/lib/dysflow reste en place (rm -r pour le retirer)." >&2
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
    # `install` écrirait à travers le lien, dans lib/dysflow/dysflow.
    rm -f "$BIN"
fi

mkdir -p "$PREFIX/bin"
install -m 755 scripts/dysflow-app.sh "$BIN"
echo "Installé : $BIN (ouvre /Applications/Dysflow.app)"
