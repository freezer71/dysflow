#!/usr/bin/env bash
# La porte du lot 4 (tâche 10), en une commande : la qualité de la découpe et
# le détecteur de silence du Mac. Parakeet int8 par sherpa-onnx, processeur
# seul, 4 fils, audio au rythme du temps réel (examples/dictation_bench.rs),
# sur le corpus de scripts/dictee-corpus.sh : relecture complète, découpe
# Silero, découpe énergie.
#
#   DYSFLOW_PORTE_PARAKEET=<dossier parakeet_tdt_v3_int8> \
#   DYSFLOW_PORTE_SILERO=<silero_vad.onnx> \
#   ./scripts/dictee-porte.sh [<corpus>]            # la porte (35 à 40 min)
#   ... ./scripts/dictee-porte.sh --court [<corpus>] # essai : fr-pauses seul, indicatif
#
# <corpus> : le dossier de scripts/dictee-corpus.sh (target/dictee-corpus par
# défaut), qui contient audios/.
#
# Ce qui passe, chaque fois en temps réel :
#   fr-continu, en-continu, fr-pauses, fr-bruit-10db, fr-lent : relecture
#     complète, Silero, énergie ;
#   fr-boucle-10min : Silero, énergie (la relecture complète de 10 minutes,
#     sans seuil qui la demande et au coût qui croît avec la durée, seulement
#     avec DYSFLOW_PORTE_BOUCLE_COMPLETE=1) ;
#   mot-* (R37) : Silero, énergie.
#
# Au calme : charge sur 1 min < 2 et aucun swap (la règle de
# scripts/mlx-charge.sh). Ordre :
#   1. swap au départ : la porte refuse s'il y en a (attendre n'y change rien) ;
#   2. construction (release, bibliothèques de scripts/sherpa-onnx.sh), puis
#      le banc assemblé dans target/dictee-porte comme un paquet ;
#   3. attente du calme, DYSFLOW_PORTE_ATTENTE_S au plus (900 s) ; toujours
#      pas au calme : refusée ;
#   4. les passages ; chacun note la charge et le swap à son début et à sa fin ;
#   5. jugement : la mesure n'est valide que si la machine était au calme au
#      départ, au début de chaque passage (charge < 2) et sans swap apparu
#      jusqu'à la fin ; sinon « valide : non », indicatif.
# `--court` n'attend ni ne refuse : son résumé dit « indicatif ».
#
# Seuils (plan, tâche 10, étape 4), jugés pour chaque détecteur :
#   - WER ≤ relecture complète + 1 point, sur fr-continu, en-continu,
#     fr-pauses et fr-bruit-10db ;
#   - p95 du délai après coupe ≤ 1 s, sur chacun de ces audios et sur la
#     boucle ;
#   - boucle de 10 minutes : médiane du délai après coupe du dernier tiers
#     ≤ celle du premier + 0,1 s.
# Rapportés sans entrer dans le verdict : les mots seuls entendus ou non
# (R37), le locuteur lent, le délai après la fin de la parole (p50, p95), le
# calcul par seconde d'audio de chaque régime (R41), le pire retard du
# provisoire, la mémoire.
#
# Sortie : target/runs/dictee-porte-<date>/ (un JSON par passage, resume.txt),
# le résumé aussi à l'écran. Codes : 0 mesure valide (verdicts dans le
# résumé) ; 1 un passage a échoué ; 2 mauvais usage ; 3 machine pas au calme
# (refusée) ; 4 mesure faite mais invalide (à refaire au calme). `--court`
# rend 0 même indicatif.
set -euo pipefail
# Défense en profondeur : aucun ONNX Runtime lancé d'ici n'écrit ni n'envoie sa télémétrie (onnx::NO_TELEMETRY).
export ORT_DISABLE_TELEMETRY=1
cd "$(dirname "$0")/.."

MODE=porte
if [ "${1:-}" = --court ]; then
    MODE=court
    shift
fi
case "${1:-}" in
-*) echo "argument inconnu : $1 (--court, puis le dossier du corpus)" >&2; exit 2 ;;
esac
CORPUS=${1:-target/dictee-corpus}
AUDIOS="$CORPUS/audios"
PARAKEET=${DYSFLOW_PORTE_PARAKEET:-}
SILERO=${DYSFLOW_PORTE_SILERO:-}
if [ ! -f "$AUDIOS/corpus.json" ]; then
    echo "Erreur : $AUDIOS/corpus.json absent ; lancer d'abord ./scripts/dictee-corpus.sh $CORPUS" >&2
    exit 2
fi
if [ -z "$PARAKEET" ] || [ ! -f "$PARAKEET/encoder.int8.onnx" ] || [ -z "$SILERO" ] || [ ! -f "$SILERO" ]; then
    echo "Erreur : DYSFLOW_PORTE_PARAKEET (parakeet_tdt_v3_int8/) et DYSFLOW_PORTE_SILERO (silero_vad.onnx)" >&2
    exit 2
fi
AUDIOS=$(cd "$AUDIOS" && pwd -P)
PARAKEET=$(cd "$PARAKEET" && pwd -P)
SILERO="$(cd "$(dirname "$SILERO")" && pwd -P)/$(basename "$SILERO")"

# La machine : « c1 c5 c15 swap_mo ».
lecture() {
    local swap
    swap=$(LC_ALL=C sysctl -n vm.swapusage | sed -E 's/.*used = ([0-9.]+)M.*/\1/')
    echo "$(LC_ALL=C sysctl -n vm.loadavg | tr -d '{}' | awk '{print $1, $2, $3}') $swap"
}
au_calme() {
    awk -v c="$1" -v s="$2" 'BEGIN { exit !(c < 2 && s == 0) }'
}
dire() {
    # shellcheck disable=SC2086 # découpage voulu
    set -- $1
    echo "charge $1 $2 $3, swap $4 Mo"
}

DEPART=$(lecture)
# shellcheck disable=SC2086
set -- $DEPART
if [ "$MODE" = porte ] && ! awk -v s="$4" 'BEGIN { exit !(s == 0) }'; then
    echo "Refusé : swap utilisé ($(dire "$DEPART")) ; il faut aucun swap au départ (redémarrer)" >&2
    exit 3
fi

if python3 -c "" >/dev/null 2>&1; then PYTHON=python3; else echo "Erreur : python3 introuvable" >&2; exit 1; fi
# shellcheck source=scripts/paquet-commun.sh
. scripts/paquet-commun.sh
# shellcheck source=scripts/sherpa-onnx.sh
. scripts/sherpa-onnx.sh
sherpa_fetch
echo "Construction du banc (release, sherpa-onnx $SHERPA_VERSION, ONNX Runtime $ORT_VERSION)…"
cargo build --release --locked --example dictation_bench --features stt-sherpa \
    --config "$SHERPA_CONFIG_SEARCH" --config "$SHERPA_CONFIG_LIB"
OUT=target/dictee-porte
rm -rf "$OUT"
mkdir -p "$OUT"
cp "target/${CARGO_BUILD_TARGET:+$CARGO_BUILD_TARGET/}release/examples/dictation_bench" "$OUT/"
cp "$SHERPA_LIB_DIR/$SHERPA_LIB" "$OUT/"
cp "$ORT_DIR/$ORT_LIB" "$OUT/$ORT_DEST"
BANC="$PWD/$OUT/dictation_bench"

if [ "$MODE" = porte ]; then
    attente=${DYSFLOW_PORTE_ATTENTE_S:-900}
    depuis=$(date +%s)
    DEPART=$(lecture)
    # shellcheck disable=SC2086
    set -- $DEPART
    while ! au_calme "$1" "$4"; do
        if [ $(($(date +%s) - depuis)) -ge "$attente" ]; then
            echo "Refusé : pas au calme après la construction ($(dire "$DEPART"), $attente s d'attente)" >&2
            exit 3
        fi
        sleep 15
        DEPART=$(lecture)
        # shellcheck disable=SC2086
        set -- $DEPART
    done
else
    DEPART=$(lecture)
fi

SORTIE=target/runs/dictee-porte-$(date +%Y%m%d-%H%M%S)
mkdir -p "$SORTIE"
SORTIE=$(cd "$SORTIE" && pwd -P)
echo "Passages ($MODE), relevés dans ${SORTIE}…"

# passage AUDIO RÉGIME : RÉGIME = complete, silero ou energie.
passage() {
    local audio=$1 regime=$2 lang=fr args=()
    case "$audio" in en-* | mot-en-*) lang=en ;; esac
    case "$regime" in
    complete) args=(--full) ;;
    silero) args=(--detector silero --silero "$SILERO") ;;
    energie) args=(--detector energy) ;;
    esac
    echo "  $audio, $regime"
    local json="$SORTIE/$audio.$regime.json" err="$SORTIE/$audio.$regime.err"
    ( cd / && exec env -u DYSFLOW_ORT_LIB "$BANC" --model-dir "$PARAKEET" --runtime ort-cpu \
        --threads 4 --language "$lang" --realtime "${args[@]}" \
        --wav "$AUDIOS/$audio.wav" --ref "$AUDIOS/$audio.txt" ) >"$json" 2>"$err" &
    local pid=$! debut fini="" code=0
    debut=$(date +%s)
    # Échéance : trois fois l'audio et 2 min. Une sortie bloquée après le
    # JSON écrit (vu le 23 nov. 2026 : ONNX Runtime 1.31 tourne sans fin dans
    # OneDsTelemetry::Shutdown, à la sortie du processus) est arrêtée 30 s
    # après le JSON et notée dans sorties-bloquees.txt : la mesure est faite,
    # mais le processus qui tourne chargerait le passage suivant.
    local limite=$(( $(wc -c <"$AUDIOS/$audio.wav") * 3 / 32000 + 120 ))
    while kill -0 "$pid" 2>/dev/null; do
        if [ -z "$fini" ] && grep -q '}$' "$json" 2>/dev/null; then
            fini=$(date +%s)
        fi
        if [ -n "$fini" ] && [ $(($(date +%s) - fini)) -ge 30 ]; then
            echo "$audio $regime : sortie bloquée 30 s après le JSON, processus arrêté" | tee -a "$SORTIE/sorties-bloquees.txt"
            kill "$pid"; sleep 1; kill -9 "$pid" 2>/dev/null || true
            wait "$pid" 2>/dev/null || true
            return 0
        fi
        if [ $(($(date +%s) - debut)) -ge "$limite" ]; then
            kill "$pid"; sleep 1; kill -9 "$pid" 2>/dev/null || true
            wait "$pid" 2>/dev/null || true
            echo "Erreur : le passage $audio, $regime dépasse $limite s, arrêté" >&2
            exit 1
        fi
        sleep 0.5
    done
    wait "$pid" || code=$?
    if [ "$code" != 0 ]; then
        cat "$err" >&2
        echo "Erreur : le passage $audio, $regime a échoué (code $code)" >&2
        exit 1
    fi
}

if [ "$MODE" = court ]; then
    for regime in complete silero energie; do passage fr-pauses "$regime"; done
else
    for audio in fr-continu en-continu fr-pauses fr-bruit-10db fr-lent; do
        for regime in complete silero energie; do passage "$audio" "$regime"; done
    done
    for audio in "$AUDIOS"/mot-*.wav; do
        audio=$(basename "$audio" .wav)
        for regime in silero energie; do passage "$audio" "$regime"; done
    done
    if [ "${DYSFLOW_PORTE_BOUCLE_COMPLETE:-}" = 1 ]; then
        passage fr-boucle-10min complete
    fi
    for regime in silero energie; do passage fr-boucle-10min "$regime"; done
fi
FIN=$(lecture)

code=0
"$PYTHON" -I - "$SORTIE" "$MODE" "$DEPART" "$FIN" <<'PYTHON' | tee "$SORTIE/resume.txt" || code=$?
import glob, json, os, sys

out, mode, depart, fin = sys.argv[1:5]
runs = {}
for path in sorted(glob.glob(os.path.join(out, "*.json"))):
    audio, regime, _ = os.path.basename(path).rsplit(".", 2)
    with open(path, encoding="utf-8") as f:
        runs[(audio, regime)] = json.load(f)

def calm(load, swap):
    return load is not None and swap is not None and load < 2 and swap == 0

reasons = []
d = [float(x) for x in depart.split()]
f = [float(x) for x in fin.split()]
if not calm(d[0], d[3]):
    reasons.append(f"au départ : charge {d[0]}, swap {d[3]} Mo")
for (audio, regime), r in runs.items():
    start = r["machine"]["debut"]
    if not calm((start["charge"] or [None])[0], start["swap_mo"]):
        reasons.append(f"au début de {audio}, {regime} : charge {start['charge']}, swap {start['swap_mo']} Mo")
if f[3] != 0:
    reasons.append(f"à la fin : swap {f[3]} Mo")
valid = not reasons

def pct(x):
    return "—" if x is None else f"{x * 100:.1f} %"

def s(x):
    return "—" if x is None else f"{x:.3f} s"

print(f"Porte du lot 4 ({mode}) ; mesure valide : " + ("oui" if valid else "non (" + " ; ".join(reasons) + ") : indicatif"))
print(f"départ : charge {d[0]} {d[1]} {d[2]}, swap {d[3]} Mo ; fin : charge {f[0]} {f[1]} {f[2]}, swap {f[3]} Mo")
print()
print(f"{'audio':26} {'régime':9} {'WER':>7} {'coupe p50':>10} {'p95':>8} {'fin parole p50':>15} {'p95':>8} {'calcul/s':>9} {'retard prov.':>12} {'RSS Mo':>7}")
for (audio, regime), r in sorted(runs.items()):
    cut = r["delai_apres_coupe_s"] or {}
    end = (r["delai_apres_fin_de_parole_s"] or {}).get("mesure") or {}
    print(f"{audio:26} {regime:9} {pct(r['wer']):>7} {s(cut.get('p50')):>10} {s(cut.get('p95')):>8} "
          f"{s(end.get('p50')):>15} {s(end.get('p95')):>8} {r['facteur_temps_reel']:>9.3f} "
          f"{s(r['pire_retard_provisoire_s']):>12} {r['rss_pic_mo'] or '—':>7}")
print()

QUALITY = ["fr-continu", "en-continu", "fr-pauses", "fr-bruit-10db"]
LOOP = "fr-boucle-10min"
verdicts = {}
for det in ("silero", "energie"):
    fails, missing = [], []
    for audio in QUALITY:
        full, seg = runs.get((audio, "complete")), runs.get((audio, det))
        if not full or not seg:
            missing.append(audio)
            continue
        if seg["wer"] > full["wer"] + 0.01 + 1e-9:
            fails.append(f"{audio} : WER {pct(seg['wer'])} > relecture {pct(full['wer'])} + 1 point")
    for audio in QUALITY + [LOOP]:
        seg = runs.get((audio, det))
        if not seg:
            if audio not in missing:
                missing.append(audio)
            continue
        p95 = seg["delai_apres_coupe_s"]["p95"]
        if p95 is None or p95 > 1.0:
            fails.append(f"{audio} : p95 après coupe {s(p95)} > 1 s")
    loop = runs.get((LOOP, det))
    if loop:
        first, last = loop["delai_par_tiers_s"]["premier"]["p50"], loop["delai_par_tiers_s"]["dernier"]["p50"]
        if first is None or last is None or last > first + 0.1:
            fails.append(f"boucle : dernier tiers {s(last)} > premier {s(first)} + 0,1 s")
        print(f"{det} boucle : médiane du premier tiers {s(first)}, du dernier {s(last)} ; "
              f"p95 {s(loop['delai_par_tiers_s']['premier']['p95'])} puis {s(loop['delai_par_tiers_s']['dernier']['p95'])}")
    if missing:
        verdicts[det] = "NON JUGÉE (passages absents : " + ", ".join(missing) + ")"
    elif fails:
        verdicts[det] = "ÉCHOUÉE : " + " ; ".join(fails)
    else:
        verdicts[det] = "PASSÉE"
print()
for det, v in verdicts.items():
    print(f"Verdict {det} : {v}" + ("" if valid else " (indicatif)"))
if mode == "porte" and "PASSÉE" in verdicts.get("energie", "") and valid:
    print("=> le détecteur d'énergie suffit au Mac (D5)")
elif mode == "porte" and valid and verdicts.get("energie", "").startswith("ÉCHOUÉE"):
    print("=> énergie en échec : Silero à porter en mlx-rs (tâche 14)")

print()
print("Mots seuls (R37), entendus par détecteur :")
for det in ("silero", "energie"):
    words = [(a, r) for (a, reg), r in sorted(runs.items()) if a.startswith("mot-") and reg == det]
    if words:
        print(f"  {det} : " + ", ".join(f"{a[4:]} {'oui' if r['reference_entendue'] else 'NON'} ({r['texte']!r})" for a, r in words))
if any(a == "fr-lent" for a, _ in runs):
    print("Locuteur lent (fr-lent) : " + ", ".join(f"{reg} WER {pct(r['wer'])}" for (a, reg), r in sorted(runs.items()) if a == "fr-lent"))
print("Voyelle tenue : non mesurée (aucun enregistrement réel dans les sources)")
print("Pic MLX : null (Parakeet sherpa-onnx calcule sur le processeur, sans MLX)")
stuck = os.path.join(out, "sorties-bloquees.txt")
if os.path.exists(stuck):
    with open(stuck, encoding="utf-8") as f:
        lines = f.read().split("\n")
    print(f"Sorties bloquées (ONNX Runtime, télémétrie à la sortie) : {len([l for l in lines if l])} passage(s), voir {stuck}")
else:
    print("Sorties bloquées : aucune")
sys.exit(0 if valid or mode == "court" else 4)
PYTHON
exit "$code"
