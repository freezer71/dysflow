"use client";

import Link from "next/link";
import type { ComponentProps } from "react";
import type { Lang } from "../_i18n";

// Un lien du sélecteur de langue. Le clic retient la langue choisie (cookie
// `langue`, un an) : le proxy ne renvoie plus vers la langue du navigateur.
// Pas de préchargement : sans le cookie, le proxy aurait renvoyé la page
// anglaise vers la française, et le routeur aurait gardé la mauvaise.

export function LienLangue({ langue, ...props }: { langue: Lang } & ComponentProps<typeof Link>) {
  return (
    <Link
      {...props}
      prefetch={false}
      onClick={() => {
        document.cookie = `langue=${langue}; path=/; max-age=31536000; samesite=lax`;
      }}
    />
  );
}
