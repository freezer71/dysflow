import type { Metadata } from "next";
import { en } from "./en";
import { fr, type Dico } from "./fr";

export type { Dico };
export type Lang = "en" | "fr";
export type Page = "accueil" | "modeles" | "telecharger";

/** L'adresse publique du site, base des adresses absolues des métadonnées. */
export const site = "https://sillage.koumalabs.org";

const chemins: Record<Lang, Record<Page, string>> = {
  en: { accueil: "/", modeles: "/models", telecharger: "/download" },
  fr: { accueil: "/fr", modeles: "/fr/modeles", telecharger: "/fr/telecharger" },
};

/** L'adresse d'une page dans une langue, avec une ancre éventuelle (sans `#`). */
export function chemin(lang: Lang, page: Page, ancre?: string): string {
  const base = chemins[lang][page];
  return ancre ? `${base}#${ancre}` : base;
}

export function dico(lang: Lang): Dico {
  return lang === "fr" ? fr : en;
}

/** L'autre langue, pour le sélecteur. */
export function autreLangue(lang: Lang): Lang {
  return lang === "fr" ? "en" : "fr";
}

/**
 * Les alternatives d'une page : chaque langue se désigne et désigne l'autre,
 * l'anglais sert de repli (`x-default`). La page canonique est la page
 * elle-même, dans la langue où on la lit.
 */
export function alternates(page: Page, lang: Lang = "en"): Metadata["alternates"] {
  return {
    canonical: chemin(lang, page),
    languages: {
      en: chemin("en", page),
      fr: chemin("fr", page),
      "x-default": chemin("en", page),
    },
  };
}

/** Les métadonnées de la racine d'une langue : titre, modèle, description. */
export function metadonneesRacine(lang: Lang): Metadata {
  const d = dico(lang).meta;
  return {
    metadataBase: new URL(site),
    title: { default: d.titre, template: d.modele },
    description: d.description,
  };
}
