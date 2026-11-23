#!/bin/sh
# Les deux dictées des fixtures de Parakeet (tâche 14b), parlées par Kokoro :
#   macos/Tests/Fixtures/dictee-charge.wav : l'entrée « francais » (et ses
#     dérivées « douze_secondes », « dixieme ») de tests/fixtures/mlx/
#     parakeet_tdt_v{3,2}.json, et la dictée de macos/Tests/MLXLoadTests.swift ;
#   tests/fixtures/mlx/dictee-en.wav : l'entrée « anglais ».
#
# Moteur : Kokoro-82M en MLX, le calcul Rust de Dysflow (src/tts/kokoro/mlx,
# mlx-rs 0.34.0, mlx-sys 0.7.0, MLX v0.32.2, noyaux mlx.metallib de mlx-sys
# précompilés : le build standard), par examples/kokoro_dictees.rs.
# L'exemple suit le chemin de `dysflow --output` :
#   - configuration et voix (export::apply_voice) ;
#   - document et phrases ;
#   - KokoroEngine : phonèmes par dysflow-espeak (espeak-ng 1.52.0,
#     scripts/build-espeak.sh), style par longueur ;
#   - export::render au débit 1.
# Une seule différence : chaque passage prend la graine 7
# (MlxBackend::infer_seeded, celle de kokoro-parite.json). La graine du
# produit vient de l'horloge, et deux rendus y diffèrent.
#
# Paquet : mlx-community/Kokoro-82M-bf16, révision
# a71e4d38b236d968966a2002c4c895dbd12b1c3c (kokoro-v1_0.safetensors,
# config.json, voices/), le paquet kokoro_82m_mlx de src/models.rs.
# Licence : Apache-2.0, poids et voix. Sources :
#   - la carte du paquet, https://huggingface.co/mlx-community/Kokoro-82M-bf16
#     (README.md à cette révision : « license: apache-2.0 ») ;
#   - celle du modèle d'origine, https://huggingface.co/hexgrad/Kokoro-82M
#     (« license: apache-2.0 », « Apache-licensed weights ») : les voix sont
#     des fichiers de ce dépôt.
# Voir aussi NOTICE.
#
# Les textes, la voix, le débit :
#   fr : « N'oublie pas d'envoyer le devis au client avant la fin de la
#        semaine. », voix ff_siwis, débit 1 ;
#   en : « Please send the weekly report to the whole team before Friday. »,
#        voix af_heart, débit 1.
#
# Kokoro rend du 24 kHz. L'exemple l'écrit en flottant 32 bits, sans
# quantification. afconvert (outil de macOS ; ici macOS 27.2, 26B123) le
# ramène en WAV mono 16 kHz, PCM 16 bits :
#   afconvert -f WAVE -d LEI16@16000 -c 1 -r 127 --src-complexity bats <24k> <16k>
# Le WAV écrit a un fmt WAVE_FORMAT_EXTENSIBLE et un chunk FLLR, que les
# lecteurs des essais sautent.
#
# Empreintes (macOS 27.2, Apple M5) :
#   dictee-charge.wav : fichier SHA-256 23781dd70d485faf21d078a9849c2f3ae94754a5a42788afe28d129767fd675b ;
#     67 600 échantillons, SHA-256 des f32 (petit boutiste, /32 768)
#     1cac739221e63f2025a2a22b464222adadf4dc6575393b5df73026fef7278777
#   dictee-en.wav : fichier SHA-256 d2dfe33c8a58f4da3485297ae4dc197538e1bbafbfad6529928a784d4b7deefb ;
#     62 400 échantillons, SHA-256 des f32 71f83a83dd2fd5a54c16b3d9676bf7d1a244f21d50966d8efbb88fb5f1ac52b9
# Les essais vérifient l'empreinte des f32 (ParakeetFixtureTests.swift,
# src/stt/mlx/parakeet/tests.rs) et celle du fichier français
# (MLXLoadTests.swift).
#
# Deux lancements de suite rendent les mêmes octets. Une autre version de
# macOS (afconvert), de MLX ou de ses noyaux, ou d'espeak-ng peut en rendre
# d'autres. Il faut alors refiger :
#   TEST_RUNNER_DYSFLOW_WRITE_FIXTURES=1 … ./scripts/test-macos.sh ParakeetFixtureTests
# ainsi que l'empreinte et le texte de MLXLoadTests.
#
# Usage, depuis n'importe où :
#   DYSFLOW_KOKORO_DIR=<dossier du paquet> tests/fixtures/mlx/make_dictees.sh [racine]
# [racine] : où écrire les deux fichiers, sous leurs chemins du dépôt. Par
# défaut, le dépôt lui-même. Un autre dossier sert à vérifier que deux
# rendus sont identiques.
#
# Rien ne joue de son. Le HOME est temporaire : les réglages et la
# bibliothèque de l'utilisateur ne sont jamais lus.
set -eu
cd "$(dirname "$0")/../../.."
repo=$PWD
root=${1:-$repo}
kokoro=${DYSFLOW_KOKORO_DIR:?DYSFLOW_KOKORO_DIR : le dossier de Kokoro (kokoro-v1_0.safetensors, config.json, voices/)}
espeak=$repo/target/espeak
[ -x "$espeak/bin/dysflow-espeak" ] || { echo "dysflow-espeak absent : scripts/build-espeak.sh" >&2; exit 1; }

# shellcheck source=scripts/mlx-sources.sh
. scripts/mlx-sources.sh
cargo build --release --locked --features mlx --example kokoro_dictees
example=$repo/target/release/examples/kokoro_dictees

tmp=$(mktemp -d "${TMPDIR:-/tmp}/dictees.XXXXXX")
trap 'rm -rf "$tmp"' EXIT
mkdir "$tmp/home"

# dictee <langue> <voix> <texte> <chemin dans le dépôt>
dictee() {
    env -i PATH=/usr/bin:/bin HOME="$tmp/home" TMPDIR="$tmp/" \
        DYSFLOW_MLX_METALLIB="$DYSFLOW_MLX_METALLIB" \
        DYSFLOW_ESPEAK="$espeak/bin/dysflow-espeak" DYSFLOW_ESPEAK_DATA="$espeak/share" \
        "$example" render "$kokoro" "$1" "$2" "$3" "$tmp/$1-24k.wav"
    mkdir -p "$(dirname "$root/$4")"
    afconvert -f WAVE -d LEI16@16000 -c 1 -r 127 --src-complexity bats "$tmp/$1-24k.wav" "$root/$4"
    echo "$4 : fichier $(shasum -a 256 "$root/$4" | cut -d' ' -f1)"
    echo "$4 : $("$example" sha "$root/$4")"
}

dictee fr ff_siwis "N'oublie pas d'envoyer le devis au client avant la fin de la semaine." \
    macos/Tests/Fixtures/dictee-charge.wav
dictee en af_heart "Please send the weekly report to the whole team before Friday." \
    tests/fixtures/mlx/dictee-en.wav
