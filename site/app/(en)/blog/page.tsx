import type { Metadata } from "next";
import { dico, metadonnees } from "../../_i18n";
import { Blog } from "../../_pages/Blog";

export const metadata: Metadata = metadonnees("en", "blog", {
  titre: dico("en").blog.meta.titre,
  description: dico("en").blog.meta.description,
});

export default function Page() {
  return <Blog lang="en" />;
}
