#!/bin/sh
# Installe sillage en une commande, depuis la dernière publication GitHub :
#
#   curl -fsSL https://sillage.koumalabs.org/install.sh | sh
#
# Sur un Mac à puce Apple : l'app dans /Applications (ou ~/Applications), et la
# commande `sillage` qui l'ouvre. Sous Linux (x86_64, ARM64) : le lecteur au
# terminal dans ~/.local/lib/sillage, et la commande `sillage`. L'empreinte
# SHA-256 de chaque archive est vérifiée avant d'installer quoi que ce soit.
#
# Relancé, il met à jour : la dernière version remplace l'installée (modèles et
# réglages restent), ou il dit qu'elle est déjà là. `sillage --update` le
# relance.
#
#   … | sh -s -- --force         réinstalle même si la version est déjà là
#   … | sh -s -- --uninstall     retire sillage (--desinstaller marche aussi)
#
# SILLAGE_VERSION=0.3.0 : une version précise plutôt que la dernière (0.3.0 ou
# plus : avant, le lecteur s'appelait sillon).
# PREFIX=/usr/local    : où poser la commande (défaut ~/.local).
#
# Les messages sont en anglais, sauf en français si LC_ALL, LC_MESSAGES ou LANG
# (le premier qui n'est pas vide) a pour langue fr : même règle que le
# terminal et l'app.
#
# Pour compiler depuis les sources, voir scripts/install.sh et
# scripts/build-macos.sh.
# Apostrophes typographiques voulues dans les messages ; $PATH affiché tel quel.
# shellcheck disable=SC1112,SC2016
set -eu

# SILLAGE_DEPOT : un autre dépôt (ou un serveur local, pour les tests).
DEPOT="${SILLAGE_DEPOT:-https://github.com/freezer71/sillage}"
PREFIX="${PREFIX:-$HOME/.local}"

# La langue : ce qui précède le premier _, -, . ou @ du premier de LC_ALL,
# LC_MESSAGES, LANG qui n'est pas vide ; fr (sans tenir compte de la casse) →
# français, tout le reste (C, POSIX, rien) → anglais.
langue=""
for variable in "${LC_ALL:-}" "${LC_MESSAGES:-}" "${LANG:-}"; do
    if [ -n "$variable" ]; then
        langue=$variable
        break
    fi
done
langue=$(printf '%s' "$langue" | sed 's/[_.@-].*//' | tr '[:upper:]' '[:lower:]')
FR=""
if [ "$langue" = fr ]; then FR=1; fi
# t "English" "Français" : le message dans la langue choisie.
t() { if [ -n "$FR" ]; then printf '%s' "$2"; else printf '%s' "$1"; fi; }

if [ -t 2 ]; then
    gras=$(printf '\033[1m') ; vert=$(printf '\033[32m') ; rouge=$(printf '\033[31m') ; fin=$(printf '\033[0m')
else
    gras="" ; vert="" ; rouge="" ; fin=""
fi
etape() { printf '%s›%s %s\n' "$gras" "$fin" "$*" >&2; }
echec() { printf '%s%s%s %s\n' "$rouge" "$(t 'Error:' 'Erreur :')" "$fin" "$*" >&2; exit 1; }

# sillon, l'ancien nom (jusqu'à 0.2.1) : son app, sa commande et son
# dossier, s'ils viennent de lui. Ses modèles et réglages, eux, sont repris
# par sillage au premier lancement. Jamais en test (SILLAGE_APPLICATIONS).
retirer_sillon() {
    [ -z "${SILLAGE_APPLICATIONS:-}" ] || return 0
    ancienne="$PREFIX/bin/sillon"
    if [ -L "$ancienne" ] && readlink "$ancienne" | grep -q 'lib/sillon/sillon'; then
        rm -f "$ancienne"
    elif [ -f "$ancienne" ] && grep -q "ouvre l'app" "$ancienne" 2>/dev/null; then
        rm -f "$ancienne"
    fi
    rm -rf "$PREFIX/lib/sillon"
    if [ "$(uname -s)" = Darwin ]; then
        osascript -e 'quit app "Sillon"' >/dev/null 2>&1 || true
        for vieille in /Applications/Sillon.app "$HOME/Applications/Sillon.app"; do
            if [ -d "$vieille" ]; then
                rm -rf "$vieille" && etape "$(t "Old app removed: $vieille" "Ancienne app retirée : $vieille")"
            fi
        done
    fi
}

# --uninstall (ou --desinstaller) : retire ce que ce script a posé, et rien d'autre (les
# modèles téléchargés et les réglages restent, pour une réinstallation).
FORCER=""
DESINSTALLER=""
for option in "$@"; do
    case "$option" in
    --force) FORCER=1 ;;
    --uninstall | --desinstaller) DESINSTALLER=1 ;;
    *) echec "$(t "unknown option: $option (--force, --uninstall)" "option inconnue : $option (--force, --uninstall)")" ;;
    esac
done

if [ -n "$DESINSTALLER" ]; then
    if [ "$(uname -s)" = Darwin ]; then
        osascript -e 'quit app "Sillage"' >/dev/null 2>&1 || true
        rm -rf /Applications/Sillage.app "$HOME/Applications/Sillage.app"
        donnees="$HOME/Library/Application Support/sillage"
    else
        rm -rf "$PREFIX/lib/sillage"
        donnees="${XDG_DATA_HOME:-$HOME/.local/share}/sillage"
    fi
    rm -f "$PREFIX/bin/sillage"
    retirer_sillon
    printf '%s✓ %s%s\n' "$vert$gras" "$(t 'sillage is uninstalled.' 'sillage est désinstallé.')" "$fin" >&2
    printf '%s\n' "$(t "Models and settings stay in $donnees (rm -r to remove them)." "Les modèles et les réglages restent dans $donnees (rm -r pour les retirer).")" >&2
    exit 0
fi

command -v curl >/dev/null 2>&1 || echec "$(t 'curl is required' 'curl est nécessaire')"
command -v tar >/dev/null 2>&1 || echec "$(t 'tar is required' 'tar est nécessaire')"

# La version : celle demandée, sinon celle de la dernière publication (la
# redirection de /releases/latest donne son tag, sans passer par l'API).
if [ -n "${SILLAGE_VERSION:-}" ]; then
    version=${SILLAGE_VERSION#v}
else
    tag=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "$DEPOT/releases/latest") \
        || echec "$(t 'cannot reach GitHub' 'impossible de joindre GitHub')"
    version=${tag##*/v}
    case "$version" in
    "" | */*) echec "$(t "no release found at $DEPOT/releases" "aucune publication trouvée sur $DEPOT/releases")" ;;
    esac
fi

systeme=$(uname -s)
arch=$(uname -m)
# Un terminal sous Rosetta répond x86_64 sur un Mac à puce Apple.
if [ "$systeme" = Darwin ] && [ "$(sysctl -n hw.optional.arm64 2>/dev/null)" = 1 ]; then
    arch=arm64
fi
case "$systeme/$arch" in
Darwin/arm64) archive="sillage-macos-arm64.zip" ;;
Darwin/*) echec "$(t 'the app needs an Apple silicon Mac (M1 or later)' "l'app demande un Mac à puce Apple (M1 et suivants)")" ;;
Linux/x86_64 | Linux/amd64) archive="sillage-linux-x86_64.tar.gz" ;;
Linux/aarch64 | Linux/arm64) archive="sillage-linux-aarch64.tar.gz" ;;
*) echec "$(t "$systeme $arch is not supported (Apple silicon macOS, Linux x86_64 or ARM64)" "$systeme $arch n'est pas pris en charge (macOS à puce Apple, Linux x86_64 ou ARM64)")" ;;
esac

# Déjà à jour ? La version de l'app (Info.plist) ou du binaire installé.
installee=""
if [ "$systeme" = Darwin ]; then
    for dossier in ${SILLAGE_APPLICATIONS:+"$SILLAGE_APPLICATIONS"} /Applications "$HOME/Applications"; do
        plist="$dossier/Sillage.app/Contents/Info.plist"
        if [ -f "$plist" ]; then
            installee=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$plist" 2>/dev/null || true)
            break
        fi
    done
elif [ -x "$PREFIX/lib/sillage/sillage" ]; then
    installee=$("$PREFIX/lib/sillage/sillage" --version 2>/dev/null | cut -d' ' -f2 || true)
fi
if [ -n "$installee" ] && [ "$installee" = "$version" ] && [ -z "$FORCER" ]; then
    printf '%s✓ %s%s\n' "$vert$gras" "$(t "sillage $version is already up to date." "sillage $version est déjà à jour.")" "$fin" >&2
    printf '%s %scurl -fsSL https://sillage.koumalabs.org/install.sh | sh -s -- --force%s\n' "$(t 'To reinstall anyway:' 'Pour réinstaller quand même :')" "$gras" "$fin" >&2
    exit 0
fi
if [ -n "$installee" ] && [ "$installee" != "$version" ]; then
    etape "$(t "Updating: $installee → $version" "Mise à jour : $installee → $version")"
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM

url="$DEPOT/releases/download/v$version/$archive"
etape "$(t "Downloading sillage $version ($archive)" "Téléchargement de sillage $version ($archive)")"
curl -fL --progress-bar -o "$tmp/$archive" "$url" || echec "$(t "download failed: $url" "téléchargement impossible : $url")"
curl -fsSL -o "$tmp/$archive.sha256" "$url.sha256" || echec "$(t "checksum not found: $url.sha256" "empreinte introuvable : $url.sha256")"

attendue=$(cut -d' ' -f1 <"$tmp/$archive.sha256")
if command -v sha256sum >/dev/null 2>&1; then
    obtenue=$(sha256sum "$tmp/$archive" | cut -d' ' -f1)
else
    obtenue=$(shasum -a 256 "$tmp/$archive" | cut -d' ' -f1)
fi
[ "$attendue" = "$obtenue" ] || echec "$(t 'the archive checksum does not match: corrupted download' "l'empreinte de l'archive ne correspond pas : téléchargement corrompu")"
etape "$(t 'SHA-256 checksum verified' 'Empreinte SHA-256 vérifiée')"

mkdir -p "$PREFIX/bin"
retirer_sillon

if [ "$systeme" = Darwin ]; then
    # /Applications si on peut y écrire, sinon ~/Applications : jamais de sudo.
    # SILLAGE_APPLICATIONS (tests) : un autre dossier, sans fermer ni ouvrir l'app.
    apps=${SILLAGE_APPLICATIONS:-/Applications}
    if [ ! -w "$apps" ]; then
        apps="$HOME/Applications"
        mkdir -p "$apps"
    fi
    if [ -z "${SILLAGE_APPLICATIONS:-}" ] && pgrep -xq Sillage; then
        etape "$(t 'Closing sillage, which is open' 'Fermeture de sillage, ouvert')"
        osascript -e 'quit app "Sillage"' >/dev/null 2>&1 || true
        sleep 1
    fi
    ditto -x -k "$tmp/$archive" "$tmp/app"
    [ -d "$tmp/app/Sillage.app" ] || echec "$(t 'the archive does not contain Sillage.app' "l'archive ne contient pas Sillage.app")"
    rm -rf "$apps/Sillage.app"
    mv "$tmp/app/Sillage.app" "$apps/"
    # Téléchargée par curl, l'app n'est pas mise en quarantaine ; on retire
    # quand même l'attribut, au cas où.
    xattr -dr com.apple.quarantine "$apps/Sillage.app" 2>/dev/null || true
    etape "$(t "Sillage.app installed in $apps" "Sillage.app installée dans $apps")"

    # La commande `sillage`, qui ouvre l'app — sauf si elle est déjà le
    # lecteur au terminal (installé depuis les sources), qu'on ne remplace pas.
    commande="$PREFIX/bin/sillage"
    if [ -L "$commande" ] && readlink "$commande" | grep -q 'lib/sillage/sillage'; then
        etape "$(t "$commande is the terminal reader: left as is" "$commande est le lecteur au terminal : laissé tel quel")"
    else
        introuvable=$(t 'not found' 'introuvable')
        cat >"$commande" <<EOF
#!/bin/sh
$(t "# The Mac \`sillage\` command: opens the app, not the terminal.
# Installed by install.sh. To remove it: rm \"$commande\"" "# La commande \`sillage\` du Mac : ouvre l'app, pas le terminal.
# Installée par install.sh. Pour la retirer : rm \"$commande\"")
APP="\${SILLAGE_APP:-$apps/Sillage.app}"
if [ "\${1:-}" = --update ]; then
    curl -fsSL https://sillage.koumalabs.org/install.sh | PREFIX="$PREFIX" sh
    exit
fi
[ -d "\$APP" ] || { echo "sillage: \$APP $introuvable" >&2; exit 1; }
[ \$# -eq 0 ] && exec open -a "\$APP"
exec open -a "\$APP" -- "\$@"
EOF
        chmod +x "$commande"
    fi
    [ -n "${SILLAGE_APPLICATIONS:-}" ] || open "$apps/Sillage.app" 2>/dev/null || true
else
    mkdir "$tmp/paquet"
    tar -xzf "$tmp/$archive" -C "$tmp/paquet"
    # Un seul dossier dans l'archive (sillage-<version>-linux-<arch>).
    dossier=$(find "$tmp/paquet" -mindepth 1 -maxdepth 1 -type d | head -1)
    if [ -z "$dossier" ] || [ ! -x "$dossier/sillage" ]; then
        echec "$(t 'the archive does not contain the sillage binary' "l'archive ne contient pas le binaire sillage")"
    fi
    mkdir -p "$PREFIX/lib"
    rm -rf "$PREFIX/lib/sillage"
    mv "$dossier" "$PREFIX/lib/sillage"
    ln -sfn ../lib/sillage/sillage "$PREFIX/bin/sillage"
    etape "$(t "Installed in $PREFIX/lib/sillage" "Installé dans $PREFIX/lib/sillage")"
    "$PREFIX/bin/sillage" --check-kokoro >/dev/null 2>&1 \
        || etape "$(t "Warning: Kokoro does not load ($PREFIX/bin/sillage --check-kokoro); the system voice will be used instead" "Attention : Kokoro ne se charge pas ($PREFIX/bin/sillage --check-kokoro) ; la voix du système lira")"
fi

printf '\n%s✓ %s%s\n' "$vert$gras" "$(t "sillage $version is installed." "sillage $version est installé.")" "$fin" >&2
case ":$PATH:" in
*":$PREFIX/bin:"*) ;;
*)
    printf '\n%s\n' "$(t "$PREFIX/bin is not in your PATH. Add this to your ~/.zshrc or ~/.bashrc:" "$PREFIX/bin n’est pas dans votre PATH. Ajoutez à votre ~/.zshrc ou ~/.bashrc :")" >&2
    printf '  export PATH="%s/bin:$PATH"\n' "$PREFIX" >&2
    ;;
esac
if [ "$systeme" = Darwin ]; then
    printf '\n%s\n' "$(t 'sillage is open in the menu bar. The Kokoro voice is installed automatically.' 'sillage est ouvert dans la barre des menus. La voix Kokoro s’installe toute seule.')" >&2
    printf '%s %ssillage document.pdf%s\n' "$(t 'To read a file:' 'Pour lire un fichier :')" "$gras" "$fin" >&2
else
    printf '\n%s %ssillage document.md%s   ·   cat notes.md | sillage\n' "$(t 'To read a file:' 'Pour lire un fichier :')" "$gras" "$fin" >&2
    printf '%s\n' "$(t 'The Kokoro voice (108 MB) is downloaded automatically the first time you read.' 'La voix Kokoro (108 Mo) se télécharge toute seule à la première lecture.')" >&2
fi
printf '%s %scurl -fsSL https://sillage.koumalabs.org/install.sh | sh -s -- --uninstall%s\n' "$(t 'To uninstall:' 'Pour désinstaller :')" "$gras" "$fin" >&2
