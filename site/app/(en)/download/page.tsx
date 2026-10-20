import type { Metadata } from "next";
import { alternates, dico } from "../../_i18n";
import { Telecharger } from "../../_pages/Telecharger";

export const metadata: Metadata = {
  title: dico("en").telecharger.meta.titre,
  description: dico("en").telecharger.meta.description,
  alternates: alternates("telecharger", "en"),
};

export default function Page() {
  return <Telecharger lang="en" />;
}
