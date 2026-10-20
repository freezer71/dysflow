import type { Metadata } from "next";
import { alternates } from "../../_i18n";
import { Accueil } from "../../_pages/Accueil";

export const metadata: Metadata = {
  alternates: alternates("accueil", "fr"),
};

export default function Page() {
  return <Accueil lang="fr" />;
}
