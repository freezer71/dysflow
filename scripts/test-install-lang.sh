#!/bin/sh
# Teste la langue de install.sh : anglais par défaut, français si LC_ALL,
# LC_MESSAGES ou LANG (dans cet ordre, le premier non vide) commence par fr.
# Ne touche jamais à l'installation réelle : HOME et PREFIX temporaires, et la
# désinstallation n'est exécutée que sous Linux (sous macOS elle retirerait
# /Applications/Sillage.app).
#
#   sh scripts/test-install-lang.sh
set -eu

racine=$(cd "$(dirname "$0")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM
echecs=0

# lancer <attendu> <arguments> -- <variables d'environnement…>
verifie() {
    attendu=$1 ; option=$2 ; shift 2
    set +e
    sortie=$(env -i PATH="$PATH" HOME="$tmp/home" PREFIX="$tmp/prefix" "$@" sh "$racine/install.sh" "$option" 2>&1)
    code=$?
    set -e
    if [ "$code" -ne 0 ] && printf '%s' "$sortie" | grep -q -e "$attendu"; then
        echo "ok   $* -> $attendu"
    else
        echo "ECHEC $* -> attendu « $attendu », code $code, sortie : $sortie"
        echecs=$((echecs + 1))
    fi
}

mkdir -p "$tmp/home" "$tmp/prefix"
verifie "option inconnue" --option-inconnue LANG=fr_FR.UTF-8
verifie "unknown option" --option-inconnue LANG=C
verifie "option inconnue" --option-inconnue LC_ALL= LANG=fr_CA.UTF-8
verifie "unknown option" --option-inconnue LC_ALL=en_US.UTF-8 LANG=fr_FR.UTF-8
verifie "option inconnue" --option-inconnue LC_MESSAGES=FR LANG=en_US.UTF-8
verifie "option inconnue" --option-inconnue LANG=fr@euro
verifie "unknown option" --option-inconnue LANG=francais
verifie "unknown option" --option-inconnue LANG=POSIX
verifie "unknown option" --option-inconnue
verifie "--force, --uninstall" --option-inconnue

# Les deux noms de l'option de désinstallation sont acceptés.
for nom in --uninstall --desinstaller; do
    # La ligne du case : « --uninstall | --desinstaller) », pas le message d'erreur.
    if grep -E "^[[:space:]]*(--[a-z]+ \| )*$nom( \| --[a-z]+)*\)" "$racine/install.sh" | grep -q 'DESINSTALLER=1'; then
        echo "ok   $nom accepté"
    else
        echo "ECHEC $nom absent du case"
        echecs=$((echecs + 1))
    fi
done

# Sous Linux, la désinstallation s'exécute pour de bon (préfixe temporaire).
if [ "$(uname -s)" = Linux ]; then
    for nom in --uninstall --desinstaller; do
        for langue in en_US.UTF-8 fr_FR.UTF-8; do
            set +e
            sortie=$(env -i PATH="$PATH" HOME="$tmp/home" PREFIX="$tmp/prefix" SILLAGE_APPLICATIONS="$tmp" LANG="$langue" sh "$racine/install.sh" "$nom" 2>&1)
            code=$?
            set -e
            case "$langue/$sortie" in
            en*/*uninstalled*) attendu=oui ;;
            fr*/*désinstallé*) attendu=oui ;;
            *) attendu=non ;;
            esac
            if [ "$code" -eq 0 ] && [ "$attendu" = oui ]; then
                echo "ok   $nom ($langue) désinstalle"
            else
                echo "ECHEC $nom ($langue) : code $code, $sortie"
                echecs=$((echecs + 1))
            fi
        done
    done
fi

[ "$echecs" -eq 0 ] || { echo "$echecs échec(s)"; exit 1; }
echo "install.sh : langues vérifiées"
