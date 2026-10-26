import Link from "next/link";
import type { ReactNode } from "react";

// Le corps d'un article, écrit en markdown réduit : `## titre`, paragraphes,
// listes (`- ` ou `1. `), et dans le texte `**gras**`, `*italique*` et
// `[lien](adresse)`. Assez pour des articles courts, sans dépendance.

/** L'ancre d'un intertitre : minuscules, sans accents, des tirets. */
function ancre(texte: string): string {
  return texte
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");
}

function Lien({ href, children }: { href: string; children: ReactNode }) {
  const classes = "font-bold underline decoration-2 underline-offset-4 hover:decoration-citron";
  return href.startsWith("/") ? (
    <Link href={href} className={classes}>{children}</Link>
  ) : (
    <a href={href} className={classes}>{children}</a>
  );
}

/** Le texte d'une ligne, avec ses liens, son gras et son italique. */
function enLigne(texte: string): ReactNode[] {
  const morceaux: ReactNode[] = [];
  const motif = /\[([^\]]+)\]\(([^)]+)\)|\*\*([^*]+)\*\*|\*([^*]+)\*/g;
  let fin = 0;
  for (const m of texte.matchAll(motif)) {
    if (m.index > fin) morceaux.push(texte.slice(fin, m.index));
    const cle = m.index;
    if (m[1] !== undefined) morceaux.push(<Lien key={cle} href={m[2]}>{m[1]}</Lien>);
    else if (m[3] !== undefined) morceaux.push(<strong key={cle} className="font-extrabold">{m[3]}</strong>);
    else morceaux.push(<em key={cle}>{m[4]}</em>);
    fin = m.index + m[0].length;
  }
  if (fin < texte.length) morceaux.push(texte.slice(fin));
  return morceaux;
}

/** Les intertitres d'un corps, pour le sommaire. */
export function intertitres(corps: string): { id: string; texte: string }[] {
  return corps
    .split("\n")
    .filter((l) => l.startsWith("## "))
    .map((l) => ({ id: ancre(l.slice(3)), texte: l.slice(3) }));
}

export function Texte({ corps }: { corps: string }) {
  const blocs = corps.trim().split(/\n\s*\n/);
  return (
    <>
      {blocs.map((bloc, i) => {
        const lignes = bloc.trim().split("\n");
        if (bloc.startsWith("## ")) {
          const titre = bloc.slice(3).trim();
          return (
            <h2 key={i} id={ancre(titre)} className="mt-14 scroll-mt-24 text-balance text-3xl font-extrabold tracking-tight">
              {titre}
            </h2>
          );
        }
        if (lignes.every((l) => l.startsWith("- "))) {
          return (
            <ul key={i} className="mt-5 space-y-3">
              {lignes.map((l) => (
                <li key={l} className="flex gap-3">
                  <span className="mt-[0.6em] size-2.5 shrink-0 rounded-full bg-citron ring-2 ring-encre/10" aria-hidden="true" />
                  <span className="min-w-0">{enLigne(l.slice(2))}</span>
                </li>
              ))}
            </ul>
          );
        }
        if (lignes.every((l) => /^\d+\. /.test(l))) {
          return (
            <ol key={i} className="mt-5 space-y-3">
              {lignes.map((l, n) => (
                <li key={l} className="flex gap-3">
                  <span className="grid size-8 shrink-0 place-items-center rounded-full bg-carte-2 text-base font-extrabold" aria-hidden="true">
                    {n + 1}
                  </span>
                  <span className="min-w-0 pt-0.5">{enLigne(l.replace(/^\d+\. /, ""))}</span>
                </li>
              ))}
            </ol>
          );
        }
        return (
          <p key={i} className="mt-5">
            {enLigne(lignes.join(" "))}
          </p>
        );
      })}
    </>
  );
}
