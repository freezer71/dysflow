#!/bin/sh
# Les modèles épinglés des essais (scripts/modeles-epingles.txt) dans DOSSIER :
# chaque fichier vérifié par son SHA-256, téléchargé de son adresse épinglée
# seulement s'il manque ou n'a pas la bonne empreinte. Affiche les octets
# téléchargés (le coût d'un cache froid).
#
#   ./scripts/modeles-epingles.sh DOSSIER
#
# DOSSIER est le cache des modèles de la CI (un par système, sous la clé de
# scripts/modeles-epingles.txt), partagé par la porte (ort-gate.sh, MODELS) et
# le bout en bout (paquet-bout-en-bout.sh).
set -eu

if [ "$#" -ne 1 ]; then
    echo "Usage : $0 DOSSIER" >&2
    exit 2
fi
mkdir -p "$1"
dossier=$(cd "$1" && pwd)
cd "$(dirname "$0")/.."
# shellcheck source=scripts/paquet-commun.sh
. scripts/paquet-commun.sh

octets=0
for chemin in $(pinned_paths); do
    fichier="$dossier/$chemin"
    if ! { [ -f "$fichier" ] && sha256_ok "$(pin_sha256 "$chemin")" "$fichier"; }; then
        fetch_pinned "$chemin" "$fichier"
        octets=$((octets + $(wc -c < "$fichier")))
    fi
done
echo "Modèles épinglés : tous vérifiés ($dossier) ; téléchargés à l'instant : $octets octets"
