#!/bin/sh
# Rasterise le logo : l'icône macOS (.icns) et le PNG que Linux affiche.
#
# Le dessin vit en SVG dans assets/logo (scripts/logo_gen.py le calcule) ;
# ce script n'en tire que les images dont les systèmes ont besoin. Elles sont
# versionnées, pour que `package.sh` n'exige ni macOS ni outil de rendu.
#
#   ./scripts/logo.sh
#
# À relancer seulement quand le dessin change. Demande macOS : sips rasterise
# le SVG, iconutil assemble l'icône.
#
# Le rendu passe par sips et non par Quick Look : ce dernier aplatit l'image
# sur du blanc, ce qui cerclait l'icône d'un carré opaque dans le Dock.
#
# Sous 32 points, le trait fin se bouche : les petites tuiles
# reçoivent la marque au trait gras. C'est le même logo, redessiné pour tenir.
set -eu
cd "$(dirname "$0")/.."

for outil in sips iconutil; do
    command -v "$outil" >/dev/null 2>&1 || {
        echo "$outil manquant : ce script demande macOS." >&2
        exit 1
    }
done

LOGO=assets/logo
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# Un seul rendu par variante, en 1024 : les tuiles en descendent. Réduire une
# grande image lisse mieux les traits fins que rastériser droit en 16 pixels.
for variante in grande:sillage-app petite:sillage-app-small; do
    sips -s format png -z 1024 1024 "$LOGO/${variante#*:}.svg" \
        --out "$TMP/${variante%%:*}.png" >/dev/null
done

tuile() {                        # tuile <variante.png> <taille> <nom>
    cp "$1" "$TMP/icon.iconset/$3"
    sips -z "$2" "$2" "$TMP/icon.iconset/$3" >/dev/null
}

mkdir -p "$TMP/icon.iconset"
# Ce que l'œil voit fait 16 ou 32 points, @2x ou non : la petite marque suit
# le point, pas le pixel.
tuile "$TMP/petite.png"   16 icon_16x16.png
tuile "$TMP/petite.png"   32 icon_16x16@2x.png
tuile "$TMP/petite.png"   32 icon_32x32.png
tuile "$TMP/petite.png"   64 icon_32x32@2x.png
tuile "$TMP/grande.png"  128 icon_128x128.png
tuile "$TMP/grande.png"  256 icon_128x128@2x.png
tuile "$TMP/grande.png"  256 icon_256x256.png
tuile "$TMP/grande.png"  512 icon_256x256@2x.png
tuile "$TMP/grande.png"  512 icon_512x512.png
tuile "$TMP/grande.png" 1024 icon_512x512@2x.png

iconutil -c icns "$TMP/icon.iconset" -o "$LOGO/sillage.icns"
echo "  $LOGO/sillage.icns"

# L'app macOS : les mêmes tuiles, en catalogue d'actifs (AppIcon).
SET=macos/Resources/Assets.xcassets/AppIcon.appiconset
mkdir -p "$SET"
cp "$TMP"/icon.iconset/*.png "$SET/"
{
    printf '{\n  "images" : [\n'
    sep=""
    for t in 16 32 128 256 512; do
        for e in 1 2; do
            if [ "$e" = 1 ]; then f="icon_${t}x${t}.png"; else f="icon_${t}x${t}@2x.png"; fi
            printf '%b    { "filename" : "%s", "idiom" : "mac", "scale" : "%sx", "size" : "%sx%s" }' \
                "$sep" "$f" "$e" "$t" "$t"
            sep=",\n"
        done
    done
    printf '\n  ],\n  "info" : { "author" : "xcode", "version" : 1 }\n}\n'
} > "$SET/Contents.json"
printf '{\n  "info" : { "author" : "xcode", "version" : 1 }\n}\n' \
    > macos/Resources/Assets.xcassets/Contents.json
echo "  $SET"

# Linux : un seul PNG, à la taille que réclament les thèmes d'icônes.
cp "$TMP/grande.png" "$LOGO/sillage.png"
sips -z 512 512 "$LOGO/sillage.png" >/dev/null
echo "  $LOGO/sillage.png"
