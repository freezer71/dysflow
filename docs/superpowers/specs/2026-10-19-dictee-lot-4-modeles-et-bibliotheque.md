# Dictée, lot 4 : cinq modèles de plus et une bibliothèque des modèles

19 oct. 2026 · app macOS seulement. Suit `2026-10-16-modeles-stt-tts-design.md`.

## Demande de l'utilisateur

- Proposer plus de modèles de dictée, NVIDIA en tête, et creuser les autres
  éditeurs (recherche du 19 oct. 2026, validée : « Ok, parfait »).
- Avec beaucoup de modèles, la table de la section Dictée serait surchargée :
  un **sélecteur en bibliothèque, comme pour les voix** (`VoiceLibrarySheet`).

## Modèles ajoutés (identifiants fixés ici, partagés par les deux chantiers)

| id | Nom affiché | Dépôt (MLX) | Langues | Code amont |
|---|---|---|---|---|
| `parakeet_tdt_v2` | Parakeet v2 | `mlx-community/parakeet-tdt-0.6b-v2` (CC-BY-4.0) | anglais | `Parakeet` (déjà copié) |
| `nemotron_35_asr_streaming_8bit` | Nemotron 3.5 Streaming | `mlx-community/nemotron-3.5-asr-streaming-0.6b-8bit` (OpenMDW-1.1) | 40 langues, fr compris | `NemotronASR` |
| `qwen3_asr_06b_8bit` | Qwen3-ASR 0.6B | `mlx-community/Qwen3-ASR-0.6B-8bit` (Apache 2.0) | 30 langues, détection seule | `Qwen3ASR` |
| `qwen3_asr_17b_4bit` | Qwen3-ASR 1.7B | `mlx-community/Qwen3-ASR-1.7B-4bit` (Apache 2.0) | idem | `Qwen3ASR` |
| `cohere_transcribe_8bit` | Cohere Transcribe | `mlx-community/cohere-transcribe-03-2026-mlx-8bit` (Apache 2.0) | 14 langues, la langue est imposée | `CohereTranscribe` |

Code amont : `Blaizzy/mlx-audio-swift`, `Sources/MLXAudioSTT/Models/…`, **copié**
comme au lot 2 (pas de dépendance), noté dans `macos/Vendor/KokoroMLX/ORIGINE.md`
avec le commit. Révisions des dépôts épinglées dans `src/models.rs`.

Chiffres publiés (pour la ligne et le `pace`/`summary`) :
- Nemotron : FLEURS fr ≈ 9 % de mots faux (Parakeet v3 ≈ 5 %) ; son atout est
  le **vrai flux** (tranches de 80 ms à 1,12 s, cache) : chaque bout d'audio
  n'est lu qu'une fois, au lieu de la relecture glissante de 1,5 s.
- Cohere : 5,4 % de moyenne au classement Open ASR (anglais), le plus précis.
- Qwen3-ASR : reconnaît la langue seul (96,8 % / 97,9 %).

## Chantier A — moteurs

- Rust (`src/models.rs`) : les cinq paquets, `kind: Dictation`, tailles et
  révisions relevées sur Hugging Face, licences, `NOTICE`. Parakeet v2 reprend
  `Runtime::ParakeetMlx` ; nouveaux `Runtime` pour Nemotron, Qwen3-ASR, Cohere.
- Swift : chargeurs et `LoadedSpeechModel` pour chacun, branchés là où
  Parakeet/Canary/Whisper le sont (`MLXDictationEngine`, routage).
  Nemotron : d'abord comme les autres (relecture glissante), puis en flux,
  sur le modèle de `VoxtralDictationEngine`, si le code amont le permet.
- Cohere reçoit la langue du réglage « Langue » de la dictée.
- Tests : unitaires Rust (`cargo test`) ; Swift compilé seulement en local
  (`xcodebuild build-for-testing`), **jamais `xcodebuild test`** en local
  (l'app s'ouvre et bloque le Mac) : les tests Swift tournent sur la CI.

## Chantier B — bibliothèque des modèles de dictée

- Section Dictée : la table ne montre plus que la dictée de macOS, les modèles
  installés, et celui qui se télécharge ou a échoué. Dessous, un bouton
  « Parcourir les modèles… », comme « Parcourir les voix… ».
- Feuille « Bibliothèque des modèles » (même gabarit, même style Felt, mêmes
  raccourcis que `VoiceLibrarySheet`) : recherche par nom ; filtres
  **Éditeur** (d'après `ModelTable.maker`), **Langue** (Toutes · Français ·
  Anglais seulement), **État** (Tous · Installés · À télécharger) ; chaque
  ligne : logo, nom, langues · précision, résumé, taille, et l'action
  (Télécharger / progression + Annuler / Supprimer…) ; « Utiliser » en bas
  choisit le modèle (installé) pour la dictée. Échap ferme, Retour utilise.
- Logos : Cohere (LobeHub, déjà sous licence jointe) ; Nemotron → NVIDIA.
- La lecture garde sa table (peu de modèles).
- Accessibilité comme ailleurs : chaque ligne un élément, étiquette complète,
  progression annoncée par paliers de 25 %.
