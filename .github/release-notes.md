Lecteur à voix haute fait pour être suivi des yeux : markdown, PDF et images
(reconnaissance de texte), lus par Kokoro, sur votre machine.

## Installer

**Mac (puce Apple) et Linux (x86_64, ARM64)**, une seule commande :

```bash
curl -fsSL https://raw.githubusercontent.com/freezer71/sillon/main/install.sh | sh
```

Elle choisit l'archive de votre système, vérifie son empreinte SHA-256 et
installe sans `sudo` : l'app dans `/Applications` et la commande `sillon` sur
Mac, le lecteur au terminal dans `~/.local` sous Linux.

**Ou, sur Mac, le `.dmg`** ci-dessous : l'ouvrir, glisser Sillon dans
Applications. L'app n'est pas encore notariée par Apple : au premier
lancement, macOS la bloque. Ouvrez *Réglages Système › Confidentialité et
sécurité* et cliquez sur *Ouvrir quand même* (la commande ci-dessus évite ce
détour).

**La voix Kokoro se télécharge toute seule** au premier lancement ; la voix du
système lit en attendant.

## Prérequis

- Mac : puce Apple (M1 et suivants), macOS 14 ou plus ; la dictée demande
  macOS 26 ;
- Linux : glibc 2.39 ou plus (Ubuntu 24.04, Debian 13, Fedora 40 ou plus
  récents), ALSA (`libasound2`).

## Désinstaller

```bash
curl -fsSL https://raw.githubusercontent.com/freezer71/sillon/main/install.sh | sh -s -- --desinstaller
```

Chaque archive a son empreinte SHA-256 (`.sha256`).
