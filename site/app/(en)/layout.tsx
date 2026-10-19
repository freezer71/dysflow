import type { ReactNode } from "react";
import { metadonneesRacine } from "../_i18n";
import { Racine } from "../_pages/Racine";

export const metadata = metadonneesRacine("en");

export default function Layout({ children }: { children: ReactNode }) {
  return <Racine lang="en">{children}</Racine>;
}
