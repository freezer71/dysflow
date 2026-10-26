import type { Metadata } from "next";
import { dico, metadonnees } from "../../../_i18n";
import { Blog } from "../../../_pages/Blog";

export const metadata: Metadata = metadonnees("fr", "blog", {
  titre: dico("fr").blog.meta.titre,
  description: dico("fr").blog.meta.description,
});

export default function Page() {
  return <Blog lang="fr" />;
}
