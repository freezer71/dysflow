import type { ReactNode } from "react";
import { Atkinson_Hyperlegible_Next, Atkinson_Hyperlegible_Mono } from "next/font/google";
import { Pied } from "../_components/ui";
import type { Lang } from "../_i18n";
import "../globals.css";

const atkinson = Atkinson_Hyperlegible_Next({
  variable: "--font-atkinson",
  subsets: ["latin", "latin-ext"],
});

const atkinsonMono = Atkinson_Hyperlegible_Mono({
  variable: "--font-atkinson-mono",
  subsets: ["latin"],
});

/**
 * Le corps commun des deux mises en page racines, `(en)` et `(fr)`. L'en-tête
 * n'est pas ici : chaque page le pose avec `Cadre`, car le sélecteur de
 * langue doit savoir de quelle page il part.
 */
export function Racine({ lang, children }: { lang: Lang; children: ReactNode }) {
  return (
    <html lang={lang} className={`${atkinson.variable} ${atkinsonMono.variable} h-full antialiased`}>
      <body className="flex min-h-full flex-col font-sans">
        {children}
        <Pied lang={lang} />
      </body>
    </html>
  );
}
