import type { IdArticle } from "./slugs";

/** Le texte d'un article dans une langue. `corps` est en markdown réduit (`Texte.tsx`). */
export type TexteArticle = {
  titre: string;
  description: string;
  chapo: string;
  corps: string;
};

export type Article = {
  id: IdArticle;
  /** Date de publication, AAAA-MM-JJ. */
  publie: string;
  modifie?: string;
  fr: TexteArticle;
  en: TexteArticle;
};
