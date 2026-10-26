import type { MetadataRoute } from "next";
import { articles, cheminsArticle } from "./_blog";
import { absolu } from "./_data/schema";
import { cheminsDe, type Chemins, type Lang, type Page } from "./_i18n";

// Chaque page, dans chaque langue, avec ses alternatives : les moteurs
// relient ainsi les deux versions d'une même page.

function entrees(chemins: Chemins, options: Omit<MetadataRoute.Sitemap[number], "url"> = {}): MetadataRoute.Sitemap {
  const languages = { en: absolu(chemins.en), fr: absolu(chemins.fr), "x-default": absolu(chemins.en) };
  return (["en", "fr"] as Lang[]).map((lang) => ({
    url: absolu(chemins[lang]),
    alternates: { languages },
    ...options,
  }));
}

export default function sitemap(): MetadataRoute.Sitemap {
  const pages: [Page, number][] = [
    ["accueil", 1],
    ["telecharger", 0.9],
    ["modeles", 0.7],
    ["blog", 0.7],
  ];
  return [
    ...pages.flatMap(([page, priority]) => entrees(cheminsDe(page), { priority })),
    ...articles.flatMap((a) =>
      entrees(cheminsArticle(a), { lastModified: a.modifie ?? a.publie, priority: 0.6 }),
    ),
  ];
}
