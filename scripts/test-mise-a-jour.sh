#!/bin/sh
# La mise à jour d'un vrai poste Linux : install.sh (celui de la racine, que
# lance `curl … | sh` et `dysflow --update`) installe la dernière version
# publiée sur GitHub, puis met à jour vers le paquet DIST (celui de
# package.sh), servi en local comme GitHub sert une publication : archive
# dysflow-linux-<arch>.tar.gz et son empreinte. Ensuite, depuis /, --version
# et --check-kokoro passent, et le dossier installé est exactement DIST.
#
#   ./scripts/test-mise-a-jour.sh dist
#
# Pour la CI (Linux) : télécharge la dernière publication, installe sous un
# PREFIX et un HOME provisoires (jamais ~/.local), et lance un serveur HTTP
# local le temps de la mise à jour, arrêté à la sortie.
set -eu
# Défense en profondeur : aucun ONNX Runtime lancé d'ici n'écrit ni n'envoie
# sa télémétrie (onnx::NO_TELEMETRY).
export ORT_DISABLE_TELEMETRY=1

if [ "$#" -ne 1 ] || [ ! -x "$1/dysflow" ]; then
    echo "Usage : $0 DIST (le dossier de package.sh, avec son binaire dysflow)" >&2
    exit 2
fi
dist=$(cd "$1" && pwd)
cd "$(dirname "$0")/.."
[ "$(uname -s)" = Linux ] || { echo "Erreur : ce test est celui de Linux (install.sh pose l'app sur Mac)" >&2; exit 1; }

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
arch=$(uname -m)
tmp="${RUNNER_TEMP:-$PWD/target}/mise-a-jour"
rm -rf "$tmp"
mkdir -p "$tmp/home"
prefix="$tmp/prefix"

# installe [VARIABLE=VALEUR…] [OPTION…] : install.sh de la racine, HOME et
# PREFIX provisoires, en français (ses messages disent « Mise à jour : … →
# … »). Les VARIABLE=VALEUR d'abord, les options ensuite.
installe() {
    set -- env HOME="$tmp/home" XDG_DATA_HOME="$tmp/home/data" XDG_CONFIG_HOME="$tmp/home/config" \
        XDG_CACHE_HOME="$tmp/home/cache" PREFIX="$prefix" LANG=fr_FR.UTF-8 "$@"
    "$@"
}

# 1. La dernière version publiée, depuis GitHub.
installe sh install.sh
avant=$("$prefix/bin/dysflow" --version)
echo "Installé depuis GitHub : $avant"

# 2. Ce paquet, publié comme release.yml le publie, sur un serveur local.
publication="$tmp/srv/releases/download/v$version"
mkdir -p "$publication"
dossier="dysflow-$version-linux-$arch"
cp -R "$dist" "$tmp/$dossier"
( cd "$tmp" && tar -czf "$publication/dysflow-linux-$arch.tar.gz" "$dossier" )
( cd "$publication" && sha256sum "dysflow-linux-$arch.tar.gz" > "dysflow-linux-$arch.tar.gz.sha256" )
port=8765
python3 -I -m http.server "$port" --bind 127.0.0.1 --directory "$tmp/srv" >/dev/null 2>&1 &
serveur=$!
trap 'kill "$serveur" 2>/dev/null || true' EXIT INT TERM
essais=0
until curl -fsS -o /dev/null "http://127.0.0.1:$port/releases/download/v$version/dysflow-linux-$arch.tar.gz.sha256"; do
    essais=$((essais + 1))
    if [ "$essais" -ge 50 ]; then
        echo "Erreur : le serveur local ne répond pas" >&2
        exit 1
    fi
    sleep 0.2
done

# --force : la dernière publication peut porter le même numéro que ce
# paquet ; la mise à jour remplace alors quand même le dossier.
if ! installe DYSFLOW_DEPOT="http://127.0.0.1:$port" DYSFLOW_VERSION="$version" sh install.sh --force \
    > "$tmp/mise-a-jour.log" 2>&1; then
    cat "$tmp/mise-a-jour.log"
    echo "Erreur : la mise à jour a échoué" >&2
    exit 1
fi
cat "$tmp/mise-a-jour.log"
kill "$serveur" 2>/dev/null || true

# 3. Le binaire mis à jour, depuis /, sans variable qui désigne ORT ou espeak
# ailleurs.
lance() {
    ( cd / && env -u DYSFLOW_ORT_LIB -u DYSFLOW_ESPEAK -u DYSFLOW_ESPEAK_DATA HOME="$tmp/home" "$prefix/bin/dysflow" "$@" )
}
apres=$(lance --version)
echo "Après la mise à jour : $apres (avant : $avant)"
[ "$apres" = "dysflow $version" ] || { echo "Erreur : version $apres, attendu dysflow $version" >&2; exit 1; }
lance --check-kokoro
# Le dossier installé est le paquet entier, dictée comprise, et rien d'autre.
( cd "$dist" && find . | LC_ALL=C sort ) > "$tmp/attendu.txt"
( cd "$prefix/lib/dysflow" && find . | LC_ALL=C sort ) > "$tmp/installe.txt"
if ! diff "$tmp/attendu.txt" "$tmp/installe.txt"; then
    echo "Erreur : le dossier installé n'est pas le paquet" >&2
    exit 1
fi
for fichier in silero_vad.onnx libsherpa-onnx-c-api.so libonnxruntime.so; do
    [ -f "$prefix/lib/dysflow/$fichier" ] || { echo "Erreur : $fichier absent après la mise à jour" >&2; exit 1; }
done
echo "Mise à jour : $avant → $apres, --version et --check-kokoro passent"
