import type { Metadata } from "next";
import { dico, metadonnees } from "../../../_i18n";
import { Confidentialite } from "../../../_pages/Confidentialite";

export const metadata: Metadata = metadonnees("fr", "confidentialite", {
  titre: dico("fr").confidentialite.meta.titre,
  description: dico("fr").confidentialite.meta.description,
});

export default function Page() {
  return <Confidentialite lang="fr" />;
}
