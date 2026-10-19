import type { Metadata } from "next";
import { alternates, dico } from "../../../_i18n";
import { Telecharger } from "../../../_pages/Telecharger";

export const metadata: Metadata = {
  title: dico("fr").telecharger.meta.titre,
  description: dico("fr").telecharger.meta.description,
  alternates: alternates("telecharger", "fr"),
};

export default function Page() {
  return <Telecharger lang="fr" />;
}
