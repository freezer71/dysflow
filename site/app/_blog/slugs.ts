// Les adresses des articles, dans chaque langue. À part du texte des
// articles : le proxy s'en sert, et n'a pas à embarquer les articles.

export const slugs = {
  "ecouter-en-lisant": { en: "listening-while-reading-dyslexia", fr: "ecouter-en-lisant-dyslexie" },
  "dictee-vocale": { en: "speech-to-text-dyslexia", fr: "dictee-vocale-dyslexie" },
  "police-dyslexie": { en: "best-font-for-dyslexia", fr: "police-dyslexie-etudes" },
  "pdf-scanne": { en: "read-scanned-pdf-aloud-mac", fr: "lire-pdf-scanne-voix-haute-mac" },
} as const;

export type IdArticle = keyof typeof slugs;
