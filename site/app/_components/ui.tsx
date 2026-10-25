import Link from "next/link";
import type { ReactNode } from "react";
import { LienLangue } from "./LienLangue";
import { chemin, dico, type Lang, type Page } from "../_i18n";
import { depot } from "../_data/liens";
import { Confort } from "./Confort";

/** La marque, redessinée à l'encre du thème (le SVG de `assets/logo`). */
const reseau = [[22.5, 21, 2], [23.5, 33.5, 1.8], [36, 18.5, 2], [33, 35, 1.7], [52, 21.5, 2.1]];

const tete =
  "M 23 55 C 22 49 17 44 16.5 36 C 15.5 22 24 12.5 34 12.5 C 42 12.5 46 18.5 45.5 26 C 45.3 27.8 44.3 28.8 44.8 30.5 C 46 33 49.5 35 49.8 36.3 C 50 37.3 48 37.8 46.8 38 C 47.8 39 47.8 40 46.6 40.8 C 47.6 41.5 47.4 42.6 46.4 43.3 C 46 44 46.6 45.2 46.6 46 C 46.4 48 43 48.6 40 48.4 C 38.6 50.5 39 53 39.8 55.5";

/** La tête et son réseau. `liseré` cerne le réseau de la couleur de la
 *  tête, pour qu'il se détache d'un fond clair. */
function Marque({ encre, liseré }: { encre: string; liseré: number }) {
  return (
    <>
      <path d={tete} fill="none" stroke={encre} strokeWidth="3.2" strokeLinecap="round" strokeLinejoin="round" />
      {[
        ...(liseré ? [{ couleur: encre, l: liseré }] : []),
        { couleur: "var(--citron)", l: 0 },
      ].map(({ couleur, l }) => (
        <g key={couleur} fill={couleur} stroke={couleur} strokeWidth={1.5 + 2 * l} strokeLinecap="round">
          {reseau.map(([x, y]) => (
            <line key={`${x}-${y}`} x1="30" y1="27" x2={x} y2={y} />
          ))}
          {[[30, 27, 3.8], ...reseau].map(([x, y, r]) => (
            <circle key={`${x}-${y}`} cx={x} cy={y} r={r + l} stroke="none" />
          ))}
        </g>
      ))}
    </>
  );
}

/** Le logo. Sur fond clair, la tête seule, à l'encre. En sombre, l'icône de
 *  l'app : la tête claire sur sa plaque (`assets/logo/dysflow-app-dark.svg`).
 *  Les deux sont rendus ; le CSS montre le bon (`.logo-clair`, `.logo-sombre`). */
export function Logo({ className = "size-7" }: { className?: string }) {
  return (
    <>
      <svg viewBox="0 0 64 64" className={`logo-clair ${className}`} aria-hidden="true">
        <Marque encre="currentColor" liseré={0.7} />
      </svg>
      <svg viewBox="0 0 64 64" className={`logo-sombre ${className}`} aria-hidden="true">
        <rect x="6.25" y="6.25" width="51.5" height="51.5" rx="11.5" fill="#262625" />
        <g transform="translate(32 32) scale(0.9) translate(-32 -32)">
          <Marque encre="#F2F2EF" liseré={0} />
        </g>
      </svg>
    </>
  );
}

/** Le sélecteur de langue : un lien vers la même page dans l'autre langue. */
function SelecteurLangue({ lang, page }: { lang: Lang; page: Page }) {
  const d = dico(lang).entete;
  return (
    <div role="group" aria-label={d.langues} className="flex items-center rounded-full bg-carte-2 p-0.5 text-sm font-bold">
      {(["en", "fr"] as const).map((l) => {
        const actif = l === lang;
        return (
          <LienLangue
            key={l}
            langue={l}
            href={chemin(l, page)}
            hrefLang={l}
            lang={l}
            aria-current={actif ? "true" : undefined}
            className={`rounded-full px-2.5 py-1 uppercase ${actif ? "bg-carte text-encre" : "text-encre-pale hover:text-encre"}`}
          >
            {l}
          </LienLangue>
        );
      })}
    </div>
  );
}

export function Entete({ lang, page }: { lang: Lang; page: Page }) {
  const d = dico(lang).entete;
  return (
    <header className="sticky top-0 z-20 border-b border-filet/70 bg-fond/85 backdrop-blur">
      {/* En très grand texte, l'en-tête passe sur deux rangs plutôt que de déborder. */}
      <nav className="mx-auto flex min-h-16 max-w-6xl flex-wrap items-center gap-x-6 gap-y-2 px-4 py-3 sm:px-6">
        <Link href={chemin(lang, "accueil")} aria-label={d.accueil} className="flex items-center gap-2 text-lg font-extrabold tracking-tight">
          <Logo />
          Dysflow
        </Link>
        <div className="ml-auto flex flex-wrap items-center justify-end gap-1 text-[15px] text-encre-douce sm:gap-2">
          <Link href={chemin(lang, "accueil", "parcours")} className="hidden rounded-full px-3 py-1.5 hover:bg-carte-2 hover:text-encre sm:block">
            {d.parcours}
          </Link>
          <Link href={chemin(lang, "modeles")} className="hidden rounded-full px-3 py-1.5 hover:bg-carte-2 hover:text-encre min-[420px]:block">
            {d.modeles}
          </Link>
          <a href={depot} className="hidden rounded-full px-3 py-1.5 hover:bg-carte-2 hover:text-encre lg:block">
            {d.github}
          </a>
          <Confort libelles={dico(lang).confort} />
          <SelecteurLangue lang={lang} page={page} />
          <Bouton href={chemin(lang, "telecharger")} petit>
            {d.telecharger}
          </Bouton>
        </div>
      </nav>
    </header>
  );
}

/** L'en-tête et le corps d'une page : `page` dit quelle page le sélecteur de langue vise. */
export function Cadre({ lang, page, children }: { lang: Lang; page: Page; children: ReactNode }) {
  return (
    <>
      <Entete lang={lang} page={page} />
      <main id="contenu" tabIndex={-1} className="flex-1 outline-none">{children}</main>
    </>
  );
}

export function Pied({ lang }: { lang: Lang }) {
  const d = dico(lang).pied;
  return (
    <footer className="mt-24 border-t border-filet">
      <div className="mx-auto flex max-w-6xl flex-col gap-6 px-4 py-10 text-[15px] text-encre-douce sm:flex-row sm:items-center sm:px-6">
        <div className="flex items-center gap-2 font-bold text-encre">
          <Logo className="size-6" />
          Dysflow
        </div>
        <p>{d.licence}</p>
        <div className="flex flex-wrap gap-x-5 gap-y-2 sm:ml-auto">
          <Link href={chemin(lang, "modeles")} className="hover:text-encre">{d.modeles}</Link>
          <Link href={chemin(lang, "telecharger")} className="hover:text-encre">{d.telecharger}</Link>
          <a href={depot} className="hover:text-encre">{d.code}</a>
        </div>
      </div>
    </footer>
  );
}

type BoutonProps = {
  href: string;
  children: ReactNode;
  variante?: "encre" | "citron" | "doux";
  petit?: boolean;
};

const variantes = {
  encre: "bg-encre text-sur-encre hover:opacity-90",
  citron: "bg-citron text-citron-encre hover:brightness-95",
  doux: "bg-carte-2 text-encre hover:bg-filet",
};

/** Un lien en forme de bouton. Interne : `<Link>` ; externe : `<a>`. */
export function Bouton({ href, children, variante = "encre", petit }: BoutonProps) {
  const classes = `inline-flex max-w-full items-center justify-center gap-2 rounded-full text-center font-bold transition ${
    petit ? "px-4 py-2 text-[15px]" : "px-6 py-3.5 text-base"
  } ${variantes[variante]}`;
  if (href.startsWith("/")) {
    return (
      <Link href={href} className={classes}>
        {children}
      </Link>
    );
  }
  return (
    <a href={href} className={classes}>
      {children}
    </a>
  );
}

export function IconeTelecharger({ className = "size-5" }: { className?: string }) {
  return (
    <svg viewBox="0 0 24 24" className={className} fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M12 4v11m0 0-4.5-4.5M12 15l4.5-4.5M5 19h14" />
    </svg>
  );
}

export function Pastille({ children, className = "" }: { children: ReactNode; className?: string }) {
  return (
    <span className={`inline-flex items-center rounded-full px-2.5 py-0.5 text-[13px] font-bold ${className}`}>
      {children}
    </span>
  );
}

export function TitreSection({ surtitre, titre, texte, principal }: { surtitre: string; titre: string; texte?: string; principal?: boolean }) {
  const Titre = principal ? "h1" : "h2";
  return (
    <div className="mx-auto max-w-2xl text-center">
      <p className="text-sm font-bold uppercase tracking-[0.14em] text-encre-pale">{surtitre}</p>
      <Titre className="mt-3 text-balance text-3xl font-extrabold tracking-tight sm:text-4xl">{titre}</Titre>
      {texte && <p className="mt-4 text-pretty text-lg leading-relaxed text-encre-douce">{texte}</p>}
    </div>
  );
}
