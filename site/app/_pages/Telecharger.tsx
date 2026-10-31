import { Commande } from "../_components/Commande";
import { Bouton, Cadre, IconeTelecharger, Pastille } from "../_components/ui";
import { commandeDesinstallation, commandeInstallation, depot, derniereVersion, telechargements } from "../_data/liens";
import { dico, type Lang } from "../_i18n";

export async function Telecharger({ lang }: { lang: Lang }) {
  const d = dico(lang).telecharger;
  const libelles = dico(lang).commande;
  const version = await derniereVersion();
  return (
    <Cadre lang={lang} page="telecharger">
      <div className="mx-auto max-w-4xl px-4 pt-16 sm:px-6">
        <div className="text-center">
          <p className="text-sm font-bold uppercase tracking-[0.14em] text-encre-pale">
            {version ? `${d.version} ${version}` : d.derniere}
          </p>
          <h1 className="mt-3 text-balance text-4xl font-extrabold tracking-tight sm:text-5xl">
            {d.titre}
          </h1>
          <p className="mx-auto mt-4 max-w-xl text-pretty text-lg text-encre-douce">
            {d.intro}
          </p>
        </div>

        <div className="mt-10">
          <Commande texte={commandeInstallation} libelles={libelles} grande objectif="copy_install_command" />
          <ul className="mt-6 grid gap-2 sm:grid-cols-2">
            {d.etapes.map((e) => (
              <li key={e} className="flex items-center gap-3 rounded-2xl bg-carte px-4 py-3 text-[15px]">
                <span className="grid size-6 shrink-0 place-items-center rounded-full bg-citron text-citron-encre" aria-hidden="true">
                  <svg viewBox="0 0 24 24" className="size-3.5" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round"><path d="m5 12 5 5 9-10" /></svg>
                </span>
                {e}
              </li>
            ))}
          </ul>
        </div>

        <div className="mt-16 flex items-center gap-4 text-sm font-bold uppercase tracking-[0.14em] text-encre-pale">
          <span className="h-px flex-1 bg-filet" />
          {d.ou}
          <span className="h-px flex-1 bg-filet" />
        </div>

        <div className="mt-8 grid gap-4 md:grid-cols-2">
          <section className="flex min-w-0 flex-col rounded-[28px] bg-carte p-6 sm:p-8">
            <div className="flex items-center gap-2">
              <h2 className="text-2xl font-extrabold">Mac</h2>
              <Pastille className="bg-carte-2 text-encre-douce">{d.mac.pastille}</Pastille>
            </div>
            <p className="mt-2 leading-relaxed text-encre-douce">
              {d.mac.texte}
            </p>
            <div className="mt-6">
              <Bouton href={telechargements.dmg} objectif="download">
                <IconeTelecharger />
                {d.mac.bouton}
              </Bouton>
            </div>
            <p className="mt-5 text-sm leading-relaxed text-encre-pale">
              {d.mac.note}
            </p>
          </section>

          <section className="flex min-w-0 flex-col rounded-[28px] bg-carte p-6 sm:p-8">
            <div className="flex items-center gap-2">
              <h2 className="text-2xl font-extrabold">Linux</h2>
              <Pastille className="bg-carte-2 text-encre-douce">{d.linux.pastille}</Pastille>
            </div>
            <p className="mt-2 leading-relaxed text-encre-douce">
              {d.linux.texte}
            </p>
            <div className="mt-6 flex flex-wrap gap-3">
              <Bouton href={telechargements.linuxX64} variante="doux" objectif="download">
                <IconeTelecharger />
                x86_64
              </Bouton>
              <Bouton href={telechargements.linuxArm64} variante="doux" objectif="download">
                <IconeTelecharger />
                ARM64
              </Bouton>
            </div>
            <p className="mt-5 text-sm leading-relaxed text-encre-pale">
              {d.linux.archivesAvant}{" "}
              <a href={telechargements.publications} className="underline underline-offset-2 hover:text-encre">
                {d.linux.archivesLien}
              </a>
              .
            </p>
          </section>
        </div>

        <section className="mt-4 rounded-[28px] bg-carte p-6 sm:p-8">
          <h2 className="text-xl font-extrabold">{d.miseAJour.titre}</h2>
          <p className="mt-2 text-encre-douce">
            {d.miseAJour.texte}
          </p>
          <div className="mt-4">
            <Commande texte="dysflow --update" libelles={libelles} />
          </div>
        </section>

        <section className="mt-4 rounded-[28px] bg-carte p-6 sm:p-8">
          <h2 className="text-xl font-extrabold">{d.desinstaller.titre}</h2>
          <p className="mt-2 text-encre-douce">{d.desinstaller.texte}</p>
          <div className="mt-4">
            <Commande texte={commandeDesinstallation} libelles={libelles} />
          </div>
          <p className="mt-3 text-encre-douce">
            {d.desinstaller.garder} <code className="font-mono text-encre">--keep-data</code>.
          </p>
        </section>

        <p className="mt-10 text-center text-encre-douce">
          {d.compilerAvant}{" "}
          <a href={telechargements.sources} className="font-bold text-encre underline underline-offset-2">
            {d.compilerLien}
          </a>
          . {d.souciAvant}{" "}
          <a href={`${depot}/issues`} className="font-bold text-encre underline underline-offset-2">
            {d.souciLien}
          </a>
          .
        </p>
      </div>
    </Cadre>
  );
}
