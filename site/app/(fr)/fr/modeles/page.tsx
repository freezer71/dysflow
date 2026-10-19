import type { Metadata } from "next";
import { alternates, dico } from "../../../_i18n";
import { Modeles } from "../../../_pages/Modeles";

export const metadata: Metadata = {
  title: dico("fr").modeles.meta.titre,
  description: dico("fr").modeles.meta.description,
  alternates: alternates("modeles", "fr"),
};

export default function Page() {
  return <Modeles lang="fr" />;
}
