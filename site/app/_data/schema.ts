import { depot, telechargements } from "./liens";
import { chemin, dico, imageDe, nomDuSite, site, type Lang } from "../_i18n";

// Les entités schema.org communes à plusieurs pages. Les adresses sont
// absolues : les données structurées ne connaissent pas `metadataBase`.

export const absolu = (chemin: string) => new URL(chemin, site).toString();

/** L'image de partage d'une page, en adresse absolue. */
export const imageAbsolue = (cheminPage: string) => absolu(imageDe(cheminPage));

export const organisation = {
  "@type": "Organization",
  "@id": absolu("/#organisation"),
  name: nomDuSite,
  url: site,
  logo: absolu("/icon.svg"),
  sameAs: [depot],
};

export function siteWeb(lang: Lang) {
  return {
    "@type": "WebSite",
    "@id": absolu(`${chemin(lang, "accueil")}#site`),
    name: nomDuSite,
    url: absolu(chemin(lang, "accueil")),
    inLanguage: lang,
    description: dico(lang).meta.description,
    publisher: { "@id": organisation["@id"] },
  };
}

export function logiciel(lang: Lang) {
  return {
    "@type": "SoftwareApplication",
    "@id": absolu("/#app"),
    name: nomDuSite,
    description: dico(lang).meta.description,
    url: absolu(chemin(lang, "accueil")),
    image: imageAbsolue(chemin(lang, "accueil")),
    applicationCategory: "UtilitiesApplication",
    applicationSubCategory: "Text-to-speech, speech-to-text",
    operatingSystem: "macOS 14+, Linux",
    downloadUrl: absolu(chemin(lang, "telecharger")),
    installUrl: telechargements.dmg,
    license: "https://opensource.org/licenses/MIT",
    isAccessibleForFree: true,
    offers: { "@type": "Offer", price: "0", priceCurrency: "EUR" },
    author: { "@id": organisation["@id"] },
  };
}

/** Le fil d'Ariane : chaque maillon, de l'accueil à la page. */
export function filAriane(maillons: { nom: string; chemin: string }[]) {
  return {
    "@type": "BreadcrumbList",
    itemListElement: maillons.map((m, i) => ({
      "@type": "ListItem",
      position: i + 1,
      name: m.nom,
      item: absolu(m.chemin),
    })),
  };
}
