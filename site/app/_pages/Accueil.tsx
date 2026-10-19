import Link from "next/link";
import { Commande } from "../_components/Commande";
import { DemoLecteur } from "../_components/DemoLecteur";
import { Bouton, Cadre, IconeTelecharger, Pastille, TitreSection } from "../_components/ui";
import { commandeInstallation, depot } from "../_data/liens";
import { dictee, lecture } from "../_data/modeles";
import { chemin, dico, type Lang } from "../_i18n";

// Les tracés des icônes, dans l'ordre des avantages du dictionnaire.
const icones = [
  "M12 3 5 6v6c0 4.2 3 7.6 7 9 4-1.4 7-4.8 7-9V6l-7-3Zm-3 9 2 2 4-4",
  "M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12Zm10 3a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z",
  "M7 3h7l5 5v13H7V3Zm7 0v5h5M10 13h6M10 17h6",
  "M8 7 3 12l5 5M16 7l5 5-5 5M14 4l-4 16",
  "M13 2 4 14h7l-1 8 9-12h-7l1-8Z",
  "M6 3h12v18l-6-4-6 4V3Z",
  "M12 3a3 3 0 0 0-3 3v6a3 3 0 0 0 6 0V6a3 3 0 0 0-3-3Zm-7 9a7 7 0 0 0 14 0M12 19v3",
  "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18Zm3-12a4 4 0 1 0 0 6",
];

const nombreDeModeles = lecture.length + dictee.length;

function Valeur({ v, sillage, mots }: { v: string; sillage?: boolean; mots: { oui: string; non: string; partiel: string } }) {
  if (v === "@oui")
    return (
      <span className={`inline-grid size-7 place-items-center rounded-full ${sillage ? "bg-citron text-citron-encre" : "bg-carte-2 text-encre"}`} aria-label={mots.oui}>
        <svg viewBox="0 0 24 24" className="size-4" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round"><path d="m5 12 5 5 9-10" /></svg>
      </span>
    );
  if (v === "@non")
    return <span className="text-xl text-encre-pale" aria-label={mots.non}>—</span>;
  if (v === "@partiel")
    return <span className="text-sm text-encre-douce">{mots.partiel}</span>;
  return <span className="text-sm leading-snug text-encre-douce">{v}</span>;
}

export function Accueil({ lang }: { lang: Lang }) {
  const d = dico(lang).accueil;
  const vitrine = [lecture[0], dictee[0], dictee[1]];
  const libelles = dico(lang).commande;
  return (
    <Cadre lang={lang} page="accueil">
      {/* Accroche */}
      <section className="mx-auto grid max-w-6xl items-center gap-12 px-4 pb-20 pt-14 sm:px-6 lg:grid-cols-[1.05fr_1fr] lg:pt-24">
        <div className="min-w-0">
          <div className="flex flex-wrap gap-2">
            <Pastille className="bg-citron text-citron-encre">{d.pastilles[0]}</Pastille>
            <Pastille className="bg-carte-2">{d.pastilles[1]}</Pastille>
            <Pastille className="bg-carte-2">{d.pastilles[2]}</Pastille>
          </div>
          <h1 className="mt-6 text-balance text-5xl font-extrabold leading-[1.02] tracking-tight sm:text-6xl lg:text-7xl">
            {d.titre}
          </h1>
          <p className="mt-6 max-w-xl text-pretty text-xl leading-relaxed text-encre-douce">
            {d.intro}
          </p>
          <div className="mt-9 flex flex-wrap gap-3">
            <Bouton href={chemin(lang, "telecharger")}>
              <IconeTelecharger />
              {d.telecharger}
            </Bouton>
            <Bouton href={depot} variante="doux">
              {d.github}
            </Bouton>
          </div>
          <div className="mt-8 max-w-xl">
            <p className="mb-2 text-[15px] text-encre-pale">{d.commande}</p>
            <Commande texte={commandeInstallation} libelles={libelles} />
          </div>
        </div>
        <DemoLecteur fichier={dico(lang).demo.fichier} titre={dico(lang).demo.titre} lignes={dico(lang).demo.lignes} vitesse={dico(lang).demo.vitesse} />
      </section>

      {/* Chiffres */}
      <section className="mx-auto max-w-6xl px-4 sm:px-6">
        <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
          {d.chiffres.map((c) => (
            <div key={c.legende} className="rounded-[22px] bg-carte p-6">
              <p className="text-4xl font-extrabold tracking-tight">{c.valeur.replace("#", `${nombreDeModeles}`)}</p>
              <p className="mt-1 text-[15px] text-encre-douce">{c.legende}</p>
            </div>
          ))}
        </div>
      </section>

      {/* Avantages */}
      <section id="avantages" className="mx-auto max-w-6xl scroll-mt-20 px-4 pt-28 sm:px-6">
        <TitreSection
          surtitre={d.avantagesTitre.surtitre}
          titre={d.avantagesTitre.titre}
          texte={d.avantagesTitre.texte}
        />
        <div className="mt-14 grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
          {d.avantages.map((a, i) => (
            <article key={a.titre} className="rounded-[22px] bg-carte p-6">
              <span className="grid size-11 place-items-center rounded-2xl bg-citron text-citron-encre">
                <svg viewBox="0 0 24 24" className="size-5" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                  <path d={icones[i]} />
                </svg>
              </span>
              <h3 className="mt-5 text-lg font-extrabold leading-snug">{a.titre}</h3>
              <p className="mt-2 text-[15px] leading-relaxed text-encre-douce">{a.texte}</p>
            </article>
          ))}
        </div>
      </section>

      {/* Pour qui */}
      <section className="mx-auto max-w-6xl px-4 pt-28 sm:px-6">
        <div className="rounded-[32px] bg-encre p-8 text-sur-encre sm:p-12">
          <p className="text-sm font-bold uppercase tracking-[0.14em] opacity-60">{d.pourQui.surtitre}</p>
          <h2 className="mt-3 max-w-2xl text-balance text-3xl font-extrabold tracking-tight sm:text-4xl">
            {d.pourQui.titre}
          </h2>
          <div className="mt-10 grid gap-8 md:grid-cols-3">
            {d.pourQui.publics.map((p) => (
              <div key={p.titre} className="border-t border-current/20 pt-5">
                <h3 className="text-lg font-extrabold">{p.titre}</h3>
                <p className="mt-2 leading-relaxed opacity-75">{p.texte}</p>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* Comparaison */}
      <section id="comparaison" className="mx-auto max-w-6xl scroll-mt-20 px-4 pt-28 sm:px-6">
        <TitreSection
          surtitre={d.comparaisonTitre.surtitre}
          titre={d.comparaisonTitre.titre}
          texte={d.comparaisonTitre.texte}
        />
        <div className="mt-14 overflow-x-auto rounded-[22px] bg-carte">
          <table className="w-full min-w-[760px] border-collapse text-left">
            <thead>
              <tr>
                <th className="w-[28%] p-5 text-sm font-bold text-encre-pale">{d.comparaison.critere}</th>
                {d.comparaison.concurrents.map((c, i) => (
                  <th key={c} className={`p-5 text-center text-[15px] font-extrabold ${i === 0 ? "bg-bande" : ""}`}>
                    {c}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {d.comparaison.lignes.map((ligne) => (
                <tr key={ligne.critere} className="border-t border-filet">
                  <td className="p-5 font-bold">{ligne.critere}</td>
                  {ligne.cases.map((v, i) => (
                    <td key={i} className={`p-5 text-center ${i === 0 ? "bg-bande" : ""}`}>
                      <Valeur v={v} sillage={i === 0} mots={d.comparaison} />
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <p className="mt-4 text-center text-sm text-encre-pale">
          {d.comparaison.note}{" "}
          <a href={`${depot}/issues`} className="underline underline-offset-2 hover:text-encre">{d.comparaison.noteLien}</a>.
        </p>
      </section>

      {/* Modèles */}
      <section className="mx-auto max-w-6xl px-4 pt-28 sm:px-6">
        <TitreSection
          surtitre={d.modelesTitre.surtitre}
          titre={d.modelesTitre.titre}
          texte={d.modelesTitre.texte}
        />
        <div className="mt-14 grid gap-3 md:grid-cols-3">
          {vitrine.map((m, i) => (
            <div key={m.id} className="rounded-[22px] bg-carte p-6">
              <p className="text-sm font-bold text-encre-pale">{i === 0 ? d.lecture : d.dictee}</p>
              <h3 className="mt-2 text-xl font-extrabold">{m.nom[lang]}</h3>
              <p className="mt-2 text-[15px] leading-relaxed text-encre-douce">{m.description[lang]}</p>
              <div className="mt-5 flex flex-wrap gap-2">
                <Pastille className="bg-carte-2">{m.taille[lang]}</Pastille>
                <Pastille className="bg-carte-2">{m.langues[lang]}</Pastille>
              </div>
            </div>
          ))}
        </div>
        <div className="mt-8 text-center">
          <Bouton href={chemin(lang, "modeles")} variante="doux">
            {d.voirModeles.replace("#", `${nombreDeModeles}`)}
          </Bouton>
        </div>
      </section>

      {/* Questions */}
      <section className="mx-auto max-w-3xl px-4 pt-28 sm:px-6">
        <TitreSection surtitre={d.questionsTitre.surtitre} titre={d.questionsTitre.titre} />
        <div className="mt-12 space-y-3">
          {d.faq.map((f) => (
            <details key={f.q} className="group rounded-[20px] bg-carte px-6 py-5">
              <summary className="flex cursor-pointer list-none items-center justify-between gap-4 text-lg font-extrabold">
                {f.q}
                <span className="grid size-8 shrink-0 place-items-center rounded-full bg-carte-2 transition group-open:rotate-45" aria-hidden="true">
                  +
                </span>
              </summary>
              <p className="mt-3 leading-relaxed text-encre-douce">{f.r}</p>
            </details>
          ))}
        </div>
      </section>

      {/* Appel final */}
      <section className="mx-auto max-w-6xl px-4 pt-28 sm:px-6">
        <div className="rounded-[32px] bg-citron px-8 py-14 text-center text-citron-encre sm:py-20">
          <h2 className="mx-auto max-w-2xl text-balance text-4xl font-extrabold tracking-tight sm:text-5xl">
            {d.final.titre}
          </h2>
          <p className="mx-auto mt-4 max-w-xl text-lg opacity-75">
            {d.final.texte}
          </p>
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
