import Link from "next/link";
import type { ReactNode } from "react";
import { depot } from "../_data/liens";

/** La marque, redessinée à l'encre du thème (le SVG de `assets/logo`). */
export function Logo({ className = "size-7" }: { className?: string }) {
  return (
    <svg viewBox="0 0 64 64" className={className} aria-hidden="true">
      <path
        d="M 45 19 H 19 A 6.5 6.5 0 0 0 19 32 H 45 A 6.5 6.5 0 0 1 45 45 H 28.5"
        fill="none"
        stroke="currentColor"
        strokeWidth="5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <circle cx="19" cy="45" r="3.4" className="fill-[var(--citron)]" stroke="currentColor" strokeWidth="1.2" />
    </svg>
  );
}

export function Entete() {
  return (
    <header className="sticky top-0 z-20 border-b border-filet/70 bg-fond/85 backdrop-blur">
      <nav className="mx-auto flex h-16 max-w-6xl items-center gap-6 px-4 sm:px-6">
        <Link href="/" className="flex items-center gap-2 text-lg font-extrabold tracking-tight">
          <Logo />
          sillage
        </Link>
        <div className="ml-auto flex items-center gap-1 text-[15px] text-encre-douce sm:gap-2">
          <Link href="/#avantages" className="hidden rounded-full px-3 py-1.5 hover:bg-carte-2 hover:text-encre sm:block">
            Avantages
          </Link>
          <Link href="/#comparaison" className="hidden rounded-full px-3 py-1.5 hover:bg-carte-2 hover:text-encre md:block">
            Comparaison
          </Link>
          <Link href="/modeles" className="rounded-full px-3 py-1.5 hover:bg-carte-2 hover:text-encre">
            Modèles
          </Link>
          <a href={depot} className="hidden rounded-full px-3 py-1.5 hover:bg-carte-2 hover:text-encre sm:block">
            GitHub
          </a>
          <Bouton href="/telecharger" petit>
            Télécharger
          </Bouton>
        </div>
      </nav>
    </header>
  );
}

export function Pied() {
  return (
    <footer className="mt-24 border-t border-filet">
      <div className="mx-auto flex max-w-6xl flex-col gap-6 px-4 py-10 text-[15px] text-encre-douce sm:flex-row sm:items-center sm:px-6">
        <div className="flex items-center gap-2 font-bold text-encre">
          <Logo className="size-6" />
          sillage
        </div>
        <p>Logiciel libre, sous licence MIT. Fait en France.</p>
        <div className="flex gap-5 sm:ml-auto">
          <Link href="/modeles" className="hover:text-encre">Modèles</Link>
          <Link href="/telecharger" className="hover:text-encre">Télécharger</Link>
          <a href={depot} className="hover:text-encre">Code source</a>
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
  const classes = `inline-flex items-center justify-center gap-2 rounded-full font-bold transition ${
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

export function TitreSection({ surtitre, titre, texte }: { surtitre: string; titre: string; texte?: string }) {
  return (
    <div className="mx-auto max-w-2xl text-center">
      <p className="text-sm font-bold uppercase tracking-[0.14em] text-encre-pale">{surtitre}</p>
      <h2 className="mt-3 text-balance text-3xl font-extrabold tracking-tight sm:text-4xl">{titre}</h2>
      {texte && <p className="mt-4 text-pretty text-lg leading-relaxed text-encre-douce">{texte}</p>}
    </div>
  );
}
