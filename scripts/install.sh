#!/bin/sh
# Installe le lecteur au terminal : tout ce que package.sh assemble dans dist/
# (le binaire, ONNX Runtime, dysflow-espeak et ses données, les licences) va
# dans $PREFIX/lib/dysflow, et $PREFIX/bin/dysflow est un lien vers
# ../lib/dysflow/dysflow. Le binaire résout ce lien pour trouver ses voisins.
# Les modèles et la configuration vivent dans le répertoire de données.
#
#   ./scripts/install.sh                          # ~/.local, sans droits
#   PREFIX=/usr/local sudo -E ./scripts/install.sh
#   FEATURES=kokoro-onnx ./scripts/install.sh     # sans reconnaissance de texte
#   REMPLACER=1 ./scripts/install.sh              # remplace sans question la commande de l'app
#
# Pour désinstaller : rm -r "$PREFIX/lib/dysflow" "$PREFIX/bin/dysflow"
set -eu
cd "$(dirname "$0")/.."

PREFIX="${PREFIX:-$HOME/.local}"
FEATURES="${FEATURES:-ocr,kokoro-onnx}"
export FEATURES
BIN="$PREFIX/bin/dysflow"
LIB="$PREFIX/lib/dysflow"

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

# Depuis la racine, sans les variables qui désignent ORT ou espeak ailleurs :
# le binaire ne dépend que de ce qui est installé.
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

case ":$PATH:" in
    *":$PREFIX/bin:"*) ;;
    *)
        echo "Ajoutez $PREFIX/bin à votre PATH, par exemple dans ~/.zshrc :"
        echo "  export PATH=\"$PREFIX/bin:\$PATH\""
        ;;
esac
