#!/usr/bin/env python3
"""Référence de parité du port Swift de Kokoro (macos/Vendor/KokoroMLX).

Fait parler mlx-audio Python 0.5.8, la chaîne du banc d'écoute, sur une
phrase courte, graine 7, et écrit macos/Tests/Fixtures/kokoro-parite.json :
identifiants, ligne de style, durées et audio (16 bits). Le test
`KokoroParityTests` rejoue les mêmes entrées dans le port Swift.

    HF_HOME=<cache hf> <python avec mlx-audio 0.5.8> scripts/kokoro-parite.py <voix p-fenrir60.safetensors>
"""
import base64, json, os, sys
import numpy as np, mlx.core as mx
from mlx_audio.tts.utils import load_model
from mlx_audio.tts.models.kokoro.pipeline import KokoroPipeline

REPO = "mlx-community/Kokoro-82M-bf16"
TEXT = "Ce matin, le marché était plein."
OUT = os.path.join(os.path.dirname(__file__), "..", "macos", "Tests", "Fixtures", "kokoro-parite.json")

voice = sys.argv[1]
model = load_model(REPO)
pipe = KokoroPipeline(lang_code="f", model=model, repo_id=REPO)
pack = pipe.load_voice(voice)
ps = "".join(r.phonemes for r in pipe(TEXT, voice=voice))
ids = [0] + [model.vocab[p] for p in ps if p in model.vocab] + [0]
style = np.array(pack[len(ps) - 1]).reshape(-1)
mx.random.seed(7)
out = model(ps, mx.array(style[None]), 1, return_output=True)
audio = np.clip(np.array(out.audio).reshape(-1) * 32767, -32768, 32767).round().astype("<i2")
json.dump({
    "source": f"mlx-audio 0.5.8 (Python), {REPO}, voix p-fenrir60, graine 7",
    "text": TEXT, "phonemes": ps, "ids": ids,
    "style": [float(x) for x in style],
    "durations": np.array(out.pred_dur).astype(int).tolist(),
    "audio_i16_base64": base64.b64encode(audio.tobytes()).decode(),
}, open(OUT, "w"), ensure_ascii=False)
print(ps, len(audio))
