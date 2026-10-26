import type { Metadata } from "next";
import { dico, metadonnees } from "../../../_i18n";
import { Telecharger } from "../../../_pages/Telecharger";

export const metadata: Metadata = metadonnees("fr", "telecharger", {
  titre: dico("fr").telecharger.meta.titre,
  description: dico("fr").telecharger.meta.description,
});

export default function Page() {
  return <Telecharger lang="fr" />;
}
