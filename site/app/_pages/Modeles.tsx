import { Bouton, Cadre, IconeTelecharger, Pastille, TitreSection } from "../_components/ui";
import { dictee, lecture, type Modele } from "../_data/modeles";
import { chemin, dico, type Lang } from "../_i18n";

function Fiche({ m, lang }: { m: Modele; lang: Lang }) {
  const d = dico(lang).modeles;
  const lignes = [
    [d.champs.taille, m.taille[lang]],
    [d.champs.precision, m.precision[lang]],
    [d.champs.langues, m.langues[lang]],
    [d.champs.calcul, m.calcul[lang]],
    [d.champs.rythme, m.rythme[lang]],
    ...(m.voix ? [[d.champs.voix, m.voix[lang]]] : []),
    [d.champs.licence, m.licence[lang]],
  ];
  return (
    <article className={`flex flex-col rounded-[22px] bg-carte p-6 ${m.recommande ? "ring-2 ring-citron" : ""}`}>
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-sm font-bold text-encre-pale">{m.famille[lang]}</span>
        {m.recommande && <Pastille className="bg-citron text-citron-encre">{d.recommande}</Pastille>}
        {m.lourd && <Pastille className="bg-carte-2 text-encre-douce">{d.lourd}</Pastille>}
        <Pastille className="ml-auto bg-carte-2 text-encre-douce">{m.plateforme[lang]}</Pastille>
      </div>
      <h3 className="mt-3 text-xl font-extrabold">{m.nom[lang]}</h3>
      <p className="mt-2 text-[15px] leading-relaxed text-encre-douce">{m.description[lang]}</p>
      <dl className="mt-5 grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 border-t border-filet pt-4 text-[15px]">
        {lignes.map(([cle, valeur]) => (
          <div key={cle} className="contents">
            <dt className="text-encre-pale">{cle}</dt>
            <dd className="font-bold">{valeur}</dd>
          </div>
        ))}
      </dl>
      {m.source && (
        <a href={m.source} className="mt-5 text-sm font-bold text-encre-douce underline underline-offset-2 hover:text-encre">
          {d.source}
        </a>
      )}
    </article>
  );
}

function Tableau({ modeles, lang, nom }: { modeles: Modele[]; lang: Lang; nom: string }) {
  const d = dico(lang).modeles.champs;
  return (
    // Défilable, donc atteignable au clavier : les flèches font défiler.
    <div tabIndex={0} role="region" aria-label={nom} className="mt-8 overflow-x-auto rounded-[22px] bg-carte">
      <table className="w-full min-w-[640px] border-collapse text-left text-[15px]">
        <thead className="text-sm text-encre-pale">
          <tr>
            <th className="p-4 font-bold">{d.modele}</th>
            <th className="p-4 font-bold">{d.taille}</th>
            <th className="p-4 font-bold">{d.langues}</th>
            <th className="p-4 font-bold">{d.rythme}</th>
          </tr>
        </thead>
        <tbody>
          {modeles.map((m) => (
            <tr key={m.id} className="border-t border-filet">
              <td className="p-4 font-extrabold">{m.nom[lang]}</td>
              <td className="p-4 tabular-nums">{m.taille[lang]}</td>
              <td className="p-4">{m.langues[lang]}</td>
              <td className="p-4 text-encre-douce">{m.rythme[lang]}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export function Modeles({ lang }: { lang: Lang }) {
  const d = dico(lang).modeles;
  return (
    <Cadre lang={lang} page="modeles">
      <div className="mx-auto max-w-6xl px-4 pt-16 sm:px-6">
        <TitreSection
          surtitre={d.entete.surtitre}
          titre={d.entete.titre}
          texte={d.entete.texte}
          principal
        />

        <section id="lecture" className="scroll-mt-20 pt-20">
          <h2 className="text-3xl font-extrabold tracking-tight">{d.lecture.titre}</h2>
          <p className="mt-2 max-w-2xl text-lg text-encre-douce">
            {d.lecture.texte}
          </p>
          <Tableau modeles={lecture} lang={lang} nom={d.lecture.titre} />
          <div className="mt-6 grid gap-3 md:grid-cols-2 lg:grid-cols-3">
            {lecture.map((m) => (
              <Fiche key={m.id} m={m} lang={lang} />
            ))}
          </div>
        </section>

        <section id="dictee" className="scroll-mt-20 pt-24">
          <h2 className="text-3xl font-extrabold tracking-tight">{d.dictee.titre}</h2>
          <p className="mt-2 max-w-2xl text-lg text-encre-douce">
            {d.dictee.texte}
          </p>
          <Tableau modeles={dictee} lang={lang} nom={d.dictee.titre} />
          <div className="mt-6 grid gap-3 md:grid-cols-2 lg:grid-cols-3">
            {dictee.map((m) => (
              <Fiche key={m.id} m={m} lang={lang} />
            ))}
          </div>
        </section>

        <section className="pt-24">
          <div className="rounded-[22px] bg-carte p-8">
            <h2 className="text-2xl font-extrabold">{d.ocr.titre}</h2>
            <p className="mt-2 max-w-3xl leading-relaxed text-encre-douce">
              {d.ocr.texte}
            </p>
          </div>
          <p className="mt-6 text-sm text-encre-pale">
            {d.mesures}
          </p>
          <div className="mt-10 text-center">
            <Bouton href={chemin(lang, "telecharger")}>
              <IconeTelecharger />
              {d.telecharger}
            </Bouton>
          </div>
        </section>
      </div>
    </Cadre>
  );
}
