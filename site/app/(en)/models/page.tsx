import type { Metadata } from "next";
import { alternates, dico } from "../../_i18n";
import { Modeles } from "../../_pages/Modeles";

export const metadata: Metadata = {
  title: dico("en").modeles.meta.titre,
  description: dico("en").modeles.meta.description,
  alternates: alternates("modeles", "en"),
};

export default function Page() {
  return <Modeles lang="en" />;
}
