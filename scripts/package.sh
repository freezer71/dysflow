#!/bin/sh
# Assemble un dossier distribuable : le binaire et la bibliothèque qu'il charge.
#
# Le shim audio.cpp est une bibliothèque partagée — c'est un choix assumé, qui
# laisse CMake résoudre les dépendances transitives. Le binaire la cherche donc
# à côté de lui, puis dans un `lib/` voisin. Ce script met le premier en place.
set -e
cd "$(dirname "$0")/.."

# FEATURES=audio-cpp,ocr,gui joint aussi la fenêtre : sillon-gui, et sur macOS
# un bundle Sillon.app, sur Linux un sillon.desktop.
FEATURES="${FEATURES:-audio-cpp,ocr}"
OUT="${OUT:-dist}"

echo "Compilation (features : $FEATURES)…"
cargo build --release --features "$FEATURES"

rm -rf "$OUT"
mkdir -p "$OUT"
cp target/release/sillon "$OUT/"

# Le nom du répertoire de build porte une empreinte, il faut le retrouver —
# et chaque jeu de features laisse le sien : on prend le plus récent, sans
# quoi un shim d'un build antérieur pouvait être livré à la place de celui
# qu'on vient de compiler.
for ext in dylib so; do
    shim=$(find target/release/build -path "*out/lib/libaudiocpp_shim.$ext" -print0 2>/dev/null \
        | xargs -0 ls -t 2>/dev/null | head -1)
    [ -n "$shim" ] && cp "$shim" "$OUT/" && break
done

if [ -z "$shim" ]; then
    echo "Aucun shim trouvé : binaire compilé sans audio-cpp, rien à joindre." >&2
else
    echo "Joint : $(basename "$shim")"
fi

# La fenêtre suit quand elle a été compilée.
case ",$FEATURES," in
    *,gui,*)
        cp target/release/sillon-gui "$OUT/"
        case "$(uname -s)" in
            Darwin)
                # Un bundle minimal : le Dock le lance, un fichier déposé sur
                # son icône lui arrive. Le shim est cherché à côté du binaire.
                APP="$OUT/Sillon.app/Contents"
                mkdir -p "$APP/MacOS" "$APP/Resources"
                cp target/release/sillon-gui "$APP/MacOS/sillon-gui"
                for s in "$OUT"/libaudiocpp_shim.*; do
                    [ -f "$s" ] && cp "$s" "$APP/MacOS/"
                done
                # L'icône est versionnée toute faite : scripts/logo.sh la
                # rasterise, et lui seul demande macOS.
                cp assets/logo/sillon.icns "$APP/Resources/"
                VERSION=$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')
                cat > "$APP/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>Sillon</string>
  <key>CFBundleDisplayName</key><string>Sillon</string>
  <key>CFBundleIdentifier</key><string>org.koumalabs.sillon</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleExecutable</key><string>sillon-gui</string>
  <key>CFBundleIconFile</key><string>sillon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>CFBundleDocumentTypes</key><array><dict>
    <key>CFBundleTypeName</key><string>Document</string>
    <key>CFBundleTypeRole</key><string>Viewer</string>
    <key>LSItemContentTypes</key><array>
      <string>net.daringfireball.markdown</string>
      <string>public.plain-text</string>
      <string>com.adobe.pdf</string>
      <string>public.image</string>
    </array>
  </dict></array>
</dict></plist>
PLIST
                echo "Joint : Sillon.app"
                ;;
            Linux)
                cat > "$OUT/sillon.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=Sillon
Comment=Lit un document à voix haute, à suivre des yeux
Exec=sillon-gui %f
Icon=sillon
Terminal=false
Categories=Office;Audio;Accessibility;
MimeType=text/markdown;text/plain;application/pdf;image/png;image/jpeg;
DESKTOP
                # Le thème d'icônes la cherche par son nom : à poser dans
                # ~/.local/share/icons/hicolor/512x512/apps/sillon.png.
                cp assets/logo/sillon.png "$OUT/"
                echo "Joint : sillon.desktop et sillon.png"
                ;;
        esac
        ;;
esac

echo "Vérification hors de l'arbre de compilation…"
( cd "$OUT" && ./sillon --show-config >/dev/null )
if [ -f "$OUT/sillon-gui" ]; then
    ( cd "$OUT" && ./sillon-gui --show-config >/dev/null )
fi
echo "Prêt : $OUT/"
ls -lh "$OUT"
