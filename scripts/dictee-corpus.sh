#!/usr/bin/env bash
# Le corpus de la porte du lot 4 (tâche 10 : qualité de la découpe, détecteur
# du Mac), hors git, épinglé par empreinte. Télécharge ce qui manque, vérifie
# tout, puis construit les audios du banc (examples/dictation_bench.rs).
#
#   ./scripts/dictee-corpus.sh [<dossier>]      # target/dictee-corpus par défaut
#
# Sur Mac seulement : afconvert (macOS) ramène tout en 16 kHz mono, 16 bits.
# Rien n'est joué.
#
# Sources, toutes redistribuables :
#   - FLEURS (Google, CC-BY-4.0), dev en français (fr_fr) et en anglais
#     (en_us) : des locuteurs réels qui lisent des phrases de Wikipédia, avec
#     leur transcription. Dépôt google/fleurs de Hugging Face, révision
#     70bb2e84b976b7e960aa89f1c648e09c59f894dd. Les plus petits fichiers qui
#     portent de l'audio (dev : 143 et 171 Mo ; test : 349 et 290 Mo ; train :
#     1,7 et 1,4 Go). FLEURS ne nomme pas ses locuteurs : les audios mis bout à
#     bout mêlent des voix, ramenées au même niveau.
#   - DEMAND (Thiemann, Ito, Vincent, 2013 ; CC-BY-4.0), Zenodo 1227121 :
#     PCAFETER_16k.zip, une cafétéria, micro 1 (ch01.wav), le bruit de fond
#     mêlé à 10 dB.
#   - Lingua Libre (Wikimedia Commons) : des mots seuls, enregistrés par des
#     locuteurs nommés ; licence de chaque fichier dans LINGUA_LIBRE plus bas
#     (CC0, CC BY 4.0 ou CC BY-SA 4.0). « oui », « non », « si » en français,
#     « yes », « no » en anglais (R37). Trois clips anglais prévus
#     (LetsgoLeeLee « yes », Robust Footman « yes » et « no ») ne sont pas
#     épinglés : Commons a refusé de les servir (HTTP 429) le 23 nov. 2026.
#
# Pas de voyelle tenue réelle dans ces sources (cherchée sur Commons) : non
# mesurée. Le locuteur lent est le plus lent (mots par seconde de parole) du
# dev français de FLEURS.
#
# Les audios construits (<dossier>/audios/, chacun avec son texte .txt) :
#   fr-continu      phrases FLEURS en français, silences de tête et de queue
#                   retirés, 0,15 s entre deux : jamais 0,416 s de silence
#                   voulu, la coupe forcée à 12 s travaille ; 45 s au moins
#   en-continu      la même chose en anglais
#   fr-pauses       phrases FLEURS en français, entières, leurs silences
#                   (pauses naturelles) gardés ; 45 s au moins
#   fr-bruit-10db   d'autres phrases entières, mêlées au bruit de la cafétéria
#                   10 dB sous la parole (niveau de la parole : ses trames
#                   actives) ; 45 s au moins
#   fr-boucle-10min fr-pauses répété jusqu'à 10 minutes au moins
#   fr-lent         la phrase la plus lente du dev français, entière
#   mot-*           un mot Lingua Libre, 1 s de silence numérique devant,
#                   1,5 s derrière
# Chaque phrase FLEURS est ramenée à -23 dBFS (RMS de ses trames actives) ;
# les mots seuls gardent leur niveau d'origine. <dossier>/audios/corpus.json
# décrit chaque audio (sources, durées, pauses).
#
# Chaque téléchargement va dans son propre dossier neuf, sous
# <dossier>/sources/ ; l'extraction, dans <dossier>/extrait/. Rien de
# téléchargé n'est lancé ; le Python de la construction (bibliothèque standard
# seule) vient de ce script, lancé en -I.
set -euo pipefail
# Défense en profondeur : aucun ONNX Runtime lancé d'ici n'écrit ni n'envoie sa télémétrie (onnx::NO_TELEMETRY).
export ORT_DISABLE_TELEMETRY=1
cd "$(dirname "$0")/.."
# shellcheck source=scripts/paquet-commun.sh
. scripts/paquet-commun.sh

if [ "$(uname -s)" != Darwin ]; then
    echo "Erreur : le corpus se construit sur Mac (afconvert)" >&2
    exit 1
fi
CORPUS=${1:-target/dictee-corpus}
mkdir -p "$CORPUS"
CORPUS=$(cd "$CORPUS" && pwd -P)

FLEURS=https://huggingface.co/datasets/google/fleurs/resolve/70bb2e84b976b7e960aa89f1c648e09c59f894dd/data
# nom|URL|SHA-256|octets
SOURCES="
fleurs-fr_fr-audio/dev.tar.gz|$FLEURS/fr_fr/audio/dev.tar.gz|f2f065dec3b02212e27151c51162d2213df55d0a8efc6b88e36992673ddf66e6|142638560
fleurs-fr_fr-tsv/dev.tsv|$FLEURS/fr_fr/dev.tsv|3e0b792358c1cb4a426fe1c18fc1571d5406b390d738a2ed1bfd3c8b9d28de44|180518
fleurs-en_us-audio/dev.tar.gz|$FLEURS/en_us/audio/dev.tar.gz|2658fda72f199e12676ecac9415094667a4e14e149b146e568ea00b2a2f0954c|171250900
fleurs-en_us-tsv/dev.tsv|$FLEURS/en_us/dev.tsv|9d57ee7e91e9d4c92edb39f6bbea668ef8dc2a3ff96eb510d5580b2ad05d17ec|213065
demand-pcafeter/PCAFETER_16k.zip|https://zenodo.org/records/1227121/files/PCAFETER_16k.zip|89be7194b83cd583c4b12f4bf7c19e9cf0e612b95312e9f4aedc968cd811bcd5|107431494
"
COMMONS=https://upload.wikimedia.org/wikipedia/commons
# nom|langue|mot|locuteur|licence|URL|SHA-256
LINGUA_LIBRE="
fr-oui-poslovitch|fr|oui|Poslovitch|CC0|$COMMONS/0/0f/LL-Q150_%28fra%29-Poslovitch-oui.wav|da08fc55d6e92ba150ac6e028912477be0caeadb96e89320ffa21cf9d4f21362
fr-non-poslovitch|fr|non|Poslovitch|CC0|$COMMONS/3/3b/LL-Q150_%28fra%29-Poslovitch-non.wav|51fd32835e3ef2f8b88d6df4a9f696457596f67ed9a39edcad98045f7e8079a9
fr-si-poslovitch|fr|si|Poslovitch|CC0|$COMMONS/3/31/LL-Q150_%28fra%29-Poslovitch-si.wav|b00ea00005f2a6f7b81c5b99bda3806778f015e1e0d701b1fee6aba187ddfc02
fr-oui-grandcelinien|fr|oui|GrandCelinien|CC BY-SA 4.0|$COMMONS/0/04/LL-Q150_%28fra%29-GrandCelinien-oui.wav|dfd1dc4b338341933cac7a4c6f8c05336b2766d43983d379115018030e37c610
fr-non-grandcelinien|fr|non|GrandCelinien|CC BY-SA 4.0|$COMMONS/0/0b/LL-Q150_%28fra%29-GrandCelinien-non.wav|cbe51cbebaca5626de90c1736db212437bf486f686d388c166425ee2a630400a
fr-si-grandcelinien|fr|si|GrandCelinien|CC BY-SA 4.0|$COMMONS/5/54/LL-Q150_%28fra%29-GrandCelinien-si.wav|6e765b005d85a91cfd843c5656f87eb519aca2bcd1fc817c1a43d7ce1055838c
fr-oui-lunemarine|fr|oui|LuneMarine (DSwissK)|CC BY 4.0|$COMMONS/e/e7/LL-Q150_%28fra%29-LuneMarine_%28DSwissK%29-oui.wav|0dae4b84e4b109cb75dc442e171e623b4748d8dcde98fe07f4d7d06a59b684a1
fr-non-lunemarine|fr|non|LuneMarine (DSwissK)|CC BY 4.0|$COMMONS/c/c0/LL-Q150_%28fra%29-LuneMarine_%28DSwissK%29-non.wav|d0904431804e1ff15b23231a861fc05ed2ce8d64326e0f9444410962dac9660a
fr-si-lunemarine|fr|si|LuneMarine (DSwissK)|CC BY 4.0|$COMMONS/9/94/LL-Q150_%28fra%29-LuneMarine_%28DSwissK%29-si.wav|501d79a4457ea75d79c2f40f1f5c1dc7fd2a5940c42634eaf74495936f8901fc
en-yes-grendelkhan|en|yes|Grendelkhan|CC0|$COMMONS/6/65/LL-Q1860_%28eng%29-Grendelkhan-yes.wav|de445fe67f84313552a1e6051b1147e9ec1a2a5ce9e71cda6e06b9a863c685fe
en-no-grendelkhan|en|no|Grendelkhan|CC0|$COMMONS/4/4b/LL-Q1860_%28eng%29-Grendelkhan-no.wav|dfb410b4acb9cf7cc3b046ec563c3234c0e10245914481448c163601d3d9bad2
en-no-letsgoleelee|en|no|LetsgoLeeLee|CC0|$COMMONS/e/ef/LL-Q1860_%28eng%29-LetsgoLeeLee-no.wav|6a8bfb542a3fc9cb49af9d1c6a2bac612d31d25f4f6c0d50cdcc879b6e9a7a69
"

# fetch_into NOM URL SHA-256 : le fichier sources/NOM, téléchargé dans un
# dossier neuf s'il manque ou s'il n'a pas la bonne empreinte.
fetch_into() {
    local file="$CORPUS/sources/$1"
    if [ -f "$file" ] && sha256_ok "$3" "$file"; then
        return 0
    fi
    rm -rf "$(dirname "$file")"
    mkdir -p "$(dirname "$file")"
    echo "Téléchargement : $2"
    fetch_verified "$2" "$3" "$file"
}

while IFS='|' read -r name url sha _; do
    [ -n "$name" ] || continue
    fetch_into "$name" "$url" "$sha"
done <<<"$SOURCES"
while IFS='|' read -r name _ _ _ _ url sha; do
    [ -n "$name" ] || continue
    fetch_into "lingualibre-$name/$(basename "$url")" "$url" "$sha"
done <<<"$LINGUA_LIBRE"
echo "Sources vérifiées : $(du -sk "$CORPUS/sources" | awk '{print $1}') Kio sous $CORPUS/sources"

# L'extraction, refaite à chaque fois dans un dossier neuf, puis tout en
# 16 kHz mono 16 bits (FLEURS est en flottants, Lingua Libre à 44,1/48 kHz).
EXTRAIT="$CORPUS/extrait"
rm -rf "$EXTRAIT"
mkdir -p "$EXTRAIT"
to16k() {
    afconvert -f WAVE -d LEI16@16000 -c 1 "$1" "$2"
}
for lang in fr_fr en_us; do
    raw="$EXTRAIT/fleurs-$lang-brut"
    out="$EXTRAIT/fleurs-$lang"
    mkdir -p "$raw" "$out"
    tar -xzf "$CORPUS/sources/fleurs-$lang-audio/dev.tar.gz" -C "$raw"
    for wav in "$raw"/dev/*.wav; do
        to16k "$wav" "$out/$(basename "$wav")"
    done
    rm -rf "$raw"
done
mkdir -p "$EXTRAIT/demand"
unzip -q -j "$CORPUS/sources/demand-pcafeter/PCAFETER_16k.zip" PCAFETER/ch01.wav -d "$EXTRAIT/demand"
mkdir -p "$EXTRAIT/lingualibre"
while IFS='|' read -r name lang word speaker licence url _; do
    [ -n "$name" ] || continue
    to16k "$CORPUS/sources/lingualibre-$name/$(basename "$url")" "$EXTRAIT/lingualibre/$name.wav"
    printf '%s|%s|%s|%s|%s|%s\n' "$name" "$lang" "$word" "$speaker" "$licence" "$url"
done <<<"$LINGUA_LIBRE" >"$EXTRAIT/lingualibre/liste.txt"

rm -rf "$CORPUS/audios"
mkdir -p "$CORPUS/audios"
python3 -I - "$EXTRAIT" "$CORPUS/sources" "$CORPUS/audios" <<'PYTHON'
import array, json, math, os, sys, wave

extrait, sources, out = sys.argv[1:4]
RATE, FRAME = 16000, 512
TARGET = 10 ** (-23 / 20)  # -23 dBFS

def read(path):
    with wave.open(path) as w:
        assert (w.getnchannels(), w.getsampwidth(), w.getframerate()) == (1, 2, RATE), path
        pcm = array.array("h", w.readframes(w.getnframes()))
    return [x / 32768 for x in pcm]

def write(name, samples, text, info):
    pcm = array.array("h", (max(-32768, min(32767, round(x * 32768))) for x in samples))
    with wave.open(os.path.join(out, name + ".wav"), "wb") as w:
        w.setnchannels(1); w.setsampwidth(2); w.setframerate(RATE)
        w.writeframes(pcm.tobytes())
    with open(os.path.join(out, name + ".txt"), "w", encoding="utf-8") as f:
        f.write(text + "\n")
    info["secondes"] = round(len(samples) / RATE, 2)
    manifest[name] = info

def frames_rms(s):
    return [math.sqrt(sum(x * x for x in s[i:i + FRAME]) / FRAME) for i in range(0, len(s) - FRAME + 1, FRAME)]

def active(s):
    """Les trames actives : au plus 26 dB sous la plus forte."""
    rms = frames_rms(s)
    top = max(rms, default=0.0)
    return [i for i, r in enumerate(rms) if top > 0 and r >= top * 0.05], rms

def normalized(s):
    idx, rms = active(s)
    level = math.sqrt(sum(rms[i] ** 2 for i in idx) / len(idx))
    gain = TARGET / level
    peak = max(abs(x) for x in s) * gain
    if peak > 0.99:
        gain *= 0.99 / peak
    return [x * gain for x in s]

def trimmed(s):
    idx, _ = active(s)
    keep = int(0.1 * RATE)
    return s[max(0, idx[0] * FRAME - keep):min(len(s), (idx[-1] + 1) * FRAME + keep)]

def pauses(s):
    """Les silences d'au moins 0,416 s (13 trames sous l'actif), en secondes."""
    idx, rms = active(s)
    on = set(idx)
    runs, run = [], 0
    for i in range(len(rms)):
        if i in on:
            if run >= 13:
                runs.append(round(run * FRAME / RATE, 2))
            run = 0
        else:
            run += 1
    return runs

def rows(lang):
    with open(os.path.join(sources, f"fleurs-{lang}-tsv", "dev.tsv"), encoding="utf-8") as f:
        for line in f:
            sid, name, raw, _norm, _chars, _n, gender = line.rstrip("\n").split("\t")
            yield sid, name, raw, gender

manifest = {}
used = {"fr_fr": set(), "en_us": set()}
cache = {}

def utterance(lang, name):
    key = (lang, name)
    if key not in cache:
        cache[key] = normalized(read(os.path.join(extrait, f"fleurs-{lang}", name)))
    return cache[key]

def pick(lang, gender, seconds, trim):
    """Des phrases de `gender`, dans l'ordre du .tsv, une par phrase du texte
    (identifiant), jamais déjà prises, jusqu'à `seconds`."""
    picked, total = [], 0.0
    for sid, name, raw, g in rows(lang):
        if g != gender or sid in used[lang] or total >= seconds:
            continue
        s = utterance(lang, name)
        s = trimmed(s) if trim else s
        used[lang].add(sid)
        picked.append((name, raw, s))
        total += len(s) / RATE + (0.15 if trim else 0.0)
    assert total >= seconds, (lang, gender, total)
    return picked

def joined(picked, gap):
    audio, texts = [], []
    for _, raw, s in picked:
        if audio:
            audio.extend([0.0] * int(gap * RATE))
        audio.extend(s)
        texts.append(raw)
    return audio, " ".join(texts)

def fleurs_info(lang, picked, how):
    return {"source": f"FLEURS {lang} dev (CC-BY-4.0)", "fichiers": [p[0] for p in picked], "construction": how}

for lang, name in (("fr_fr", "fr-continu"), ("en_us", "en-continu")):
    picked = pick(lang, "FEMALE", 45, trim=True)
    audio, text = joined(picked, 0.15)
    info = fleurs_info(lang, picked, "silences de tête et de queue retirés, 0,15 s de silence numérique entre deux phrases")
    info["pauses_de_plus_de_0_416_s"] = pauses(audio)
    write(name, audio, text, info)

picked = pick("fr_fr", "MALE", 45, trim=False)
pauses_audio, pauses_text = joined(picked, 0.0)
info = fleurs_info("fr_fr", picked, "phrases entières, bout à bout, leurs silences gardés")
info["pauses_de_plus_de_0_416_s"] = pauses(pauses_audio)
write("fr-pauses", pauses_audio, pauses_text, info)

picked = pick("fr_fr", "MALE", 45, trim=False)
speech, text = joined(picked, 0.0)
noise = read(os.path.join(extrait, "demand", "ch01.wav"))
assert len(noise) >= len(speech)
idx, rms = active(speech)
speech_level = math.sqrt(sum(rms[i] ** 2 for i in idx) / len(idx))
noise_level = math.sqrt(sum(x * x for x in noise[:len(speech)]) / len(speech))
gain = speech_level / noise_level * 10 ** (-10 / 20)
mixed = [s + n * gain for s, n in zip(speech, noise)]
info = fleurs_info("fr_fr", picked, "phrases entières mêlées au bruit, 10 dB sous le niveau actif de la parole")
info["bruit"] = "DEMAND PCAFETER ch01.wav, ses premières secondes (CC-BY-4.0)"
write("fr-bruit-10db", mixed, text, info)

loop, texts = [], []
while len(loop) < 600 * RATE:
    loop.extend(pauses_audio)
    texts.append(pauses_text)
write("fr-boucle-10min", loop, " ".join(texts), {"source": "fr-pauses", "construction": f"fr-pauses répété {len(texts)} fois"})

# Le plus lent : le moins de mots par seconde de parole (trames actives).
slowest = None
for sid, name, raw, g in rows("fr_fr"):
    s = utterance("fr_fr", name)
    idx, _ = active(s)
    speaking = len(idx) * FRAME / RATE
    rate = len(raw.split()) / speaking
    if slowest is None or rate < slowest[0]:
        slowest = (rate, name, raw, s)
rate, name, raw, s = slowest
write("fr-lent", s + [0.0] * RATE, raw, {"source": "FLEURS fr_fr dev (CC-BY-4.0)", "fichiers": [name],
      "mots_par_seconde_de_parole": round(rate, 2), "construction": "la phrase la plus lente du dev, entière, 1 s de silence numérique derrière"})

with open(os.path.join(extrait, "lingualibre", "liste.txt"), encoding="utf-8") as f:
    for line in f:
        name, lang, word, speaker, licence, url = line.rstrip("\n").split("|")
        s = read(os.path.join(extrait, "lingualibre", name + ".wav"))
        write("mot-" + name, [0.0] * RATE + s + [0.0] * (RATE * 3 // 2), word,
              {"source": f"Lingua Libre, {speaker} ({licence})", "url": url, "langue": lang,
               "construction": "1 s de silence numérique, le mot, 1,5 s de silence numérique"})

with open(os.path.join(out, "corpus.json"), "w", encoding="utf-8") as f:
    json.dump(manifest, f, ensure_ascii=False, indent=1)
for name, info in manifest.items():
    print(f"{name:28} {info['secondes']:7.2f} s  {info.get('pauses_de_plus_de_0_416_s', '')}")
PYTHON
echo "Corpus prêt : $CORPUS/audios"
