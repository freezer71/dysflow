#!/usr/bin/env bash
# Emballe Dysflow.app pour la publication, dans dist/ :
#
#   dysflow-macos-arm64.zip   pour install.sh (curl | sh)
#   dysflow-macos.dmg         à télécharger : glisser l'app dans Applications
#
# chacun avec son empreinte .sha256. Les noms ne portent pas la version :
# /releases/latest/download/<nom> reste la même adresse d'une version à
# l'autre. L'app doit être construite avant (./scripts/build-macos.sh).
#
# Notarisation, si NOTARY_PROFILE (un profil du trousseau, enregistré par
# `xcrun notarytool store-credentials`) ou NOTARY_KEY_PATH (la clé .p8
# d'App Store Connect), NOTARY_KEY_ID et NOTARY_ISSUER sont donnés : l'app
# puis le .dmg partent chez Apple, et le feu vert leur est agrafé. Ils s'ouvrent alors sans
# alerte, même hors ligne. Le .dmg est signé de DYSFLOW_SIGNATURE.
#
#   ./scripts/package-macos.sh [chemin/Dysflow.app]
set -euo pipefail
cd "$(dirname "$0")/.."

APP=${1:-macos/build/Release/Dysflow.app}
[ -d "$APP" ] || { echo "Erreur : $APP introuvable (./scripts/build-macos.sh)" >&2; exit 1; }

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
app_version=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP/Contents/Info.plist")
if [ "$version" != "$app_version" ]; then
    echo "Erreur : l'app est en $app_version, Cargo.toml en $version (MARKETING_VERSION de macos/project.yml)" >&2
    exit 1
fi
codesign --verify --deep --strict "$APP"

# Envoie un fichier à la notarisation, attend la réponse ; en cas de refus,
# affiche le journal d'Apple (ce qui manque, fichier par fichier).
notariser() {
    local sortie statut id
    sortie=$(xcrun notarytool submit "$1" "${acces[@]}" --wait --timeout 30m --output-format json)
    statut=$(printf '%s' "$sortie" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("status",""))')
    id=$(printf '%s' "$sortie" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("id",""))')
    echo "Notarisation de $(basename "$1") : $statut ($id)"
    if [ "$statut" != "Accepted" ]; then
        [ -n "$id" ] && xcrun notarytool log "$id" "${acces[@]}" >&2
        echo "Erreur : notarisation refusée pour $1" >&2
        exit 1
    fi
}
notarise=false
if [ -n "${NOTARY_PROFILE:-}" ]; then
    acces=(--keychain-profile "$NOTARY_PROFILE")
    notarise=true
elif [ -n "${NOTARY_KEY_PATH:-}" ] && [ -n "${NOTARY_KEY_ID:-}" ] && [ -n "${NOTARY_ISSUER:-}" ]; then
    acces=(--key "$NOTARY_KEY_PATH" --key-id "$NOTARY_KEY_ID" --issuer "$NOTARY_ISSUER")
    notarise=true
fi

mkdir -p dist
zip="dist/dysflow-macos-arm64.zip"
dmg="dist/dysflow-macos.dmg"
rm -f "$zip" "$dmg" "$zip.sha256" "$dmg.sha256"

# L'app d'abord : son feu vert agrafé voyage dans le .zip et le .dmg.
if $notarise; then
    envoi=$(mktemp -d)
    ditto -c -k --keepParent "$APP" "$envoi/Dysflow.zip"
    notariser "$envoi/Dysflow.zip"
    rm -rf "$envoi"
    xcrun stapler staple "$APP"
fi

# ditto garde les liens symboliques et les signatures des frameworks.
ditto -c -k --keepParent "$APP" "$zip"

# Le .dmg : la fenêtre dessinée (assets/dmg, scripts/dmg/fond.py), l'app à
# gauche, Applications à droite, une flèche entre les deux. dmgbuild écrit
# la mise en page sans piloter le Finder : elle tient aussi sur la CI.
# Sur la CI, dmgbuild est déjà installé, empreintes vérifiées
# (scripts/dmg/requirements.txt). Ici, on l'installe de la même façon.
if command -v dmgbuild >/dev/null; then
    dmgbuild=(dmgbuild)
else
    outils=$(mktemp -d)
    uv venv -q "$outils"
    VIRTUAL_ENV="$outils" uv pip install -q --require-hashes -r scripts/dmg/requirements.txt
    dmgbuild=("$outils/bin/dmgbuild")
fi
"${dmgbuild[@]}" -s scripts/dmg/reglages.py -D app="$APP" -D fond=assets/dmg/fond.png \
    -D icone=assets/logo/dysflow.icns "Dysflow $version" "$dmg" >/dev/null
if [ -n "${DYSFLOW_SIGNATURE:-}" ]; then
    codesign --force --timestamp --sign "$DYSFLOW_SIGNATURE" "$dmg"
fi
if $notarise; then
    notariser "$dmg"
    xcrun stapler staple "$dmg"
    spctl --assess --type open --context context:primary-signature -v "$dmg"
fi

for f in "$zip" "$dmg"; do
    (cd dist && shasum -a 256 "$(basename "$f")" >"$(basename "$f").sha256")
    echo "$(du -h "$f" | cut -f1)	$f"
done
