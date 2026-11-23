#!/bin/sh
# Le paquet du terminal de bout en bout : depuis le dossier du paquet (celui de
# package.sh, ou l'archive Windows décompressée), la voix Kokoro écrit le texte
# de référence en WAV (`dysflow -o`), puis la dictée (`--transcribe`, Parakeet
# v3 int8 et le Silero VAD du paquet) relit l'échantillon français de
# référence du dépôt de Parakeet et le WAV de la synthèse.
#
#   ./scripts/paquet-bout-en-bout.sh PAQUET MODÈLES
#
# MODÈLES : le dossier des modèles (en CI, MODELS : le cache de la porte,
# Silero et Kokoro, et celui de Parakeet, chacun sous l'empreinte de ses
# lignes de scripts/modeles-epingles.txt). Chaque fichier y est vérifié
# par son SHA-256 et téléchargé de son adresse épinglée seulement s'il manque
# ou n'a pas la bonne empreinte (scripts/modeles-epingles.sh).
#
# Le binaire est lancé depuis /, sans les variables qui désignent ORT, espeak
# ou Silero ailleurs. Sous Linux et macOS, avec un HOME (et des XDG_*)
# provisoires, créés par le script : les familles de modèles y sont des liens
# vers le cache, rien n'est copié, rien n'est écrit chez l'utilisateur. Sous
# Windows, le dossier des données vient du profil (dirs, Known Folders), qu'une
# variable ne déplace pas : les modèles sont copiés dans
# %LOCALAPPDATA%\dysflow\models. Le script ne le fait que sur une machine de CI
# (CI=true) ou sur demande explicite (DYSFLOW_BOUT_EN_BOUT_PROFIL=1), et
# refuse si ce dossier a déjà des modèles Kokoro ou Parakeet : il ne retire
# jamais que ce qu'il a créé, à la sortie, réussie ou non.
#
# Rien n'est joué ni enregistré : le WAV est écrit dans un fichier, la dictée
# lit des fichiers.
set -eu
# Aucun ONNX Runtime lancé d'ici n'écrit ni n'envoie sa télémétrie
# (onnx::NO_TELEMETRY).
export ORT_DISABLE_TELEMETRY=1

if [ "$#" -ne 2 ]; then
    echo "Usage : $0 PAQUET MODÈLES" >&2
    exit 2
fi
paquet=$(cd "$1" && pwd)
mkdir -p "$2"
cache=$(cd "$2" && pwd)
cd "$(dirname "$0")/.."

case "$(uname -s)" in
MINGW* | MSYS* | CYGWIN*) WINDOWS=1 EXE=.exe ;;
*) WINDOWS="" EXE="" ;;
esac
if [ -n "$WINDOWS" ] && [ "${CI:-}" != true ] && [ "${DYSFLOW_BOUT_EN_BOUT_PROFIL:-}" != 1 ]; then
    {
        echo "Erreur : sous Windows, ce script copie les modèles dans le vrai profil (%LOCALAPPDATA%\dysflow\models) ;"
        echo "  il ne tourne qu'en CI (CI=true), ou avec DYSFLOW_BOUT_EN_BOUT_PROFIL=1 pour l'accepter."
        echo "Error: on Windows, this script copies the models into the real profile (%LOCALAPPDATA%\dysflow\models);"
        echo "  it only runs in CI (CI=true), or with DYSFLOW_BOUT_EN_BOUT_PROFIL=1 to accept it."
    } >&2
    exit 2
fi
if python3 -c "" >/dev/null 2>&1; then
    PYTHON=python3
elif python -c "import sys; sys.exit(sys.version_info[0] != 3)" >/dev/null 2>&1; then
    PYTHON=python
else
    echo "Erreur : Python 3 introuvable (python3 ou python)" >&2
    exit 1
fi

bin="$paquet/dysflow$EXE"
[ -x "$bin" ] || { echo "Erreur : $bin absent" >&2; exit 1; }

# Le dossier des modèles du binaire (src/paths.rs : dirs::data_local_dir,
# puis dysflow/models), vérifié avant tout téléchargement ou copie.
travail=$(mktemp -d "${TMPDIR:-/tmp}/dysflow-bout-en-bout.XXXXXX")
# Ce que le script crée dans le profil (Windows) : un dossier entier
# (cree_dossier), ou les deux familles dans un dossier qui existait
# (cree_familles=1).
cree_dossier=""
cree_familles=""
# nettoie : ce que le script a créé, et seulement cela, à toute sortie.
nettoie() {
    if [ -n "$cree_dossier" ]; then
        rm -rf "${cree_dossier:?}"
    elif [ -n "$cree_familles" ]; then
        rm -rf "${modeles:?}/kokoro" "${modeles:?}/parakeet_tdt_v3"
    fi
    rm -rf "${travail:?}"
}
trap nettoie EXIT
trap 'exit 130' INT TERM
if [ -n "$WINDOWS" ]; then
    donnees="$(cygpath -u "$LOCALAPPDATA")/dysflow"
elif [ "$(uname -s)" = Darwin ]; then
    donnees="$travail/home/Library/Application Support/dysflow"
else
    donnees="$travail/data/dysflow"
fi
modeles="$donnees/models"
for famille in kokoro parakeet_tdt_v3; do
    if [ -e "$modeles/$famille" ] || [ -L "$modeles/$famille" ]; then
        {
            echo "Erreur : $modeles/$famille existe déjà ; le script ne remplace pas des modèles qu'il n'a pas posés."
            echo "Error: $modeles/$famille already exists; the script does not replace models it did not put there."
        } >&2
        exit 1
    fi
done

./scripts/modeles-epingles.sh "$cache"

if [ -n "$WINDOWS" ]; then
    # Le profil : seulement ce qui n'y était pas, retiré à la sortie.
    if [ ! -e "$donnees" ]; then
        cree_dossier=$donnees
    elif [ ! -e "$modeles" ]; then
        cree_dossier=$modeles
    else
        cree_familles=1
    fi
    mkdir -p "$modeles"
    cp -R "$cache/kokoro" "$cache/parakeet_tdt_v3" "$modeles/"
else
    mkdir -p "$modeles"
    ln -s "$cache/kokoro" "$modeles/kokoro"
    ln -s "$cache/parakeet_tdt_v3" "$modeles/parakeet_tdt_v3"
fi

# lance ARGUMENTS… : le binaire du paquet, depuis /, en anglais, sans les
# variables qui désigneraient ORT, espeak ou Silero ailleurs. Sous Windows,
# avec un PATH réduit à Windows (aucune DLL de Git ou de la CI ne masque une
# DLL qui manquerait au paquet) ; ailleurs, avec le HOME provisoire.
lance() {
    if [ -n "$WINDOWS" ]; then
        ( cd / && env -u DYSFLOW_ORT_LIB -u DYSFLOW_ESPEAK -u DYSFLOW_ESPEAK_DATA -u DYSFLOW_SILERO_VAD \
            -u DYSFLOW_KOKORO_DIR -u DYSFLOW_MODELS LC_ALL=C PATH="/c/Windows/System32:/c/Windows" "$@" )
    else
        ( cd / && env -u DYSFLOW_ORT_LIB -u DYSFLOW_ESPEAK -u DYSFLOW_ESPEAK_DATA -u DYSFLOW_SILERO_VAD \
            -u DYSFLOW_KOKORO_DIR -u DYSFLOW_MODELS LC_ALL=C HOME="$travail/home" \
            XDG_DATA_HOME="$travail/data" XDG_CONFIG_HOME="$travail/config" \
            XDG_CACHE_HOME="$travail/cache" "$@" )
    fi
}

reference="Ne vous demandez pas ce que votre pays peut faire pour vous. Demandez-vous plutôt ce que vous pouvez faire pour lui."
printf '%s\n' "$reference" > "$travail/reference.txt"

# 1. La voix : Kokoro, jamais la voix de repli.
lance "$bin" -o "$travail/synthese.wav" --language fr "$travail/reference.txt" 2> "$travail/synthese.log"
cat "$travail/synthese.log"
if grep -q "fallback voice" "$travail/synthese.log" || ! grep -q "Kokoro" "$travail/synthese.log"; then
    echo "Erreur : la synthèse n'a pas parlé avec Kokoro" >&2
    exit 1
fi

# 16 kHz mono, PCM 16 bits : ce que --transcribe lit. Interpolation linéaire,
# par la bibliothèque standard de Python.
au_16k() {
    "$PYTHON" -I -c '
import array, sys, wave
with wave.open(sys.argv[1], "rb") as w:
    assert w.getsampwidth() == 2, "PCM 16 bits attendu"
    rate, channels = w.getframerate(), w.getnchannels()
    pcm = array.array("h", w.readframes(w.getnframes()))
if sys.byteorder == "big":
    pcm.byteswap()
mono = [sum(pcm[i:i + channels]) / channels for i in range(0, len(pcm), channels)]
n = int(len(mono) * 16000 / rate)
out = array.array("h")
for i in range(n):
    x = i * rate / 16000
    j = int(x)
    a = mono[j]
    b = mono[j + 1] if j + 1 < len(mono) else a
    out.append(int(round(a + (b - a) * (x - j))))
if sys.byteorder == "big":
    out.byteswap()
with wave.open(sys.argv[2], "wb") as w:
    w.setnchannels(1); w.setsampwidth(2); w.setframerate(16000)
    w.writeframes(out.tobytes())
print(f"{sys.argv[2]} : {n / 16000:.1f} s (depuis {rate} Hz, {channels} canal(aux))")
' "$1" "$2"
}

# Le taux d'erreur de mots, normalisé comme stt::wer (casse, ponctuation,
# apostrophes, traits d'union).
wer() {
    "$PYTHON" -I -c '
import sys
def words(text):
    out, word = [], ""
    for c in text.lower():
        if c.isalnum():
            word += c
        elif c in "\x27\u2019" and word:
            out.append(word + "\x27"); word = ""
        elif word:
            out.append(word); word = ""
    return out + ([word] if word else [])
r, h = words(sys.argv[1]), words(sys.argv[2])
row = list(range(len(h) + 1))
for i, a in enumerate(r):
    diagonal, row[0] = row[0], i + 1
    for j, b in enumerate(h):
        diagonal, row[j + 1] = row[j + 1], min(row[j + 1] + 1, row[j] + 1, diagonal + (a != b))
print(f"{row[-1] / max(len(r), 1):.3f}")
' "$1" "$2"
}

# dicte WAV SEUIL : --transcribe du WAV (ramené à 16 kHz), WER au plus SEUIL.
dicte() {
    au_16k "$1" "$1.16k.wav"
    lance "$bin" --transcribe "$1.16k.wav" --dictation-model parakeet_tdt_v3_int8 \
        --dictation-language fr > "$1.txt" 2> "$1.log" || { cat "$1.log" >&2; exit 1; }
    cat "$1.log"
    texte=$(cat "$1.txt")
    erreur=$(wer "$reference" "$texte")
    echo "Dicté ($1) : $texte"
    echo "WER : $erreur (au plus $2)"
    if ! "$PYTHON" -I -c 'import sys; sys.exit(float(sys.argv[1]) > float(sys.argv[2]))' "$erreur" "$2"; then
        echo "Erreur : WER $erreur au-delà de $2" >&2
        exit 1
    fi
}

# 2. La dictée : l'échantillon de référence, puis la synthèse.
cp "$cache/parakeet_tdt_v3/test_wavs/fr.wav" "$travail/fr.wav"
dicte "$travail/fr.wav" 0.1
dicte "$travail/synthese.wav" 0.25
echo "Bout en bout : la voix et la dictée du paquet marchent"
