Lecteur audio en terminal pour **Linux x86_64 et ARM64** : markdown, PDF et
images (reconnaissance de texte), lus à voix haute par Kokoro.

## Installer

```bash
arch=$(uname -m)          # x86_64 ou aarch64
tar -xzf sillon-*-linux-$arch.tar.gz
mkdir -p ~/.local/lib ~/.local/bin
rm -rf ~/.local/lib/sillon
mv sillon-*-linux-$arch ~/.local/lib/sillon
ln -sfn ../lib/sillon/sillon ~/.local/bin/sillon
sillon --check-kokoro
```

`~/.local/bin` doit être dans le `PATH`. Le binaire suit son lien pour trouver
ONNX Runtime et `sillon-espeak`, posés à côté de lui.

**La voix Kokoro (108 Mo) se télécharge toute seule** à la première lecture.

## Prérequis

- glibc 2.39 ou plus (Ubuntu 24.04, Debian 13, Fedora 40 ou plus récents) ;
- ALSA (`libasound2`), présent sur tout bureau Linux.

## Désinstaller

```bash
rm -r ~/.local/lib/sillon ~/.local/bin/sillon
```

L'app macOS n'est pas dans cette publication. Chaque archive a son empreinte
SHA-256 (`.sha256`).
