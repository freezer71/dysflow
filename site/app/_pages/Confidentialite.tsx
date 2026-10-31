import type { ReactNode } from "react";
import { Opposition } from "../_components/Opposition";
import { Cadre } from "../_components/ui";
import { dico, type Lang } from "../_i18n";

const contact = "contact@koumalabs.org";
const confidentialiteGithub = "https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement";

const lien = "font-bold text-encre underline underline-offset-2";

function Section({ titre, children }: { titre: string; children: ReactNode }) {
  return (
    <section className="mt-12">
      <h2 className="text-2xl font-extrabold">{titre}</h2>
      <div className="mt-4 space-y-4 leading-relaxed text-encre-douce">{children}</div>
    </section>
  );
}

function Courriel() {
  return (
    <a href={`mailto:${contact}`} className={lien}>
      {contact}
    </a>
  );
}

export function Confidentialite({ lang }: { lang: Lang }) {
  const d = dico(lang).confidentialite;
  return (
    <Cadre lang={lang} page="confidentialite">
      <div className="mx-auto max-w-3xl px-4 pt-16 sm:px-6">
        <h1 className="text-balance text-4xl font-extrabold tracking-tight sm:text-5xl">{d.titre}</h1>
        <p className="mt-4 text-pretty text-lg text-encre-douce">{d.intro}</p>
        <p className="mt-3 text-sm text-encre-pale">{d.maj}</p>

        <Section titre={d.responsable.titre}>
          <p>{d.responsable.texte}</p>
          <p>
            {d.responsable.contact} <Courriel />
          </p>
        </Section>

        <Section titre={d.audience.titre}>
          {d.audience.paragraphes.map((p) => (
            <p key={p}>{p}</p>
          ))}
          <p>{d.audience.opposition}</p>
          <Opposition libelles={d.audience} />
        </Section>

        <Section titre={d.stockage.titre}>
          <ul className="list-disc space-y-2 pl-5">
            {d.stockage.elements.map((e) => (
              <li key={e}>{e}</li>
            ))}
          </ul>
        </Section>

        <Section titre={d.hebergement.titre}>
          {d.hebergement.paragraphes.map((p) => (
            <p key={p}>{p}</p>
          ))}
        </Section>

        <Section titre={d.liens.titre}>
          <p>
            {d.liens.texte}{" "}
            <a href={confidentialiteGithub} className={lien}>
              {d.liens.lien}
            </a>
            .
          </p>
        </Section>

        <Section titre={d.droits.titre}>
          {d.droits.paragraphes.map((p) => (
            <p key={p}>{p}</p>
          ))}
          <p>
            {d.droits.ecrire} <Courriel />.
          </p>
          <p>
            {d.droits.cnilAvant}{" "}
            <a href="https://www.cnil.fr/fr/plaintes" className={lien}>
              {d.droits.cnilLien}
            </a>
            .
          </p>
        </Section>
      </div>
    </Cadre>
  );
}
