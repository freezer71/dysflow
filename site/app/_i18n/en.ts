import type { Dico } from "./fr";

// Le texte du site en anglais, la langue par défaut. Écrit pour être lu en
// anglais, pas traduit mot à mot : même ton court et concret que le français.

export const en: Dico = {
  meta: {
    titre: "sillage — the read-aloud reader you follow with your eyes",
    modele: "%s · sillage",
    description:
      "sillage reads your documents aloud and highlights every word. Neural voices and dictation, 100% on your machine, free and open source. macOS and Linux.",
  },
  entete: {
    accueil: "sillage, home",
    avantages: "Benefits",
    comparaison: "Comparison",
    modeles: "Models",
    github: "GitHub",
    telecharger: "Download",
    langues: "Language",
  },
  pied: {
    licence: "Free software, MIT license. Made in France.",
    modeles: "Models",
    telecharger: "Download",
    code: "Source code",
  },
  commande: { copier: "Copy", copie: "Copied ✓" },
  demo: {
    fichier: "annual-report.pdf",
    titre: "Introduction",
    lignes: [
      "sillage reads aloud what you",
      "give it, and shows every word",
      "the moment it is said.",
    ],
    vitesse: "1.25×",
  },
  accueil: {
    pastilles: ["Free", "Open source", "100% local"],
    titre: "Listen to your documents. Follow every word.",
    intro:
      "sillage reads your PDFs, notes and screenshots aloud, and highlights each word the moment it is said. Natural voices, computed on your machine: nothing goes online, and there is nothing to pay.",
    telecharger: "Download sillage",
    github: "See the code on GitHub",
    commande: "Or with one command, on Mac (Apple silicon) and Linux:",
    chiffres: [
      { valeur: "$0", legende: "forever, no subscription" },
      { valeur: "0 bytes", legende: "of your text sent online" },
      { valeur: "15×", legende: "faster than speech on a Mac" },
      { valeur: "#", legende: "voice and dictation models" },
    ],
    avantagesTitre: {
      surtitre: "Why sillage",
      titre: "A reader made to be followed with your eyes",
      texte:
        "Most text-to-speech readers just talk. sillage understands what it reads, and shows you where you are.",
    },
    avantages: [
      {
        titre: "Your text stays with you",
        texte:
          "Voices and dictation run on your machine. No account, no server, no quota: a contract, a medical record or a draft never leaves your computer.",
      },
      {
        titre: "Follow every word with your eyes",
        texte:
          "The spoken word gets a highlight, its line a discreet band: the word shows where the voice is, the line shows where to come back to when you look up. Made for dyslexia and long reading.",
      },
      {
        titre: "It reads whatever you give it",
        texte:
          "Markdown, text, PDF, scanned PDF, screenshot, photo of a page, clipboard, a selection in any app. Scanned pages are recognized in parallel and remembered.",
      },
      {
        titre: "It doesn't read the formatting",
        texte:
          "Never “hash heading” or “dash dash”. A code block is announced, not recited. A table is read column by column: “Route: /users. Method: GET.”",
      },
      {
        titre: "Faster, without the chipmunk voice",
        texte:
          "Speed acts on the sound while it plays: the voice speeds up without going shrill, mid-sentence, with no break. Skipping forward and back is instant.",
      },
      {
        titre: "It picks up where you left off",
        texte:
          "Your place is remembered on its own — the sentence itself, not a number. It is found again even if the document was edited in the meantime.",
      },
      {
        titre: "Dictation, as a bonus",
        texte:
          "A shortcut opens dictation, and the text is typed into the app in front. Parakeet, Whisper, Voxtral, Canary: pick yours, everything stays local.",
      },
      {
        titre: "Truly free software",
        texte:
          "Open code under the MIT license, models under open licenses. You can read it, change it, redistribute it. Nobody can take it away from you or make you pay for it.",
      },
    ],
    pourQui: {
      surtitre: "Who it's for",
      titre: "For people who read a lot, and for people who find reading hard.",
      publics: [
        {
          titre: "Dyslexia, ADHD, eye strain",
          texte:
            "Listening and following with your eyes at the same time: bimodal reading, with the font of your choice (OpenDyslexic, Atkinson Hyperlegible, Literata).",
        },
        {
          titre: "Students and researchers",
          texte:
            "PDFs are rebuilt: headings in the outline, split words rejoined, headers and footers removed. You hear the article, not the page layout.",
        },
        {
          titre: "Developers",
          texte:
            "Docs, READMEs, AI answers piped into the terminal: `llm \"…\" | sillage` starts reading before the stream ends.",
        },
      ],
    },
    comparaisonTitre: {
      surtitre: "Comparison",
      titre: "Everything the big names do, without the subscription or the cloud",
      texte:
        "Commercial readers send your text to their servers and charge for the good voices. sillage does the computing on your machine.",
    },
    comparaison: {
      critere: "Criterion",
      concurrents: ["sillage", "Speechify", "ElevenReader", "NaturalReader", "Spoken Content (macOS)"],
      oui: "Yes",
      non: "No",
      partiel: "Partly",
      lignes: [
        { critere: "Free, with no usage limit", cases: ["@oui", "Limited free tier, then subscription", "Limited free tier, then subscription", "Limited free tier, then subscription", "@oui"] },
        { critere: "Open source code", cases: ["@oui", "@non", "@non", "@non", "@non"] },
        { critere: "Your text stays on your machine", cases: ["@oui", "@non", "@non", "@non", "@oui"] },
        { critere: "Natural neural voices", cases: ["@oui", "@oui", "@oui", "@oui", "@partiel"] },
        { critere: "Highlights the word being read", cases: ["@oui", "@oui", "@oui", "@oui", "@oui"] },
        { critere: "Doesn't read markup, code or table borders", cases: ["@oui", "Not specified", "Not specified", "Not specified", "@non"] },
        { critere: "Scanned page recognition", cases: ["@oui", "@oui", "@oui", "@oui", "@non"] },
        { critere: "Built-in dictation, offline", cases: ["@oui", "Not specified", "@non", "@non", "@partiel"] },
        { critere: "Linux version", cases: ["Terminal", "Browser", "Browser", "Browser", "@non"] },
      ],
      note: "Based on what the vendors state on their websites, as of October 2026. Spotted a mistake?",
      noteLien: "Tell us",
    },
    modelesTitre: {
      surtitre: "Models",
      titre: "The best open models, your pick",
      texte:
        "Download, try, delete: you stay in control of quality, speed and disk space.",
    },
    lecture: "Reading",
    dictee: "Dictation",
    voirModeles: "See all # models",
    questionsTitre: { surtitre: "Questions", titre: "What people ask" },
    faq: [
      {
        q: "Is it really free?",
        r: "Yes, with no strings attached: no account, no ads, no “Pro” version. sillage is free software under the MIT license; the models are downloaded from their public repositories.",
      },
      {
        q: "Are my documents sent anywhere?",
        r: "No. Reading, text recognition and dictation are all computed on your machine. The only network access is downloading the models you choose.",
      },
      {
        q: "Which Mac do I need?",
        r: "A Mac with Apple silicon (M1 and later). Dictation needs macOS 26. Kokoro, the default voice, takes 342 MB and under 6% of compute time while reading.",
      },
      {
        q: "What about Linux?",
        r: "The terminal reader runs on Linux x86_64 and ARM64 (glibc 2.39 or later), with the same Kokoro voice and built-in text recognition. The same install command as on Mac installs it.",
      },
      {
        q: "Which languages?",
        r: "Reading is made for English and French, with a single voice for both. Dictation covers up to 99 languages depending on the model you choose.",
      },
      {
        q: "What about Windows?",
        r: "Not for now. The engine is written in Rust and could be ported: contributions are welcome on GitHub.",
      },
    ],
    final: {
      titre: "Your next document: listen to it.",
      texte: "Free, open, and installed with one command.",
    },
  },
  modeles: {
    meta: {
      titre: "The models",
      description:
        "Every sillage voice and dictation model: Kokoro, Qwen3-TTS, MagpieTTS, Parakeet, Phonon-2, Whisper, Canary, Voxtral. Sizes, languages, speed, licenses.",
    },
    entete: {
      surtitre: "Models",
      titre: "Choose your voice, and your dictation",
      texte:
        "Every model runs on your machine. They are downloaded, chosen and deleted in the app's settings: you stay in control of disk space. Speeds are measured, not promised.",
    },
    champs: {
      taille: "Size",
      precision: "Accuracy",
      langues: "Languages",
      calcul: "Compute",
      rythme: "Speed",
      voix: "Voices",
      licence: "License",
      modele: "Model",
    },
    recommande: "Recommended",
    lourd: "Heavy",
    source: "The model on Hugging Face",
    lecture: {
      titre: "Reading aloud",
      texte:
        "One voice for every language. Kokoro installs itself on first launch; the system voice reads in the meantime.",
    },
    dictee: {
      titre: "Dictation",
      texte:
        "In the macOS app: a shortcut opens dictation, and the text is typed into the app in front. No model is imposed; a microphone test bench measures accuracy before you choose.",
    },
    ocr: {
      titre: "Text recognition",
      texte:
        "For images, screenshots and scanned PDFs, sillage bundles Tesseract with French and English: nothing to install. Pages are recognized in parallel and the result is remembered, so the second opening is instant.",
    },
    mesures:
      "Measured on an M5 Mac. “Busy x% of the time at 1×”: the share of playback time spent computing the voice; above 100%, playback stutters.",
    telecharger: "Download sillage",
  },
  telecharger: {
    meta: {
      titre: "Download",
      description:
        "Install sillage with one command on Mac and Linux, or download the macOS app as a .dmg.",
    },
    version: "Version",
    derniere: "Latest version",
    titre: "Installed with one command.",
    intro: "Open Terminal, paste, Enter. On Mac and on Linux alike.",
    etapes: [
      "Picks the right version for your machine",
      "Checks the download's checksum",
      "Installs without an administrator password",
      "Opens sillage — the voice installs itself",
    ],
    ou: "or download",
    mac: {
      pastille: "Apple silicon",
      texte:
        "Open the .dmg and drag Sillage into Applications. macOS 14 or later; dictation needs macOS 26.",
      bouton: "Download the .dmg",
      noteAvant: "The app isn't notarized by Apple yet: on first launch, open",
      noteReglages: "System Settings › Privacy & Security",
      noteMilieu: "and click",
      noteOuvrir: "Open Anyway",
      noteApres: ". The command above skips this detour.",
    },
    linux: {
      pastille: "Terminal",
      texte:
        "The terminal reader, with Kokoro and text recognition. Ubuntu 24.04, Debian 13, Fedora 40 or later.",
      archivesAvant: "The archives and their checksums are also in the",
      archivesLien: "GitHub releases",
    },
    miseAJour: {
      titre: "Update",
      texte:
        "One command, and the latest version replaces yours. Your models and settings stay. Already up to date? It tells you, without downloading anything again.",
    },
    desinstaller: {
      titre: "Uninstall",
      texte: "The same command, with an option. Your models and settings stay.",
    },
    compilerAvant: "Prefer to build it yourself?",
    compilerLien: "The instructions are in the README",
    souciAvant: "Got a problem?",
    souciLien: "Open an issue",
  },
};
