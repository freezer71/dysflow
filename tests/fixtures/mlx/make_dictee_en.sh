#!/bin/sh
# Provenance de dictee-en.wav, l'entrée anglaise des fixtures de Parakeet
# (parakeet_tdt_v3.json, parakeet_tdt_v2.json ; tâche 14) : la voix système
# « Samantha » de macOS, par `say`, en WAV mono 16 kHz 16 bits.
# 52 725 échantillons, SHA-256 36bddb8bbf5d4ecf405adc59b7fd68bdab19a501572b471bf21e31d088eeabf1
# (vérifiée par les essais : macos/Tests/ParakeetFixtureTests.swift,
# src/stt/mlx/parakeet/tests.rs). Le refaire change ces empreintes : les
# fixtures seraient à refiger (scripts/test-macos.sh ParakeetFixtureTests).
#
# Licence : le contrat de licence de macOS (/Library/Documentation/License.lpdf,
# § 2 F « Voices ») ne permet les voix système que pour un usage personnel
# et non commercial, et exclut « reproduction, … recording, publishing or
# redistribution … in a profit, non-profit, public sharing or commercial
# context ». Un enregistrement de Samantha versé dans ce dépôt en relève
# sans doute ; dictee-charge.wav (`say -v Thomas`, macos/Tests/Fixtures) de
# même. À remplacer avant toute publication du dépôt, par exemple par une
# voix de Kokoro (Apache 2.0) ou un enregistrement sous CC0, fixtures
# refigées avec (rapport de la tâche 14).
set -eu
cd "$(dirname "$0")"
say -v Samantha -o dictee-en.wav --file-format=WAVE --data-format=LEI16@16000 \
    "Please send the weekly report to the whole team before Friday."
shasum -a 256 dictee-en.wav
