import type { Metadata } from "next";
import { dico, metadonnees } from "../../_i18n";
import { Modeles } from "../../_pages/Modeles";

export const metadata: Metadata = metadonnees("en", "modeles", {
  titre: dico("en").modeles.meta.titre,
  description: dico("en").modeles.meta.description,
});

export default function Page() {
  return <Modeles lang="en" />;
}
