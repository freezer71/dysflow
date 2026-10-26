import type { Metadata } from "next";
import { en } from "./en";
import { fr, type Dico } from "./fr";

export type { Dico };
export type Lang = "en" | "fr";
export type Page = "accueil" | "modeles" | "telecharger" | "blog";

/** L'adresse publique du site, base des adresses absolues des métadonnées. */
export const site = "https://dysflow.koumalabs.org";

const chemins: Record<Lang, Record<Page, string>> = {
  en: { accueil: "/", modeles: "/models", telecharger: "/download", blog: "/blog" },
  fr: { accueil: "/fr", modeles: "/fr/modeles", telecharger: "/fr/telecharger", blog: "/fr/blog" },
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

/** L'image de partage d'une page (`app/_og/carte.tsx`), relative au site. */
export function imageDe(cheminPage: string): string {
  return `${cheminPage.replace(/\/$/, "")}/og.png`;
}

/** L'adresse d'une même page dans chaque langue. */
export type Chemins = Record<Lang, string>;

export function cheminsDe(page: Page): Chemins {
  return { en: chemin("en", page), fr: chemin("fr", page) };
}

/**
 * Les alternatives d'une page : chaque langue se désigne et désigne l'autre,
 * l'anglais sert de repli (`x-default`). La page canonique est la page
 * elle-même, dans la langue où on la lit.
 */
export function alternates(page: Page | Chemins, lang: Lang): Metadata["alternates"] {
  const c = typeof page === "string" ? cheminsDe(page) : page;
  return {
    canonical: c[lang],
    languages: { en: c.en, fr: c.fr, "x-default": c.en },
    // Le flux du blog, annoncé sur chaque page pour les lecteurs de flux.
    types: { "application/rss+xml": `${chemin(lang, "blog")}/feed.xml` },
  };
}

const locales: Record<Lang, string> = { en: "en_US", fr: "fr_FR" };

/** Le nom du site, tel que l'affichent les réseaux et les moteurs. */
export const nomDuSite = "Dysflow";

/**
 * Les métadonnées d'une page : titre, description, alternatives, et ce que
 * lisent les réseaux (Open Graph, X). Un `openGraph` posé par une page
 * remplace celui de la mise en page en entier : tout est donc redit ici.
 * L'image vient du fichier `opengraph-image` du segment.
 * Sans `titre`, c'est le titre par défaut de la langue (l'accueil).
 */
export function metadonnees(
  lang: Lang,
  page: Page | Chemins,
  { titre, description, article }: {
    titre?: string;
    description?: string;
    article?: { publie: string; modifie?: string };
  } = {},
): Metadata {
  const d = dico(lang).meta;
  const alt = alternates(page, lang);
  const titreComplet = titre ? d.modele.replace("%s", titre) : d.titre;
  const desc = description ?? d.description;
  const ici = (typeof page === "string" ? cheminsDe(page) : page)[lang];
  const image = { url: imageDe(ici), width: 1200, height: 630, alt: titreComplet };
  return {
    ...(titre ? { title: titre } : {}),
    description: desc,
    alternates: alt,
    openGraph: {
      siteName: nomDuSite,
      locale: locales[lang],
      alternateLocale: locales[autreLangue(lang)],
      url: ici,
      images: [image],
      title: titreComplet,
      description: desc,
      ...(article
        ? { type: "article", publishedTime: article.publie, modifiedTime: article.modifie ?? article.publie }
        : { type: "website" }),
    },
    twitter: { card: "summary_large_image", title: titreComplet, description: desc, images: [image] },
  };
}

/** Les métadonnées de la racine d'une langue : titre, modèle, description. */
export function metadonneesRacine(lang: Lang): Metadata {
  const d = dico(lang).meta;
  const accueil = metadonnees(lang, "accueil");
  return {
    metadataBase: new URL(site),
    title: { default: d.titre, template: d.modele },
    description: d.description,
    applicationName: nomDuSite,
    formatDetection: { telephone: false, email: false, address: false },
    // Repli pour une page qui n'en dirait rien. Ni adresse canonique ni
    // alternatives : celles de l'accueil seraient fausses ailleurs.
    openGraph: { ...accueil.openGraph, url: undefined },
    twitter: accueil.twitter,
  };
}
