import type { Metadata } from "next";
import { dico, metadonnees } from "../../_i18n";
import { Confidentialite } from "../../_pages/Confidentialite";

export const metadata: Metadata = metadonnees("en", "confidentialite", {
  titre: dico("en").confidentialite.meta.titre,
  description: dico("en").confidentialite.meta.description,
});

export default function Page() {
  return <Confidentialite lang="en" />;
}
