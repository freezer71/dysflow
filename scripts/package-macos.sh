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

mkdir -p dist
zip="dist/dysflow-macos-arm64.zip"
dmg="dist/dysflow-macos.dmg"
rm -f "$zip" "$dmg" "$zip.sha256" "$dmg.sha256"

# ditto garde les liens symboliques et les signatures des frameworks.
ditto -c -k --keepParent "$APP" "$zip"

# Le .dmg : l'app et un raccourci vers Applications, côte à côte.
fond=$(mktemp -d)
trap 'rm -rf "$fond"' EXIT
ditto "$APP" "$fond/Dysflow.app"
ln -s /Applications "$fond/Applications"
hdiutil create -quiet -volname "dysflow $version" -srcfolder "$fond" -fs HFS+ -format UDZO -ov "$dmg"

for f in "$zip" "$dmg"; do
    (cd dist && shasum -a 256 "$(basename "$f")" >"$(basename "$f").sha256")
    echo "$(du -h "$f" | cut -f1)	$f"
done
