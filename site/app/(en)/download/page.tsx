import type { Metadata } from "next";
import { dico, metadonnees } from "../../_i18n";
import { Telecharger } from "../../_pages/Telecharger";

export const metadata: Metadata = metadonnees("en", "telecharger", {
  titre: dico("en").telecharger.meta.titre,
  description: dico("en").telecharger.meta.description,
});

export default function Page() {
  return <Telecharger lang="en" />;
}
