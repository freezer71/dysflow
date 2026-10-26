import Link from "next/link";
import { DemoDictee } from "../_components/DemoDictee";
import { JsonLd } from "../_components/JsonLd";
import { Simulation } from "../_components/Simulation";
import { absolu, logiciel, organisation, siteWeb } from "../_data/schema";
import { Bouton, Cadre, IconeTelecharger, Pastille, TitreSection } from "../_components/ui";
import { chemin, dico, type Lang } from "../_i18n";

// Une idée par bloc, des phrases courtes : la page s'adresse d'abord à des
// lecteurs pour qui chaque ligne coûte.

// Les tracés des icônes, dans l'ordre des avantages du dictionnaire.
const icones = [
  "M7 3h7l5 5v13H7V3Zm7 0v5h5M10 13h6M10 17h6",
  "M13 2 4 14h7l-1 8 9-12h-7l1-8Z",
  "M12 3 5 6v6c0 4.2 3 7.6 7 9 4-1.4 7-4.8 7-9V6l-7-3Zm-3 9 2 2 4-4",
  "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18Zm3-12a4 4 0 1 0 0 6",
];

/** Une suite de gestes, numérotés, avec les touches à presser. */
function Gestes({ titre, etapes, accent }: {
  titre: string;
  etapes: readonly { texte: string; touches?: readonly { signe: string; nom: string }[] }[];
  accent?: boolean;
}) {
  return (
    <article className="rounded-[28px] bg-carte p-6 sm:p-8">
      <h3 className="text-2xl font-extrabold tracking-tight sm:text-3xl">{titre}</h3>
      <ol className="mt-6 space-y-5">
        {etapes.map((e, i) => (
          <li key={e.texte} className="flex items-start gap-4">
            <span
              className={`grid size-10 shrink-0 place-items-center rounded-full text-lg font-extrabold ${
                accent && i === etapes.length - 1 ? "bg-citron text-citron-encre" : "bg-carte-2"
              }`}
              aria-hidden="true"
            >
              {i + 1}
            </span>
            <div className="min-w-0 pt-1.5">
              <p className="text-xl font-bold leading-snug">{e.texte}</p>
              {e.touches && (
                <p className="mt-3 flex flex-wrap items-center gap-1.5">
                  {e.touches.map((t, n) => (
                    <span key={t.nom} className="flex items-center gap-1.5">
                      {n > 0 && <span className="text-encre-pale" aria-hidden="true">+</span>}
                      {/* Comme sur le clavier du Mac : le symbole en haut, le nom en bas. */}
                      <kbd className="flex min-w-16 flex-col items-start rounded-xl border-b-4 border-filet bg-fond px-3 pb-1.5 pt-1 font-sans font-bold leading-tight">
                        <span className="text-xl" aria-hidden="true">{t.signe}</span>
                        <span className="text-sm">{t.nom}</span>
                      </kbd>
                    </span>
                  ))}
                </p>
              )}
            </div>
          </li>
        ))}
      </ol>
    </article>
  );
}

/** Une étape du parcours : son numéro sur le fil, son texte, son image. */
function Etape({ numero, titre, texte, accent, dernier, children }: {
  numero: number;
  titre: string;
  texte: string;
  accent?: boolean;
  dernier?: boolean;
  children: React.ReactNode;
}) {
  return (
    <li className="relative grid gap-6 pb-16 lg:grid-cols-[0.8fr_1.2fr] lg:gap-12 lg:pl-20">
      {/* Le fil qui relie les étapes, comme la ligne que l'œil suit. */}
      {!dernier && <span className="absolute bottom-0 left-[23px] top-14 hidden w-[2px] bg-filet lg:block" aria-hidden="true" />}
      <div className="flex gap-4 lg:block lg:pt-2">
        <span
          className={`grid size-10 shrink-0 place-items-center rounded-full text-lg font-extrabold lg:absolute lg:left-0 lg:top-0 lg:size-12 ${
            accent ? "bg-citron text-citron-encre" : "bg-encre text-sur-encre"
          }`}
          aria-hidden="true"
        >
          {numero}
        </span>
        <div>
          <h3 className="text-balance break-words text-2xl font-extrabold leading-tight tracking-tight sm:text-3xl">{titre}</h3>
          <p className="mt-3 text-xl leading-relaxed text-encre-douce">{texte}</p>
        </div>
      </div>
      <div className="min-w-0">{children}</div>
    </li>
  );
}

export function Accueil({ lang }: { lang: Lang }) {
  const d = dico(lang).accueil;
  const p = d.parcours;
  const donnees = {
    "@context": "https://schema.org",
    "@graph": [
      organisation,
      siteWeb(lang),
      logiciel(lang),
      {
        "@type": "FAQPage",
        "@id": absolu(`${chemin(lang, "accueil")}#questions`),
        inLanguage: lang,
        mainEntity: d.faq.map((f) => ({
          "@type": "Question",
          name: f.q,
          acceptedAnswer: { "@type": "Answer", text: f.r },
        })),
      },
    ],
  };
  return (
    <Cadre lang={lang} page="accueil">
      <JsonLd donnees={donnees} />
      {/* Accroche */}
      <section className="mx-auto max-w-4xl px-4 pb-20 pt-14 text-center sm:px-6 lg:pt-24">
        <div className="flex flex-wrap justify-center gap-2">
          <Pastille className="bg-citron text-citron-encre">{d.pastilles[0]}</Pastille>
          <Pastille className="bg-carte-2">{d.pastilles[1]}</Pastille>
          <Pastille className="bg-carte-2">{d.pastilles[2]}</Pastille>
        </div>
        <h1 className="mt-6 text-balance break-words text-4xl font-extrabold leading-[1.05] tracking-tight hyphens-auto min-[400px]:text-5xl sm:text-6xl lg:text-7xl">
          {d.titre}
        </h1>
        <p className="mx-auto mt-6 max-w-xl text-pretty text-2xl leading-relaxed text-encre-douce">{d.intro}</p>
        <div className="mt-9 flex flex-wrap justify-center gap-3">
          <Bouton href={chemin(lang, "telecharger")}>
            <IconeTelecharger />
            {d.telecharger}
          </Bouton>
          <Bouton href="#parcours" variante="doux">
            {p.surtitre} ↓
          </Bouton>
        </div>
      </section>

      {/* Le parcours : le même message, lu sans puis avec Dysflow */}
      <section id="parcours" className="mx-auto max-w-6xl scroll-mt-20 px-4 sm:px-6">
        <TitreSection surtitre={p.surtitre} titre={p.titre} />
        <ol className="mt-14">
          <Etape numero={1} titre={p.lire.titre} texte={p.lire.texte}>
            <Simulation lignes={p.simulation.lignes} modeInitial="sans" libelles={p.simulation} />
            <p className="mt-3 px-2 text-sm text-encre-pale">{p.lire.note}</p>
          </Etape>
          <Etape numero={2} titre={p.cout.titre} texte={p.cout.texte}>
            <ul className="space-y-3">
              {p.cout.points.map((point) => (
                <li key={point} className="rounded-[22px] bg-carte-2/70 px-6 py-5 text-xl font-bold">
                  {point}
                </li>
              ))}
            </ul>
          </Etape>
          <Etape numero={3} titre={p.avec.titre} texte={p.avec.texte} accent>
            <Simulation lignes={p.simulation.lignes} modeInitial="avec" libelles={p.simulation} />
          </Etape>
          <Etape numero={4} titre={p.ecrire.titre} texte={p.ecrire.texte} accent dernier>
            <DemoDictee libelles={p.dictee} tapee={p.dictee.tapee} dite={p.dictee.dite} />
          </Etape>
        </ol>
      </section>

      {/* Mode d'emploi : les deux gestes */}
      <section id="mode-emploi" className="mx-auto max-w-6xl scroll-mt-20 px-4 pt-12 sm:px-6">
        <TitreSection surtitre={d.modeEmploi.surtitre} titre={d.modeEmploi.titre} />
        <div className="mt-12 grid gap-4 lg:grid-cols-2">
          <Gestes titre={d.modeEmploi.lire.titre} etapes={d.modeEmploi.lire.etapes} accent />
          <Gestes titre={d.modeEmploi.ecrire.titre} etapes={d.modeEmploi.ecrire.etapes} accent />
        </div>
        <p className="mt-5 text-center text-lg text-encre-douce">{d.modeEmploi.note}</p>
      </section>

      {/* Partout : les applications où ça marche */}
      <section id="partout" className="mx-auto max-w-4xl scroll-mt-20 px-4 pt-28 text-center sm:px-6">
        <TitreSection surtitre={d.partout.surtitre} titre={d.partout.titre} texte={d.partout.texte} />
        <ul className="mt-10 flex flex-wrap justify-center gap-2.5">
          {d.partout.apps.map((a) => (
            <li key={a.icone} className="flex items-center gap-2.5 rounded-full bg-carte py-2 pl-2 pr-5 text-lg font-bold">
              {/* Le nom est écrit à côté : l'icône n'a rien à dire de plus. */}
              {/* eslint-disable-next-line @next/next/no-img-element */}
              <img src={`/apps/${a.icone}.webp`} alt="" width={36} height={36} loading="lazy" className="size-9" />
              {a.nom}
            </li>
          ))}
          <li className="flex items-center rounded-full bg-citron px-5 py-2.5 text-lg font-bold text-citron-encre">{d.partout.plus}</li>
        </ul>
      </section>

      {/* Manifeste */}
      <section className="mx-auto max-w-6xl px-4 pt-28 sm:px-6">
        <div className="rounded-[32px] bg-encre px-8 py-14 text-sur-encre sm:px-14 sm:py-20">
          <h2 className="text-balance break-words text-4xl font-extrabold tracking-tight hyphens-auto min-[400px]:text-5xl sm:text-6xl">{d.manifeste.titre}</h2>
          <ul className="mt-10 space-y-4">
            {d.manifeste.lignes.map((l) => (
              <li key={l} className="flex gap-4 text-2xl font-bold leading-snug sm:text-3xl">
                <span className="mt-[0.45em] size-3 shrink-0 rounded-full bg-citron" aria-hidden="true" />
                {l}
              </li>
            ))}
          </ul>
          <p className="mt-10 text-xl opacity-70">{d.manifeste.texte}</p>
        </div>
      </section>

      {/* Avantages */}
      <section id="avantages" className="mx-auto max-w-6xl scroll-mt-20 px-4 pt-28 sm:px-6">
        <TitreSection surtitre={d.avantagesTitre.surtitre} titre={d.avantagesTitre.titre} />
        <div className="mt-12 grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
          {d.avantages.map((a, i) => (
            <article key={a.titre} className="rounded-[22px] bg-carte p-6">
              <span className="grid size-11 place-items-center rounded-2xl bg-citron text-citron-encre">
                <svg viewBox="0 0 24 24" className="size-5" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                  <path d={icones[i]} />
                </svg>
              </span>
              <h3 className="mt-5 text-xl font-extrabold leading-snug">{a.titre}</h3>
              <p className="mt-2 text-lg leading-relaxed text-encre-douce">{a.texte}</p>
            </article>
          ))}
        </div>
      </section>

      {/* Questions */}
      <section className="mx-auto max-w-3xl px-4 pt-28 sm:px-6">
        <TitreSection surtitre={d.questionsTitre.surtitre} titre={d.questionsTitre.titre} />
        <div className="mt-12 space-y-3">
          {d.faq.map((f) => (
            <details key={f.q} className="group rounded-[20px] bg-carte px-6 py-5">
              <summary className="flex cursor-pointer list-none items-center justify-between gap-4 text-xl font-extrabold">
                <span className="min-w-0 break-words">{f.q}</span>
                <span className="grid size-8 shrink-0 place-items-center rounded-full bg-carte-2 transition group-open:rotate-45" aria-hidden="true">
                  +
                </span>
              </summary>
              <p className="mt-3 text-lg leading-relaxed text-encre-douce">{f.r}</p>
            </details>
          ))}
        </div>
      </section>

      {/* Appel final */}
      <section className="mx-auto max-w-6xl px-4 pt-28 sm:px-6">
        <div className="rounded-[32px] bg-citron px-8 py-14 text-center text-citron-encre sm:py-20">
          <h2 className="mx-auto max-w-2xl text-balance text-4xl font-extrabold tracking-tight sm:text-5xl">{d.final.titre}</h2>
          <p className="mx-auto mt-4 max-w-xl text-xl opacity-75">{d.final.texte}</p>
          <div className="mt-8 flex flex-wrap justify-center gap-3">
            <Link
              href={chemin(lang, "telecharger")}
              className="inline-flex items-center gap-2 rounded-full bg-[#191919] px-6 py-3.5 font-bold text-white hover:opacity-90"
            >
              <IconeTelecharger />
              {d.telecharger}
            </Link>
          </div>
        </div>
      </section>
    </Cadre>
  );
}
