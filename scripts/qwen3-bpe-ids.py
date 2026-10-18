#!/usr/bin/env python3
"""Ids de référence de l'encodeur BPE de Qwen3-TTS (macos/Vendor/KokoroMLX/
TTS/Qwen3TTS/Qwen2BPETokenizer.swift), par transformers.AutoTokenizer.

Télécharge vocab.json, merges.txt et tokenizer_config.json du dépôt
mlx-community/Qwen3-TTS-12Hz-0.6B-Base-8bit (révision figée ; mêmes fichiers
dans le 1.7B CustomVoice), les copie dans macos/Tests/Fixtures (qwen3-*), et
écrit macos/Tests/Fixtures/qwen3-bpe.json : les ids de chaque phrase.

    HF_HOME=~/Library/Caches/sillage-bench/eval/hf \\
      ~/Library/Caches/sillage-bench/eval/venv/bin/python -I scripts/qwen3-bpe-ids.py
"""
import json, os, shutil, tempfile
import transformers
from huggingface_hub import hf_hub_download

REPO = "mlx-community/Qwen3-TTS-12Hz-0.6B-Base-8bit"
REVISION = "50f45ef0047cde7e84c2ef04326acb8ada2436a7"
FILES = ["vocab.json", "merges.txt", "tokenizer_config.json"]
FIXTURES = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "macos", "Tests", "Fixtures")
TEXTS = [
    "Bonjour.",
    "Ce matin, le marché était plein.",
    "L’été dernier, j’ai lu « Vingt mille lieues sous les mers ».",
    "Qu'on m'excuse de dire ainsi que j'avais quarante ans.",
    "Il est arrivé en 2027, à 14 h 30… enfin, presque.",
    "Prix : 3,14 € ; remise de 15 %.",
    "Où ? Là-bas ! Déjà ?",
    "Œuvre, cœur, ægipan, naïf, Noël.",
    # NFD (accents combinants, comme dans certains PDF).
    "été en forme décomposée",
    "Je me vêtis chaudement, bottes de mer, bonnet de loutre.",
    "« Allons-y », dit-il — sans attendre.",
    "C’est-à-dire : l’opération n’a pas réussi.",
    "ÇA VA ? Très bien, MERCI.",
    "deux mille vingt-sept",
    "Hello, world!",
    "It's a beautiful day, isn't it? We'll see.",
    "The quick brown fox jumps over 13 lazy dogs.",
    "She said: \"don't\" — and left.",
    "Mr. O'Neil's café opened in 1999.",
    "  deux  espaces  et une tabulation\tici.",
    "Ligne un\nLigne deux",
    "Emoji : 🙂 et symbole ≈.",
    "<|im_start|>assistant\n",
    "<|im_end|>\n<|im_start|>assistant\n",
    "<|im_start|>assistant\nBonjour.<|im_end|>\n<|im_start|>assistant\n",
]


def main():
    work = tempfile.mkdtemp()
    for name in FILES:
        src = hf_hub_download(REPO, name, revision=REVISION)
        shutil.copy(src, os.path.join(work, name))
        shutil.copy(src, os.path.join(FIXTURES, "qwen3-" + name))
    tok = transformers.AutoTokenizer.from_pretrained(work)
    cases = [{"text": t, "ids": tok.encode(t, add_special_tokens=False)} for t in TEXTS]
    assert len(cases[-3]["ids"]) == 3, cases[-3]
    assert len(cases[-2]["ids"]) == 5, cases[-2]
    json.dump({
        "source": f"transformers {transformers.__version__}, AutoTokenizer, {REPO}@{REVISION}",
        "cases": cases,
    }, open(os.path.join(FIXTURES, "qwen3-bpe.json"), "w"), ensure_ascii=False, indent=1)
    print(type(tok).__name__, len(cases), "phrases")


if __name__ == "__main__":
    main()
