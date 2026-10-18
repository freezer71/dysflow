#!/bin/sh
# Installe sillon en une commande, depuis la dernière publication GitHub :
#
#   curl -fsSL https://raw.githubusercontent.com/freezer71/sillon/main/install.sh | sh
#
# Sur un Mac à puce Apple : l'app dans /Applications (ou ~/Applications), et la
# commande `sillon` qui l'ouvre. Sous Linux (x86_64, ARM64) : le lecteur au
# terminal dans ~/.local/lib/sillon, et la commande `sillon`. L'empreinte
# SHA-256 de chaque archive est vérifiée avant d'installer quoi que ce soit.
#
# SILLON_VERSION=0.2.1 : une version précise plutôt que la dernière.
# PREFIX=/usr/local    : où poser la commande (défaut ~/.local).
#
# Pour compiler depuis les sources, voir scripts/install.sh et
# scripts/build-macos.sh.
# Apostrophes typographiques voulues dans les messages ; $PATH affiché tel quel.
# shellcheck disable=SC1112,SC2016
set -eu

# SILLON_DEPOT : un autre dépôt (ou un serveur local, pour les tests).
DEPOT="${SILLON_DEPOT:-https://github.com/freezer71/sillon}"
PREFIX="${PREFIX:-$HOME/.local}"

if [ -t 2 ]; then
    gras=$(printf '\033[1m') ; vert=$(printf '\033[32m') ; rouge=$(printf '\033[31m') ; fin=$(printf '\033[0m')
else
    gras="" ; vert="" ; rouge="" ; fin=""
fi
etape() { printf '%s›%s %s\n' "$gras" "$fin" "$*" >&2; }
echec() { printf '%sErreur :%s %s\n' "$rouge" "$fin" "$*" >&2; exit 1; }

# --desinstaller : retire ce que ce script a posé, et rien d'autre (les
# modèles téléchargés et les réglages restent, pour une réinstallation).
if [ "${1:-}" = --desinstaller ]; then
    if [ "$(uname -s)" = Darwin ]; then
        osascript -e 'quit app "Sillon"' >/dev/null 2>&1 || true
        rm -rf /Applications/Sillon.app "$HOME/Applications/Sillon.app"
        donnees="$HOME/Library/Application Support/sillon"
    else
        rm -rf "$PREFIX/lib/sillon"
        donnees="${XDG_DATA_HOME:-$HOME/.local/share}/sillon"
    fi
    rm -f "$PREFIX/bin/sillon"
    printf '%s✓ sillon est désinstallé.%s\n' "$vert$gras" "$fin" >&2
    printf 'Les modèles et les réglages restent dans %s (rm -r pour les retirer).\n' "$donnees" >&2
    exit 0
fi

command -v curl >/dev/null 2>&1 || echec "curl est nécessaire"
command -v tar >/dev/null 2>&1 || echec "tar est nécessaire"

# La version : celle demandée, sinon celle de la dernière publication (la
# redirection de /releases/latest donne son tag, sans passer par l'API).
if [ -n "${SILLON_VERSION:-}" ]; then
    version=${SILLON_VERSION#v}
else
    tag=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "$DEPOT/releases/latest") \
        || echec "impossible de joindre GitHub"
    version=${tag##*/v}
    case "$version" in
    "" | */*) echec "aucune publication trouvée sur $DEPOT/releases" ;;
    esac
fi

systeme=$(uname -s)
arch=$(uname -m)
# Un terminal sous Rosetta répond x86_64 sur un Mac à puce Apple.
if [ "$systeme" = Darwin ] && [ "$(sysctl -n hw.optional.arm64 2>/dev/null)" = 1 ]; then
    arch=arm64
fi
case "$systeme/$arch" in
Darwin/arm64) archive="sillon-$version-macos-arm64.zip" ;;
Darwin/*) echec "l'app demande un Mac à puce Apple (M1 et suivants)" ;;
Linux/x86_64 | Linux/amd64) archive="sillon-$version-linux-x86_64.tar.gz" ;;
Linux/aarch64 | Linux/arm64) archive="sillon-$version-linux-aarch64.tar.gz" ;;
*) echec "$systeme $arch n'est pas pris en charge (macOS à puce Apple, Linux x86_64 ou ARM64)" ;;
esac

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM

etape "Téléchargement de sillon $version ($archive)"
url="$DEPOT/releases/download/v$version/$archive"
curl -fL --progress-bar -o "$tmp/$archive" "$url" || echec "téléchargement impossible : $url"
curl -fsSL -o "$tmp/$archive.sha256" "$url.sha256" || echec "empreinte introuvable : $url.sha256"

attendue=$(cut -d' ' -f1 <"$tmp/$archive.sha256")
if command -v sha256sum >/dev/null 2>&1; then
    obtenue=$(sha256sum "$tmp/$archive" | cut -d' ' -f1)
else
    obtenue=$(shasum -a 256 "$tmp/$archive" | cut -d' ' -f1)
fi
[ "$attendue" = "$obtenue" ] || echec "l'empreinte de l'archive ne correspond pas : téléchargement corrompu"
etape "Empreinte SHA-256 vérifiée"

mkdir -p "$PREFIX/bin"

if [ "$systeme" = Darwin ]; then
    # /Applications si on peut y écrire, sinon ~/Applications : jamais de sudo.
    # SILLON_APPLICATIONS (tests) : un autre dossier, sans fermer ni ouvrir l'app.
    apps=${SILLON_APPLICATIONS:-/Applications}
    if [ ! -w "$apps" ]; then
        apps="$HOME/Applications"
        mkdir -p "$apps"
    fi
    if [ -z "${SILLON_APPLICATIONS:-}" ] && pgrep -xq Sillon; then
        etape "Fermeture de sillon, ouvert"
        osascript -e 'quit app "Sillon"' >/dev/null 2>&1 || true
        sleep 1
    fi
    ditto -x -k "$tmp/$archive" "$tmp/app"
    [ -d "$tmp/app/Sillon.app" ] || echec "l'archive ne contient pas Sillon.app"
    rm -rf "$apps/Sillon.app"
    mv "$tmp/app/Sillon.app" "$apps/"
    # Téléchargée par curl, l'app n'est pas mise en quarantaine ; on retire
    # quand même l'attribut, au cas où.
    xattr -dr com.apple.quarantine "$apps/Sillon.app" 2>/dev/null || true
    etape "Sillon.app installée dans $apps"

    # La commande `sillon`, qui ouvre l'app — sauf si elle est déjà le
    # lecteur au terminal (installé depuis les sources), qu'on ne remplace pas.
    commande="$PREFIX/bin/sillon"
    if [ -L "$commande" ] && readlink "$commande" | grep -q 'lib/sillon/sillon'; then
        etape "$commande est le lecteur au terminal : laissé tel quel"
    else
        cat >"$commande" <<EOF
#!/bin/sh
# La commande \`sillon\` du Mac : ouvre l'app, pas le terminal.
# Installée par install.sh. Pour la retirer : rm "$commande"
APP="\${SILLON_APP:-$apps/Sillon.app}"
[ -d "\$APP" ] || { echo "sillon : \$APP introuvable" >&2; exit 1; }
[ \$# -eq 0 ] && exec open -a "\$APP"
exec open -a "\$APP" -- "\$@"
EOF
        chmod +x "$commande"
    fi
    [ -n "${SILLON_APPLICATIONS:-}" ] || open "$apps/Sillon.app" 2>/dev/null || true
else
    tar -xzf "$tmp/$archive" -C "$tmp"
    dossier="$tmp/sillon-$version-linux-${archive##*-linux-}"
    dossier=${dossier%.tar.gz}
    [ -x "$dossier/sillon" ] || echec "l'archive ne contient pas le binaire sillon"
    mkdir -p "$PREFIX/lib"
    rm -rf "$PREFIX/lib/sillon"
    mv "$dossier" "$PREFIX/lib/sillon"
    ln -sfn ../lib/sillon/sillon "$PREFIX/bin/sillon"
    etape "Installé dans $PREFIX/lib/sillon"
    "$PREFIX/bin/sillon" --check-kokoro >/dev/null 2>&1 \
        || etape "Attention : Kokoro ne se charge pas ($PREFIX/bin/sillon --check-kokoro) ; la voix du système lira"
fi

printf '\n%s✓ sillon %s est installé.%s\n' "$vert$gras" "$version" "$fin" >&2
case ":$PATH:" in
*":$PREFIX/bin:"*) ;;
*)
    printf '\n%s/bin n’est pas dans votre PATH. Ajoutez à votre ~/.zshrc ou ~/.bashrc :\n' "$PREFIX" >&2
    printf '  export PATH="%s/bin:$PATH"\n' "$PREFIX" >&2
    ;;
esac
if [ "$systeme" = Darwin ]; then
    printf '\nsillon est ouvert dans la barre des menus. La voix Kokoro s’installe toute seule.\n' >&2
    printf 'Pour lire un fichier : %ssillon document.pdf%s\n' "$gras" "$fin" >&2
else
    printf '\nPour lire un fichier : %ssillon document.md%s   ·   cat notes.md | sillon\n' "$gras" "$fin" >&2
    printf 'La voix Kokoro (108 Mo) se télécharge toute seule à la première lecture.\n' >&2
fi
printf 'Pour désinstaller : %scurl -fsSL https://raw.githubusercontent.com/freezer71/sillon/main/install.sh | sh -s -- --desinstaller%s\n' "$gras" "$fin" >&2
