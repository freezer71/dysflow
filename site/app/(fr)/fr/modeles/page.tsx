import type { Metadata } from "next";
import { dico, metadonnees } from "../../../_i18n";
import { Modeles } from "../../../_pages/Modeles";

export const metadata: Metadata = metadonnees("fr", "modeles", {
  titre: dico("fr").modeles.meta.titre,
  description: dico("fr").modeles.meta.description,
});

export default function Page() {
  return <Modeles lang="fr" />;
}
