// Le catalogue des modèles, repris de `src/models.rs` (tailles, précisions,
// langues, licences) et des mesures notées dans le README. À tenir à jour
// quand le catalogue change.

export type Plateforme = "macOS" | "Terminal (Linux, macOS)";

export type Modele = {
  id: string;
  nom: string;
  famille: string;
  description: string;
  taille: string;
  precision: string;
  langues: string;
  plateforme: Plateforme;
  calcul: string;
  rythme: string;
  voix?: string;
  licence: string;
  recommande?: boolean;
  lourd?: boolean;
  source?: string;
};

export const lecture: Modele[] = [
  {
    id: "kokoro_82m_mlx",
    nom: "Kokoro 82M · MLX",
    famille: "Kokoro",
    description:
      "La voix de sillon. Naturelle, légère, calculée par la puce graphique du Mac. Installée toute seule au premier lancement.",
    taille: "342 Mo",
    precision: "bf16",
    langues: "Français, anglais",
    plateforme: "macOS",
    calcul: "Puce graphique (MLX)",
    rythme: "15 × plus vite que la parole sur M5 — occupé ~6 % du temps",
    voix: "30 voix, dont p-fenrir60 (française) et af_bella (américaine)",
    licence: "Apache-2.0",
    recommande: true,
    source: "https://huggingface.co/hexgrad/Kokoro-82M",
  },
  {
    id: "kokoro_82m_int8",
    nom: "Kokoro 82M · int8",
    famille: "Kokoro",
    description:
      "La même voix, compressée pour le processeur. C'est elle qui lit dans le terminal, sous Linux comme sur Mac.",
    taille: "108 Mo",
    precision: "int8",
    langues: "Français, anglais",
    plateforme: "Terminal (Linux, macOS)",
    calcul: "Processeur (ONNX Runtime)",
    rythme: "8 à 10 × plus vite que la parole",
    voix: "p-fenrir60 · af_bella, am_fenrir, af_heart",
    licence: "Apache-2.0",
    recommande: true,
    source: "https://huggingface.co/hexgrad/Kokoro-82M",
  },
  {
    id: "qwen3_tts_06b_base_8bit",
    nom: "Qwen3-TTS 0.6B · voix française",
    famille: "Qwen3",
    description:
      "Une voix française fixe, au timbre tiré du corpus SIWIS. Plus expressive, plus lourde.",
    taille: "2,0 Go",
    precision: "8 bits",
    langues: "10 langues, dont français et anglais",
    plateforme: "macOS",
    calcul: "Puce graphique (MLX)",
    rythme: "Occupé 76 % du temps à 1× sur M5 — haché au-delà de 1×",
    voix: "1 voix : siwis (française)",
    licence: "Apache-2.0 · timbre SIWIS CC-BY-4.0",
    lourd: true,
  },
  {
    id: "magpie_tts_357m_8bit",
    nom: "MagpieTTS 357M",
    famille: "NVIDIA",
    description:
      "Le modèle multilingue de NVIDIA, avec cinq voix anglaises. Le français est lu tel quel.",
    taille: "302 Mo",
    precision: "8 bits",
    langues: "9 langues, dont français et anglais",
    plateforme: "macOS",
    calcul: "Puce graphique (MLX)",
    rythme: "Occupé 69 % du temps à 1× sur M5 — haché au-delà de 1×",
    voix: "5 voix anglaises : sofia, aria, jason, leo, john_van_stan",
    licence: "NVIDIA Open Model License",
    lourd: true,
  },
  {
    id: "system",
    nom: "Voix du système",
    famille: "Système",
    description:
      "La voix de macOS, ou espeak-ng sous Linux. Rien à télécharger : elle lit pendant que Kokoro s'installe.",
    taille: "0 Mo",
    precision: "—",
    langues: "Celles du système",
    plateforme: "Terminal (Linux, macOS)",
    calcul: "Système",
    rythme: "Immédiat",
    licence: "Fournie par le système",
  },
];

export const dictee: Modele[] = [
  {
    id: "parakeet_tdt_v3",
    nom: "Parakeet v3",
    famille: "NVIDIA",
    description:
      "Reconnaît vite et bien vingt-cinq langues européennes. Le choix par défaut pour dicter en français.",
    taille: "2,5 Go",
    precision: "bf16",
    langues: "25 langues européennes",
    plateforme: "macOS",
    calcul: "Puce graphique (MLX)",
    rythme: "Relecture toutes les 1,5 s",
    licence: "CC-BY-4.0",
    recommande: true,
    source: "https://huggingface.co/mlx-community/parakeet-tdt-0.6b-v3",
  },
  {
    id: "phonon_2",
    nom: "Phonon-2",
    famille: "Fermion Research",
    description:
      "Parakeet v3 réentraîné pour l'anglais et réduit à 164 Mo, presque aussi juste. Le français passe, moins sûrement.",
    taille: "164 Mo",
    precision: "≈ 2 bits",
    langues: "Anglais (français moins sûr)",
    plateforme: "macOS",
    calcul: "Puce graphique (MLX)",
    rythme: "8 s d'audio transcrites en 0,12 s sur M5",
    licence: "CC-BY-4.0",
    source: "https://huggingface.co/FermionResearch/Phonon-2",
  },
  {
    id: "whisper_tiny_4bit",
    nom: "Whisper tiny",
    famille: "Whisper",
    description: "Le plus petit Whisper : rapide, pour un texte court ou une machine juste.",
    taille: "23 Mo",
    precision: "4 bits",
    langues: "99 langues",
    plateforme: "macOS",
    calcul: "Puce graphique (MLX)",
    rythme: "Relecture toutes les 1,5 s",
    licence: "Apache-2.0",
  },
  {
    id: "whisper_small_4bit",
    nom: "Whisper small",
    famille: "Whisper",
    description: "Un bon compromis entre taille et justesse, en quatre-vingt-dix-neuf langues.",
    taille: "140 Mo",
    precision: "4 bits",
    langues: "99 langues",
    plateforme: "macOS",
    calcul: "Puce graphique (MLX)",
    rythme: "Relecture toutes les 1,5 s",
    licence: "Apache-2.0",
  },
  {
    id: "whisper_large_v3_turbo_4bit",
    nom: "Whisper large-v3-turbo",
    famille: "Whisper",
    description: "Le plus juste des Whisper de la liste, au prix d'un téléchargement plus lourd.",
    taille: "464 Mo",
    precision: "4 bits",
    langues: "99 langues",
    plateforme: "macOS",
    calcul: "Puce graphique (MLX)",
    rythme: "Relecture toutes les 1,5 s",
    licence: "Apache-2.0",
  },
  {
    id: "canary_1b_v2_q8",
    nom: "Canary 1b-v2",
    famille: "NVIDIA",
    description: "Reconnaissance et traduction de vingt-cinq langues européennes.",
    taille: "1,1 Go",
    precision: "8 bits",
    langues: "25 langues européennes",
    plateforme: "macOS",
    calcul: "Puce graphique (MLX)",
    rythme: "Relecture toutes les 1,5 s",
    licence: "CC-BY-4.0",
  },
  {
    id: "voxtral_realtime_4bit",
    nom: "Voxtral Realtime",
    famille: "Mistral",
    description: "Écrit au fil de la parole, sans attendre la fin de la phrase.",
    taille: "3,1 Go",
    precision: "4 bits",
    langues: "13 langues",
    plateforme: "macOS",
    calcul: "Puce graphique (MLX)",
    rythme: "Au fil de la parole",
    licence: "Apache-2.0",
  },
  {
    id: "system",
    nom: "Dictée de macOS",
    famille: "Système",
    description:
      "La reconnaissance de macOS 26, sur l'appareil. Rien à télécharger, et toujours là en repli.",
    taille: "0 Mo",
    precision: "—",
    langues: "Celles de macOS",
    plateforme: "macOS",
    calcul: "Système",
    rythme: "Au fil de la parole",
    licence: "Fournie par macOS",
  },
];
