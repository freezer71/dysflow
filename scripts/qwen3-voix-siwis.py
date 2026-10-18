#!/usr/bin/env python3
"""La voix française fixe de Qwen3-TTS 0.6B Base, livrée avec Sillage.

Le x-vector de l'extrait SIWIS expr_chap_s01_0011 (CC-BY-4.0, Idiap / U.
d'Édimbourg, https://doi.org/10.7488/ds/1705), calculé une fois par
l'encodeur de locuteur du modèle (mlx-audio 0.5.8, Python), écrit dans
assets/voices/qwen3-06b-siwis.safetensors (clé speaker_embedding, 1 024
flottants) avec sa provenance (qwen3-06b-siwis.json). Le port Swift le met
à la place du locuteur (mode x-vector) : une voix fixe, pas un clonage.

L'extrait est préparé comme au banc d'écoute (prep_refs.py) : 24 kHz mono,
fondu de 30 ms à la fin, RMS à -23 dBFS, écrêtage à +-0,98, écrit en WAV
16 bits puis relu par mlx_audio.utils.load_audio (le chemin du banc).

L'extrait vient de l'archive SiwisFrenchSpeechSynthesisDatabase.zip du DOI,
fichier SiwisFrenchSpeechSynthesisDatabase/wavs/part5/expr_chap_s01_0011.wav
(761 502 octets, CRC32 22997c5a) ; le script refuse tout autre fichier.

    HF_HOME=<cache> ~/Library/Caches/sillage-bench/eval/venv/bin/python -I -B \\
      scripts/qwen3-voix-siwis.py <dossier>/expr_chap_s01_0011.wav
"""
import hashlib
import importlib.metadata
import json
import os
import sys
import tempfile

import librosa
import mlx.core as mx
import numpy as np
import soundfile as sf
from huggingface_hub import snapshot_download
from mlx_audio.tts.utils import load_model
from mlx_audio.utils import load_audio

REPO = "mlx-community/Qwen3-TTS-12Hz-0.6B-Base-8bit"
REVISION = "50f45ef0047cde7e84c2ef04326acb8ada2436a7"
MLX_AUDIO = "0.5.8"
SOURCE_SHA256 = "54f7cef4cc995d718fcadcdfe5b3c69c2a7563d195e00a77fa72df2ecfe980dc"
# Le texte exact de text/part5/expr_chap_s01_0011.txt (apostrophes typographiques).
TEXT = ("Seulement, Conseil avait un défaut. Formaliste enragé, il ne me parlait "
        "jamais qu’à la troisième personne, au point d’en être agaçant.")
OUT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)),
                                    "..", "assets", "voices", "qwen3-06b-siwis"))
SR = 24000


def sha256(path):
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()


def prepare(wav, tmp):
    """La préparation de prep_refs.py (extrait < 10,2 s : pas de coupe)."""
    x, _ = librosa.load(wav, sr=SR, mono=True)
    assert len(x) / SR < 10.2, "l'extrait devrait faire moins de 10,2 s"
    fade = int(0.03 * SR)
    x[-fade:] *= np.linspace(1, 0, fade)
    x = x * (10 ** (-23 / 20) / (np.sqrt(np.mean(x ** 2)) + 1e-9))
    x = np.clip(x, -0.98, 0.98)
    path = os.path.join(tmp, "ref.wav")
    sf.write(path, x, SR)  # PCM 16 bits, comme au banc
    return load_audio(path, sample_rate=SR)


def embed(model, audio):
    return np.array(model.extract_speaker_embedding(audio).astype(mx.float32)).reshape(-1)


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    wav = sys.argv[1]
    if sha256(wav) != SOURCE_SHA256:
        sys.exit(f"{wav} n'est pas l'extrait SIWIS attendu (sha256 {SOURCE_SHA256})")
    version = importlib.metadata.version("mlx-audio")
    if version != MLX_AUDIO:
        sys.exit(f"mlx-audio {version} : la recette est mesurée avec {MLX_AUDIO}")
    model = load_model(snapshot_download(REPO, revision=REVISION))
    with tempfile.TemporaryDirectory() as tmp:
        audio = prepare(wav, tmp)
    first, again = embed(model, audio), embed(model, audio)
    assert first.shape == (1024,), first.shape
    assert np.all(np.isfinite(first)), "valeur non finie"
    assert np.array_equal(first, again), "l'encodeur de locuteur doit être déterministe"
    mx.save_safetensors(OUT + ".safetensors", {"speaker_embedding": mx.array(first)})
    provenance = {
        "name": "siwis",
        "model": f"{REPO}@{REVISION}",
        "source": "SIWIS French Speech Synthesis Database (J. Yamagishi, P.-E. Honnet, "
                  "P. N. Garner, A. Lazaridis, 2017, University of Edinburgh CSTR / Idiap, "
                  "https://doi.org/10.7488/ds/1705), SiwisFrenchSpeechSynthesisDatabase.zip, "
                  "wavs/part5/expr_chap_s01_0011.wav (lecture expressive, locutrice de la partie 5)",
        "license": "CC-BY-4.0 (https://creativecommons.org/licenses/by/4.0/)",
        "source_sha256": SOURCE_SHA256,
        "text": TEXT,
        "preparation": "24 kHz mono, fondu 30 ms en fin, RMS -23 dBFS, écrêtage 0,98, "
                       "WAV 16 bits relu par mlx_audio.utils.load_audio (prep_refs.py)",
        "method": f"mlx-audio {MLX_AUDIO}, Qwen3TTS.extract_speaker_embedding (mode x-vector)",
        "vector_sha256": sha256(OUT + ".safetensors"),
        "evaluation": "rapport de banc local, non versionné (.superpowers/research/eval/rapport.md §8, mesuré sur le 1.7B Base) : non livré avec le dépôt",
    }
    with open(OUT + ".json", "w", encoding="utf-8") as f:
        json.dump(provenance, f, ensure_ascii=False, indent=2)
        f.write("\n")
    print(OUT + ".safetensors", float(np.linalg.norm(first)))


if __name__ == "__main__":
    main()
