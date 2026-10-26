import type { Dico } from "./fr";

// Le texte du site en anglais, la langue par défaut. Écrit pour être lu en
// anglais, pas traduit mot à mot : même ton court et concret que le français.

export const en: Dico = {
  meta: {
    titre: "Dysflow — read and write without wearing yourself out, made for dyslexia",
    modele: "%s · dysflow",
    description:
      "Dysflow is made for dyslexic and neurodivergent people who find reading or writing hard: it reads your documents aloud, highlighting every word, and types what you dictate. Free, open source, 100% on your machine.",
  },
  entete: {
    accueil: "Dysflow, home",
    parcours: "The journey",
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
  evitement: "Skip to content",
  confort: {
    bouton: "Comfort",
    titre: "Reading comfort",
    modeDys: "Dyslexia mode: turn it all on",
    reinitialiser: "Back to default",
    fermer: "Close",
    police: { titre: "Font", atkinson: "Atkinson", lexend: "Lexend" },
    taille: { titre: "Text size", normale: "Normal", grande: "Large", tresgrande: "Extra large" },
    espacement: { titre: "Spacing", normal: "Normal", large: "Wide" },
    fond: { titre: "Background", creme: "Cream", gris: "Grey", sombre: "Dark" },
    animations: { titre: "Animations", oui: "On", non: "Off" },
  },
  commande: { copier: "Copy", copie: "Copied ✓" },
  accueil: {
    pastilles: [
      "For dyslexia",
      "Free",
      "100% local",
    ],
    titre: "Your dyslexia doesn't have to hold you back.",
    intro: "Dysflow reads your text aloud. And it writes what you say.",
    telecharger: "Download Dysflow",
    parcours: {
      surtitre: "The journey",
      titre: "See the difference",
      lire: {
        titre: "Reading, when you're dyslexic",
        texte: "Try to read this message.",
        note: "An illustration, not a diagnosis.",
      },
      cout: {
        titre: "What it costs",
        texte: "It isn't a lack of intelligence.",
        points: [
          "You read it three times.",
          "You're worn out before you understand.",
          "You end up feeling behind.",
        ],
      },
      avec: {
        titre: "With Dysflow",
        texte: "The voice reads. Your eyes follow.",
      },
      ecrire: {
        titre: "Write by speaking",
        texte: "You speak. The text appears.",
      },
      simulation: {
        lignes: [
          "Hello everyone, tomorrow's",
          "meeting has been moved to two",
          "in the afternoon. Please read",
          "the brief before you come, and",
          "bring your questions: we will",
          "decide together what comes next.",
        ],
        sans: "Without Dysflow",
        avec: "With Dysflow",
        groupe: "Choose how to read",
        voix: "The voice reads",
        legendeSans: "Letters move. You reread. You get tired.",
        legendeAvec: "Nothing moves. The meaning just arrives.",
      },
      dictee: {
        clavier: "Typed",
        micro: "You speak…",
        voix: "Dysflow wrote",
        tapee: "Dear Mrs Brown, here is my geografy homwork.",
        dite: "Dear Mrs Brown, here is my geography homework.",
      },
    },
    modeEmploi: {
      surtitre: "How it works",
      titre: "Two gestures, that's all",
      lire: {
        titre: "To read",
        etapes: [
          { texte: "Select the text, or copy it." },
          { texte: "Press the shortcut.", touches: [{ signe: "⌃", nom: "control" }, { signe: "⌥", nom: "option" }, { signe: "␣", nom: "space" }] },
          { texte: "Dysflow reads it aloud." },
        ],
      },
      ecrire: {
        titre: "To write",
        etapes: [
          { texte: "Press the right option key.", touches: [{ signe: "⌥", nom: "option" }] },
          { texte: "Say what you want." },
          { texte: "Press it again: the text appears." },
        ],
      },
      note: "You can change the shortcuts in the settings.",
    },
    partout: {
      surtitre: "Everywhere",
      titre: "In all your apps",
      texte: "Wherever there's text, Dysflow reads. Wherever you type, Dysflow writes.",
      apps: [
        { nom: "Mail", icone: "mail" },
        { nom: "Safari", icone: "safari" },
        { nom: "Chrome", icone: "chrome" },
        { nom: "Word", icone: "word" },
        { nom: "Pages", icone: "pages" },
        { nom: "Notes", icone: "notes" },
        { nom: "Messages", icone: "messages" },
        { nom: "WhatsApp", icone: "whatsapp" },
        { nom: "Slack", icone: "slack" },
        { nom: "Preview", icone: "apercu" },
        { nom: "Claude", icone: "claude" },
        { nom: "Claude Code", icone: "claude-code" },
        { nom: "ChatGPT", icone: "chatgpt" },
        { nom: "Terminal", icone: "terminal" },
      ],
      plus: "And the rest of your Mac",
    },
    manifeste: {
      titre: "Don't stay behind.",
      lignes: [
        "What you know matters more than your spelling.",
        "Show the world what you know.",
      ],
      texte: "Free. Nothing to sell.",
    },
    avantagesTitre: {
      surtitre: "And also",
      titre: "Simple, and yours",
    },
    avantages: [
      {
        titre: "It reads everything",
        texte: "PDF, scanned letter, photo, screenshot.",
      },
      {
        titre: "At your pace",
        texte: "Slower, faster, without a break.",
      },
      {
        titre: "Nothing leaves your computer",
        texte: "Everything runs on your machine.",
      },
      {
        titre: "Free, forever",
        texte: "Free software. No subscription.",
      },
    ],
    questionsTitre: {
      surtitre: "Questions",
      titre: "What people ask",
    },
    faq: [
      {
        q: "Which language is it in?",
        r: "Your Mac's: English or French. This site follows your browser too.",
      },
      {
        q: "Does it replace speech therapy?",
        r: "No. It's a tool, like glasses.",
      },
      {
        q: "Is it really free?",
        r: "Yes. No account, no ads.",
      },
      {
        q: "Does my text go online?",
        r: "No. Everything stays on your computer.",
      },
      {
        q: "Which computer?",
        r: "A Mac with Apple silicon. On Linux, in the terminal.",
      },
    ],
    final: {
      titre: "Your next page: listen to it.",
      texte: "Free. Installed in a minute.",
    },
  },
  modeles: {
    meta: {
      titre: "The models",
      description:
        "Every Dysflow voice and dictation model: Kokoro, Qwen3-TTS, MagpieTTS, Parakeet, Phonon-2, Whisper, Canary, Voxtral. Sizes, languages, speed, licenses.",
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
        "For images, screenshots and scanned PDFs, Dysflow bundles Tesseract with French and English: nothing to install. Pages are recognized in parallel and the result is remembered, so the second opening is instant.",
    },
    mesures:
      "Measured on an M5 Mac. “Busy x% of the time at 1×”: the share of playback time spent computing the voice; above 100%, playback stutters.",
    telecharger: "Download Dysflow",
  },
  telecharger: {
    meta: {
      titre: "Download",
      description:
        "Install Dysflow with one command on Mac and Linux, or download the macOS app as a .dmg.",
    },
    version: "Version",
    derniere: "Latest version",
    titre: "Installed with one command.",
    intro: "Open Terminal, paste, Enter. On Mac and on Linux alike.",
    etapes: [
      "Picks the right version for your machine",
      "Checks the download's checksum",
      "Installs without an administrator password",
      "Opens Dysflow — the voice installs itself",
    ],
    ou: "or download",
    mac: {
      pastille: "Apple silicon",
      texte:
        "Open the .dmg and drag Dysflow into Applications. macOS 14 or later; dictation needs macOS 26.",
      bouton: "Download the .dmg",
      note: "Signed and notarized by Apple: it opens without a warning.",
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
