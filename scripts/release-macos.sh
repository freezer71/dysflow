#!/usr/bin/env bash
# Publie une version depuis le Mac : l'app s'y construit en quelques minutes,
# contre 26 sur la CI. Linux reste sur la CI (4 min).
#
#   1. pousse le tag v<version de Cargo.toml> : la CI construit Linux et crée
#      la publication, pas encore « dernière » ;
#   2. construit l'app, la signe Developer ID, la notarise (app et .dmg) ;
#   3. attend la CI, joint le .zip et le .dmg, marque la publication
#      « dernière » : /releases/latest passe d'un coup à la version complète.
#
# Une fois, sur ce Mac, la clé de notarisation dans le trousseau (la même que
# le secret NOTARY_KEY de GitHub, ou un mot de passe d'app de l'Apple ID) :
#
#   xcrun notarytool store-credentials dysflow
#
#   ./scripts/release-macos.sh
set -euo pipefail
cd "$(dirname "$0")/.."

PROFIL=${NOTARY_PROFILE:-dysflow}
DEPOT=freezer71/dysflow
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
tag="v$version"

echec() { echo "Erreur : $*" >&2; exit 1; }

# Tout ce qui peut manquer, avant de pousser quoi que ce soit.
app=$(sed -n 's/^ *MARKETING_VERSION: *//p' macos/project.yml | head -1)
[ "$app" = "$version" ] || echec "Cargo.toml est en $version, macos/project.yml en $app"
[ "$(git branch --show-current)" = main ] || echec "pas sur main"
[ -z "$(git status --porcelain)" ] || echec "des changements ne sont pas validés"
git fetch -q origin main --tags
[ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] || echec "main n'est pas à jour avec origin/main (git push)"
signature=$(security find-identity -v -p codesigning | sed -n 's/.*"\(Developer ID Application: [^"]*\)".*/\1/p' | head -1)
[ -n "$signature" ] || echec "aucun certificat Developer ID Application dans le trousseau"
xcrun notarytool history --keychain-profile "$PROFIL" >/dev/null 2>&1 \
    || echec "pas de profil de notarisation « $PROFIL » (xcrun notarytool store-credentials $PROFIL)"

# 1. Le tag ; s'il est déjà poussé (une reprise), on garde le sien.
if git ls-remote --exit-code --tags origin "refs/tags/$tag" >/dev/null; then
    [ "$(git rev-parse "$tag^{commit}")" = "$(git rev-parse HEAD)" ] || echec "$tag existe déjà, sur un autre commit"
    echo "$tag déjà poussé : reprise"
else
    git tag -a "$tag" -m "Dysflow $version"
    git push -q origin "$tag"
    echo "$tag poussé : la CI construit Linux"
fi

# 2. L'app, pendant que la CI travaille.
DYSFLOW_SIGNATURE=$signature ./scripts/build-macos.sh
DYSFLOW_SIGNATURE=$signature NOTARY_PROFILE=$PROFIL ./scripts/package-macos.sh
spctl --assess --type execute -v macos/build/Release/Dysflow.app

# 3. La CI, puis l'app jointe.
run=""
for _ in $(seq 60); do
    run=$(gh run list --repo "$DEPOT" --workflow release.yml --branch "$tag" --limit 1 --json databaseId --jq '.[0].databaseId // empty')
    [ -n "$run" ] && break
    sleep 5
done
[ -n "$run" ] || echec "la publication de $tag n'a pas démarré sur la CI"
echo "Attente de la CI (Linux) : https://github.com/$DEPOT/actions/runs/$run"
gh run watch "$run" --repo "$DEPOT" --exit-status --interval 20 >/dev/null || echec "la CI a échoué : gh run view $run --repo $DEPOT --log-failed"
gh release upload "$tag" --repo "$DEPOT" --clobber \
    dist/dysflow-macos-arm64.zip dist/dysflow-macos-arm64.zip.sha256 \
    dist/dysflow-macos.dmg dist/dysflow-macos.dmg.sha256
gh release edit "$tag" --repo "$DEPOT" --latest
echo "Publiée : https://github.com/$DEPOT/releases/tag/$tag"
