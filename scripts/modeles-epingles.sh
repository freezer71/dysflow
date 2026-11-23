#!/bin/sh
# Les modèles épinglés des essais (scripts/modeles-epingles.txt) dans DOSSIER :
# chaque fichier vérifié par son SHA-256, téléchargé de son adresse épinglée
# seulement s'il manque ou n'a pas la bonne empreinte. Affiche les octets
# téléchargés (le coût d'un cache froid).
#
#   ./scripts/modeles-epingles.sh DOSSIER [PRÉFIXE…]
#
# Avec des PRÉFIXES, seulement les chemins qui commencent par l'un d'eux
# (`silero_vad.onnx kokoro/` : le cache de la porte en CI). DOSSIER est le
# dossier des modèles des essais (MODELS en CI : le cache de la porte, Silero
# et Kokoro, et celui de Parakeet), lu par la porte (ort-gate.sh) et le bout
# en bout (paquet-bout-en-bout.sh).
set -eu

if [ "$#" -lt 1 ]; then
    echo "Usage : $0 DOSSIER [PRÉFIXE…]" >&2
    exit 2
fi
mkdir -p "$1"
dossier=$(cd "$1" && pwd)
shift
cd "$(dirname "$0")/.."
# shellcheck source=scripts/paquet-commun.sh
. scripts/paquet-commun.sh

octets=0
for chemin in $(pinned_paths); do
    if [ "$#" -gt 0 ]; then
        retenu=""
        for prefixe in "$@"; do
            case "$chemin" in "$prefixe"*) retenu=1 ;; esac
        done
        [ -n "$retenu" ] || continue
    fi
    fichier="$dossier/$chemin"
    if ! { [ -f "$fichier" ] && sha256_ok "$(pin_sha256 "$chemin")" "$fichier"; }; then
        fetch_pinned "$chemin" "$fichier"
        octets=$((octets + $(wc -c < "$fichier")))
    fi
done
echo "Modèles épinglés : tous vérifiés ($dossier) ; téléchargés à l'instant : $octets octets"
