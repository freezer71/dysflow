#!/bin/sh
# Installe le lecteur au terminal : tout ce que package.sh assemble dans dist/
# (le binaire, ONNX Runtime, dysflow-espeak et ses données, la bibliothèque C
# de sherpa-onnx et Silero VAD pour la dictée, les licences) va
# dans $PREFIX/lib/dysflow, et $PREFIX/bin/dysflow est un lien vers
# ../lib/dysflow/dysflow. Le binaire résout ce lien pour trouver ses voisins.
# Les modèles et la configuration vivent dans le répertoire de données.
#
# Par défaut, le terminal seul. Avec DYSFLOW_DESKTOP=1 (Linux), l'app de bureau
# (dysflow-app, GPUI) est construite et installée avec lui, et
# $PREFIX/bin/dysflow-app est un lien vers ../lib/dysflow/dysflow-app. Elle
# demande à la compilation libxkbcommon-dev, libxkbcommon-x11-dev,
# libxcb1-dev, libfontconfig-dev et libfreetype-dev (Debian, Ubuntu).
#
#   ./scripts/install.sh                          # ~/.local, sans droits
#   DYSFLOW_DESKTOP=1 ./scripts/install.sh        # avec l'app de bureau
#   PREFIX=/usr/local sudo -E ./scripts/install.sh
#   FEATURES=kokoro-onnx ./scripts/install.sh     # sans reconnaissance de texte ni dictée
#   REMPLACER=1 ./scripts/install.sh              # remplace sans question la commande de l'app
#
# Pour désinstaller :
#   rm -r "$PREFIX/lib/dysflow" "$PREFIX/bin/dysflow"; rm -f "$PREFIX/bin/dysflow-app"
set -eu
# Défense en profondeur : aucun ONNX Runtime lancé d'ici (le binaire du
# paquet : --version, --show-config, --check-kokoro) n'écrit ni n'envoie sa
# télémétrie (onnx::NO_TELEMETRY).
export ORT_DISABLE_TELEMETRY=1
cd "$(dirname "$0")/.."

# default_features : celles de package.sh (scripts/paquet-commun.sh).
# shellcheck source=scripts/paquet-commun.sh
. scripts/paquet-commun.sh

PREFIX="${PREFIX:-$HOME/.local}"
FEATURES="${FEATURES:-$(default_features)}"
export FEATURES
BIN="$PREFIX/bin/dysflow"
APP_BIN="$PREFIX/bin/dysflow-app"
LIB="$PREFIX/lib/dysflow"
# Lu par package.sh : 1 joint l'app de bureau au paquet.
DYSFLOW_DESKTOP="${DYSFLOW_DESKTOP:-}"
export DYSFLOW_DESKTOP

# bin/dysflow est-il la commande de l'app (install-app-command.sh) ? Elle est
# remplacée seulement après avertissement : le lecteur au terminal prend sa
# place.
if [ -f "$BIN" ] && [ ! -L "$BIN" ] && head -c 200 "$BIN" | grep -q "ouvre l'app"; then
    echo "Attention : $BIN est la commande qui ouvre l'app macOS (install-app-command.sh)." >&2
    echo "L'installer ici la remplace par le lecteur au terminal." >&2
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
fi

./scripts/package.sh

# Le dossier complet d'abord à côté, puis mis en place d'un coup : une copie
# interrompue ne laisse pas une installation à moitié remplacée.
mkdir -p "$PREFIX/lib" "$PREFIX/bin"
rm -rf "$LIB.nouveau"
cp -R dist "$LIB.nouveau"
rm -rf "$LIB"
mv "$LIB.nouveau" "$LIB"
ln -sfn ../lib/dysflow/dysflow "$BIN"
# L'app de bureau, si le paquet la porte (DYSFLOW_DESKTOP=1) : son lien comme
# celui du terminal. Sinon, un lien laissé par une installation précédente
# avec l'app ne pointe plus sur rien : il part.
if [ -x "$LIB/dysflow-app" ]; then
    ln -sfn ../lib/dysflow/dysflow-app "$APP_BIN"
elif [ -L "$APP_BIN" ] && [ "$(readlink "$APP_BIN")" = ../lib/dysflow/dysflow-app ]; then
    rm -f "$APP_BIN"
fi

# Depuis la racine, sans les variables qui désignent ORT ou espeak ailleurs :
# le binaire ne dépend que de ce qui est installé. Avec la dictée, qu'il
# démarre prouve aussi que la bibliothèque C de sherpa-onnx se charge.
echo "Vérification hors de l'arbre de compilation…"
( cd / && env -u DYSFLOW_ORT_LIB -u DYSFLOW_ESPEAK -u DYSFLOW_ESPEAK_DATA "$BIN" --show-config >/dev/null )
case ",$FEATURES," in
*,kokoro-onnx,*)
    if ls "$LIB"/libonnxruntime.* >/dev/null 2>&1; then
        ( cd / && env -u DYSFLOW_ORT_LIB -u DYSFLOW_ESPEAK -u DYSFLOW_ESPEAK_DATA "$BIN" --check-kokoro )
    else
        echo "ONNX Runtime non joint sur cette plateforme : Kokoro demandera DYSFLOW_ORT_LIB=<libonnxruntime>." >&2
    fi
    ;;
esac

echo "Installé : $BIN → $LIB/dysflow"
if [ -x "$LIB/dysflow-app" ]; then
    echo "Installé : $APP_BIN → $LIB/dysflow-app"
fi

case ":$PATH:" in
    *":$PREFIX/bin:"*) ;;
    *)
        echo "Ajoutez $PREFIX/bin à votre PATH, par exemple dans ~/.zshrc :"
        echo "  export PATH=\"$PREFIX/bin:\$PATH\""
        ;;
esac
