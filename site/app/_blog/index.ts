import { chemin, type Chemins, type Lang } from "../_i18n";
import { dicteeVocale } from "./articles/dictee-vocale";
import { ecouterEnLisant } from "./articles/ecouter-en-lisant";
import { pdfScanne } from "./articles/pdf-scanne";
import { policeDyslexie } from "./articles/police-dyslexie";
import { slugs } from "./slugs";
import type { Article } from "./types";

export type { Article };

/**
 * La typographie française : une espace insécable avant « : ; ? ! » et dans
 * les guillemets. Sans elle, un « ? » peut se retrouver seul en début de
 * ligne. Les articles s'écrivent avec des espaces simples ; on les pose ici.
 */
function insecables<T extends Record<string, string>>(texte: T): T {
  return Object.fromEntries(
    Object.entries(texte).map(([cle, valeur]) => [
      cle,
      valeur.replace(/ ([:;?!»])/g, "\u00a0$1").replace(/« /g, "«\u00a0"),
    ]),
  ) as T;
}

/** Les articles, du plus récent au plus ancien. */
export const articles: Article[] = [ecouterEnLisant, dicteeVocale, policeDyslexie, pdfScanne]
  .map((a) => ({ ...a, fr: insecables(a.fr) }))
  .sort((a, b) => b.publie.localeCompare(a.publie));

export function slug(article: Article, lang: Lang): string {
  return slugs[article.id][lang];
}

/** Les adresses d'un article dans chaque langue. */
export function cheminsArticle(article: Article): Chemins {
  return {
    en: `${chemin("en", "blog")}/${slug(article, "en")}`,
    fr: `${chemin("fr", "blog")}/${slug(article, "fr")}`,
  };
}

export function trouverArticle(lang: Lang, s: string): Article | undefined {
  return articles.find((a) => slug(a, lang) === s);
}

/** Les minutes de lecture, à 200 mots par minute : un rythme posé. */
export function minutesDeLecture(article: Article, lang: Lang): number {
  const mots = article[lang].corps.split(/\s+/).filter(Boolean).length;
  return Math.max(1, Math.round(mots / 200));
}

export function dateLongue(date: string, lang: Lang): string {
  return new Intl.DateTimeFormat(lang === "fr" ? "fr-FR" : "en-GB", { dateStyle: "long", timeZone: "UTC" }).format(
    new Date(`${date}T00:00:00Z`),
  );
}

/** Les autres articles, à proposer en fin de lecture. */
export function autresArticles(article: Article, nombre = 2): Article[] {
  return articles.filter((a) => a.id !== article.id).slice(0, nombre);
}
