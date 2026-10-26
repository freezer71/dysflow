#!/bin/sh
# Teste la langue de install.sh : anglais par défaut, français si LC_ALL,
# LC_MESSAGES ou LANG (dans cet ordre, le premier non vide) commence par fr.
# Ne touche jamais à l'installation réelle : HOME et PREFIX temporaires, et la
# désinstallation n'est exécutée que sous Linux (sous macOS elle retirerait
# /Applications/Dysflow.app).
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
            sortie=$(env -i PATH="$PATH" HOME="$tmp/home" PREFIX="$tmp/prefix" DYSFLOW_APPLICATIONS="$tmp" LANG="$langue" sh "$racine/install.sh" "$nom" 2>&1)
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

# --uninstall retire aussi les modèles, réglages et cache (nom actuel et
# anciens) ; --keep-data les garde. Seulement sous Linux, comme au-dessus.
if [ "$(uname -s)" = Linux ]; then
    for cas in tout garder; do
        maison="$tmp/maison-$cas"
        for rep in .config/dysflow .local/share/dysflow/models .local/share/sillage .cache/dysflow; do
            mkdir -p "$maison/$rep" && echo modele >"$maison/$rep/fichier"
        done
        options=--uninstall
        [ "$cas" = tout ] || options="--uninstall --keep-data"
        # shellcheck disable=SC2086
        sortie=$(env -i PATH="$PATH" HOME="$maison" PREFIX="$tmp/prefix" DYSFLOW_APPLICATIONS="$tmp" LANG=fr_FR.UTF-8 sh "$racine/install.sh" $options 2>&1) || true
        restants=$(find "$maison" -name fichier | wc -l | tr -d ' ')
        if [ "$cas" = tout ] && [ "$restants" -eq 0 ] && printf '%s' "$sortie" | grep -q "libérés"; then
            echo "ok   --uninstall retire modèles et réglages"
        elif [ "$cas" = garder ] && [ "$restants" -eq 4 ] && printf '%s' "$sortie" | grep -q "restent"; then
            echo "ok   --keep-data garde modèles et réglages"
        else
            echo "ECHEC $options : $restants fichier(s) restant(s), sortie : $sortie"
            echecs=$((echecs + 1))
        fi
    done
fi

[ "$echecs" -eq 0 ] || { echo "$echecs échec(s)"; exit 1; }
echo "install.sh : langues vérifiées"
