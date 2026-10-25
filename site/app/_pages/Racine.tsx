import type { ReactNode } from "react";
import { Atkinson_Hyperlegible_Next, Atkinson_Hyperlegible_Mono, Lexend } from "next/font/google";
import { Pied } from "../_components/ui";
import { dico, type Lang } from "../_i18n";
import "../globals.css";

const atkinson = Atkinson_Hyperlegible_Next({
  variable: "--font-atkinson",
  subsets: ["latin", "latin-ext"],
});

// La police au choix du panneau Confort : jamais préchargée, puisque la
// plupart des visiteurs ne la demanderont pas.
const lexend = Lexend({
  variable: "--font-lexend",
  subsets: ["latin", "latin-ext"],
  preload: false,
});

const atkinsonMono = Atkinson_Hyperlegible_Mono({
  variable: "--font-atkinson-mono",
  subsets: ["latin"],
});

/**
 * Les réglages de confort (`reglages.ts`), posés sur <html> avant la première
 * image : sinon la page s'afficherait un instant dans l'ancienne police, à
 * l'ancienne taille. Seules les clés connues passent.
 */
const reprendreConfort = `try{var r=JSON.parse(localStorage.getItem("dysflow-confort")||"{}"),c=["police","taille","espacement","fond","animations"];for(var i=0;i<c.length;i++){var v=r[c[i]];if(typeof v==="string"&&/^[a-z]+$/.test(v))document.documentElement.dataset[c[i]]=v}}catch(e){}`;

/**
 * Le corps commun des deux mises en page racines, `(en)` et `(fr)`. L'en-tête
 * n'est pas ici : chaque page le pose avec `Cadre`, car le sélecteur de
 * langue doit savoir de quelle page il part.
 */
export function Racine({ lang, children }: { lang: Lang; children: ReactNode }) {
  return (
    // Le script ci-dessous change les attributs de <html> avant React : sans
    // `suppressHydrationWarning`, l'hydratation s'en plaindrait.
    <html lang={lang} suppressHydrationWarning className={`${atkinson.variable} ${atkinsonMono.variable} ${lexend.variable} h-full antialiased`}>
      <head>
        <script dangerouslySetInnerHTML={{ __html: reprendreConfort }} />
      </head>
      <body className="flex min-h-full flex-col font-sans">
        <a
          href="#contenu"
          className="sr-only z-50 rounded-full bg-encre px-5 py-3 font-bold text-sur-encre focus:not-sr-only focus:fixed focus:left-4 focus:top-4"
        >
          {dico(lang).evitement}
        </a>
        {children}
        <Pied lang={lang} />
      </body>
    </html>
  );
}
