// Le catalogue des modèles, repris de `src/models.rs` (tailles, précisions,
// langues, licences) et des mesures notées dans le README. À tenir à jour
// quand le catalogue change.

import type { Lang } from "../_i18n";

/** Un texte visible, dans chaque langue du site. */
export type Texte = { en: string; fr: string };

/** Le même texte dans les deux langues (noms propres, sigles, valeurs). */
const pareil = (s: string): Texte => ({ en: s, fr: s });

/** Le texte d'une langue. */
export const dans = (t: Texte, lang: Lang): string => t[lang];

export type Modele = {
  id: string;
  nom: Texte;
  famille: Texte;
  description: Texte;
  taille: Texte;
  precision: Texte;
  langues: Texte;
  plateforme: Texte;
  calcul: Texte;
  rythme: Texte;
  voix?: Texte;
  licence: Texte;
  recommande?: boolean;
  lourd?: boolean;
  source?: string;
};

const macOS = pareil("macOS");
const terminal = pareil("Terminal (Linux, macOS)");
const mlx = { en: "GPU (MLX)", fr: "Puce graphique (MLX)" };
const apache = pareil("Apache-2.0");
const ccBy = pareil("CC-BY-4.0");
const nvidia = pareil("NVIDIA");
const whisper = pareil("Whisper");
const relecture = { en: "Re-read every 1.5 s", fr: "Relecture toutes les 1,5 s" };

export const lecture: Modele[] = [
  {
    id: "kokoro_82m_mlx",
    nom: pareil("Kokoro 82M · MLX"),
    famille: pareil("Kokoro"),
    description: {
      en: "The dysflow voice. Natural, light, computed by the Mac's GPU. Installed automatically on first launch.",
      fr: "La voix de dysflow. Naturelle, légère, calculée par la puce graphique du Mac. Installée toute seule au premier lancement.",
    },
    taille: { en: "342 MB", fr: "342 Mo" },
    precision: pareil("bf16"),
    langues: { en: "French, English", fr: "Français, anglais" },
    plateforme: macOS,
    calcul: mlx,
    rythme: {
      en: "15× faster than speech on an M5 — busy ~6% of the time",
      fr: "15 × plus vite que la parole sur M5 — occupé ~6 % du temps",
    },
    voix: {
      en: "30 voices, including p-fenrir60 (French) and af_bella (American)",
      fr: "30 voix, dont p-fenrir60 (française) et af_bella (américaine)",
    },
    licence: apache,
    recommande: true,
    source: "https://huggingface.co/hexgrad/Kokoro-82M",
  },
  {
    id: "kokoro_82m_int8",
    nom: pareil("Kokoro 82M · int8"),
    famille: pareil("Kokoro"),
    description: {
      en: "The same voice, compressed for the CPU. It is the one that reads in the terminal, on Linux and on Mac.",
      fr: "La même voix, compressée pour le processeur. C'est elle qui lit dans le terminal, sous Linux comme sur Mac.",
    },
    taille: { en: "108 MB", fr: "108 Mo" },
    precision: pareil("int8"),
    langues: { en: "French, English", fr: "Français, anglais" },
    plateforme: terminal,
    calcul: { en: "CPU (ONNX Runtime)", fr: "Processeur (ONNX Runtime)" },
    rythme: { en: "8 to 10× faster than speech", fr: "8 à 10 × plus vite que la parole" },
    voix: pareil("p-fenrir60 · af_bella, am_fenrir, af_heart"),
    licence: apache,
    recommande: true,
    source: "https://huggingface.co/hexgrad/Kokoro-82M",
  },
  {
    id: "qwen3_tts_06b_base_8bit",
    nom: { en: "Qwen3-TTS 0.6B · French voice", fr: "Qwen3-TTS 0.6B · voix française" },
    famille: pareil("Qwen3"),
    description: {
      en: "A fixed French voice, with a timbre drawn from the SIWIS corpus. More expressive, heavier.",
      fr: "Une voix française fixe, au timbre tiré du corpus SIWIS. Plus expressive, plus lourde.",
    },
    taille: { en: "2.0 GB", fr: "2,0 Go" },
    precision: { en: "8-bit", fr: "8 bits" },
    langues: { en: "10 languages, including French and English", fr: "10 langues, dont français et anglais" },
    plateforme: macOS,
    calcul: mlx,
    rythme: {
      en: "Busy 76% of the time at 1× on an M5 — choppy above 1×",
      fr: "Occupé 76 % du temps à 1× sur M5 — haché au-delà de 1×",
    },
    voix: { en: "1 voice: siwis (French)", fr: "1 voix : siwis (française)" },
    licence: { en: "Apache-2.0 · SIWIS timbre CC-BY-4.0", fr: "Apache-2.0 · timbre SIWIS CC-BY-4.0" },
    lourd: true,
  },
  {
    id: "magpie_tts_357m_8bit",
    nom: pareil("MagpieTTS 357M"),
    famille: nvidia,
    description: {
      en: "NVIDIA's multilingual model, with five English voices. French is read as is.",
      fr: "Le modèle multilingue de NVIDIA, avec cinq voix anglaises. Le français est lu tel quel.",
    },
    taille: { en: "302 MB", fr: "302 Mo" },
    precision: { en: "8-bit", fr: "8 bits" },
    langues: { en: "9 languages, including French and English", fr: "9 langues, dont français et anglais" },
    plateforme: macOS,
    calcul: mlx,
    rythme: {
      en: "Busy 69% of the time at 1× on an M5 — choppy above 1×",
      fr: "Occupé 69 % du temps à 1× sur M5 — haché au-delà de 1×",
    },
    voix: {
      en: "5 English voices: sofia, aria, jason, leo, john_van_stan",
      fr: "5 voix anglaises : sofia, aria, jason, leo, john_van_stan",
    },
    licence: pareil("NVIDIA Open Model License"),
    lourd: true,
  },
  {
    id: "system",
    nom: { en: "System voice", fr: "Voix du système" },
    famille: { en: "System", fr: "Système" },
    description: {
      en: "The macOS voice, or espeak-ng on Linux. Nothing to download: it reads while Kokoro installs.",
      fr: "La voix de macOS, ou espeak-ng sous Linux. Rien à télécharger : elle lit pendant que Kokoro s'installe.",
    },
    taille: { en: "0 MB", fr: "0 Mo" },
    precision: pareil("—"),
    langues: { en: "The system's languages", fr: "Celles du système" },
    plateforme: terminal,
    calcul: { en: "System", fr: "Système" },
    rythme: { en: "Instant", fr: "Immédiat" },
    licence: { en: "Provided by the system", fr: "Fournie par le système" },
  },
];

export const dictee: Modele[] = [
  {
    id: "parakeet_tdt_v3",
    nom: pareil("Parakeet v3"),
    famille: nvidia,
    description: {
      en: "Recognizes twenty-five European languages quickly and well. The default choice for dictating in French.",
      fr: "Reconnaît vite et bien vingt-cinq langues européennes. Le choix par défaut pour dicter en français.",
    },
    taille: { en: "2.5 GB", fr: "2,5 Go" },
    precision: pareil("bf16"),
    langues: { en: "25 European languages", fr: "25 langues européennes" },
    plateforme: macOS,
    calcul: mlx,
    rythme: relecture,
    licence: ccBy,
    recommande: true,
    source: "https://huggingface.co/mlx-community/parakeet-tdt-0.6b-v3",
  },
  {
    id: "phonon_2",
    nom: pareil("Phonon-2"),
    famille: pareil("Fermion Research"),
    description: {
      en: "Parakeet v3 retrained for English and shrunk to 164 MB, almost as accurate. French works, less reliably.",
      fr: "Parakeet v3 réentraîné pour l'anglais et réduit à 164 Mo, presque aussi juste. Le français passe, moins sûrement.",
    },
    taille: { en: "164 MB", fr: "164 Mo" },
    precision: pareil("≈ 2 bits"),
    langues: { en: "English (French less reliable)", fr: "Anglais (français moins sûr)" },
    plateforme: macOS,
    calcul: mlx,
    rythme: {
      en: "8 s of audio transcribed in 0.12 s on an M5",
      fr: "8 s d'audio transcrites en 0,12 s sur M5",
    },
    licence: ccBy,
    source: "https://huggingface.co/FermionResearch/Phonon-2",
  },
  {
    id: "whisper_tiny_4bit",
    nom: pareil("Whisper tiny"),
    famille: whisper,
    description: {
      en: "The smallest Whisper: fast, for a short text or a modest machine.",
      fr: "Le plus petit Whisper : rapide, pour un texte court ou une machine juste.",
    },
    taille: { en: "23 MB", fr: "23 Mo" },
    precision: { en: "4-bit", fr: "4 bits" },
    langues: { en: "99 languages", fr: "99 langues" },
    plateforme: macOS,
    calcul: mlx,
    rythme: relecture,
    licence: apache,
  },
  {
    id: "whisper_small_4bit",
    nom: pareil("Whisper small"),
    famille: whisper,
    description: {
      en: "A good trade-off between size and accuracy, in ninety-nine languages.",
      fr: "Un bon compromis entre taille et justesse, en quatre-vingt-dix-neuf langues.",
    },
    taille: { en: "140 MB", fr: "140 Mo" },
    precision: { en: "4-bit", fr: "4 bits" },
    langues: { en: "99 languages", fr: "99 langues" },
    plateforme: macOS,
    calcul: mlx,
    rythme: relecture,
    licence: apache,
  },
  {
    id: "whisper_large_v3_turbo_4bit",
    nom: pareil("Whisper large-v3-turbo"),
    famille: whisper,
    description: {
      en: "The most accurate Whisper in the list, at the cost of a heavier download.",
      fr: "Le plus juste des Whisper de la liste, au prix d'un téléchargement plus lourd.",
    },
    taille: { en: "464 MB", fr: "464 Mo" },
    precision: { en: "4-bit", fr: "4 bits" },
    langues: { en: "99 languages", fr: "99 langues" },
    plateforme: macOS,
    calcul: mlx,
    rythme: relecture,
    licence: apache,
  },
  {
    id: "canary_1b_v2_q8",
    nom: pareil("Canary 1b-v2"),
    famille: nvidia,
    description: {
      en: "Recognition and translation for twenty-five European languages.",
      fr: "Reconnaissance et traduction de vingt-cinq langues européennes.",
    },
    taille: { en: "1.1 GB", fr: "1,1 Go" },
    precision: { en: "8-bit", fr: "8 bits" },
    langues: { en: "25 European languages", fr: "25 langues européennes" },
    plateforme: macOS,
    calcul: mlx,
    rythme: relecture,
    licence: ccBy,
  },
  {
    id: "voxtral_realtime_4bit",
    nom: pareil("Voxtral Realtime"),
    famille: pareil("Mistral"),
    description: {
      en: "Writes as you speak, without waiting for the end of the sentence.",
      fr: "Écrit au fil de la parole, sans attendre la fin de la phrase.",
    },
    taille: { en: "3.1 GB", fr: "3,1 Go" },
    precision: { en: "4-bit", fr: "4 bits" },
    langues: { en: "13 languages", fr: "13 langues" },
    plateforme: macOS,
    calcul: mlx,
    rythme: { en: "As you speak", fr: "Au fil de la parole" },
    licence: apache,
  },
  {
    id: "system",
    nom: { en: "macOS Dictation", fr: "Dictée de macOS" },
    famille: { en: "System", fr: "Système" },
    description: {
      en: "macOS 26's on-device recognition. Nothing to download, and always there as a fallback.",
      fr: "La reconnaissance de macOS 26, sur l'appareil. Rien à télécharger, et toujours là en repli.",
    },
    taille: { en: "0 MB", fr: "0 Mo" },
    precision: pareil("—"),
    langues: { en: "macOS's languages", fr: "Celles de macOS" },
    plateforme: macOS,
    calcul: { en: "System", fr: "Système" },
    rythme: { en: "As you speak", fr: "Au fil de la parole" },
    licence: { en: "Provided by macOS", fr: "Fournie par macOS" },
  },
];
