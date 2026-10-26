import type { Metadata } from "next";
import { metadonnees } from "../../_i18n";
import { Accueil } from "../../_pages/Accueil";

export const metadata: Metadata = metadonnees("fr", "accueil");

export default function Page() {
  return <Accueil lang="fr" />;
}
