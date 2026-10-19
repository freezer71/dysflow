import type { ReactNode } from "react";
import { metadonneesRacine } from "../_i18n";
import { Racine } from "../_pages/Racine";

export const metadata = metadonneesRacine("fr");

export default function Layout({ children }: { children: ReactNode }) {
  return <Racine lang="fr">{children}</Racine>;
}
