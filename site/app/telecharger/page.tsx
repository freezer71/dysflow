import type { Metadata } from "next";
import { Commande } from "../_components/Commande";
import { Bouton, IconeTelecharger, Pastille } from "../_components/ui";
import { commandeDesinstallation, commandeInstallation, depot, derniereVersion, telechargements } from "../_data/liens";

export const metadata: Metadata = {
  title: "Télécharger",
  description: "Installez sillon en une commande sur Mac et Linux, ou téléchargez l'app macOS en .dmg.",
};

const etapes = [
  "Choisit la bonne version pour votre machine",
  "Vérifie l'empreinte du téléchargement",
  "Installe sans mot de passe administrateur",
  "Ouvre sillon — la voix s'installe toute seule",
];

export default async function Telecharger() {
  const version = await derniereVersion();
  return (
    <div className="mx-auto max-w-4xl px-4 pt-16 sm:px-6">
      <div className="text-center">
        <p className="text-sm font-bold uppercase tracking-[0.14em] text-encre-pale">
          {version ? `Version ${version}` : "Dernière version"}
        </p>
        <h1 className="mt-3 text-balance text-4xl font-extrabold tracking-tight sm:text-5xl">
          Installé en une commande.
        </h1>
        <p className="mx-auto mt-4 max-w-xl text-pretty text-lg text-encre-douce">
          Ouvrez le Terminal, collez, Entrée. Sur Mac comme sous Linux.
        </p>
      </div>

      <div className="mt-10">
        <Commande texte={commandeInstallation} grande />
        <ul className="mt-6 grid gap-2 sm:grid-cols-2">
          {etapes.map((e) => (
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
        ou téléchargez
        <span className="h-px flex-1 bg-filet" />
      </div>

      <div className="mt-8 grid gap-4 md:grid-cols-2">
        <section className="flex min-w-0 flex-col rounded-[28px] bg-carte p-6 sm:p-8">
          <div className="flex items-center gap-2">
            <h2 className="text-2xl font-extrabold">Mac</h2>
            <Pastille className="bg-carte-2 text-encre-douce">Puce Apple</Pastille>
          </div>
          <p className="mt-2 leading-relaxed text-encre-douce">
            Ouvrez le .dmg et glissez Sillon dans Applications. macOS 14 ou plus ; la dictée demande
            macOS 26.
          </p>
          <div className="mt-6">
            <Bouton href={telechargements.dmg}>
              <IconeTelecharger />
              Télécharger le .dmg
            </Bouton>
          </div>
          <p className="mt-5 text-sm leading-relaxed text-encre-pale">
            L&apos;app n&apos;est pas encore notariée par Apple : au premier lancement, ouvrez{" "}
            <em>Réglages Système › Confidentialité et sécurité</em> et cliquez sur <em>Ouvrir quand même</em>.
            La commande ci-dessus évite ce détour.
          </p>
        </section>

        <section className="flex min-w-0 flex-col rounded-[28px] bg-carte p-6 sm:p-8">
          <div className="flex items-center gap-2">
            <h2 className="text-2xl font-extrabold">Linux</h2>
            <Pastille className="bg-carte-2 text-encre-douce">Terminal</Pastille>
          </div>
          <p className="mt-2 leading-relaxed text-encre-douce">
            Le lecteur au terminal, avec Kokoro et la reconnaissance de texte. Ubuntu 24.04, Debian 13,
            Fedora 40 ou plus récents.
          </p>
          <div className="mt-6 flex flex-wrap gap-3">
            <Bouton href={telechargements.linuxX64} variante="doux">
              <IconeTelecharger />
              x86_64
            </Bouton>
            <Bouton href={telechargements.linuxArm64} variante="doux">
              <IconeTelecharger />
              ARM64
            </Bouton>
          </div>
          <p className="mt-5 text-sm leading-relaxed text-encre-pale">
            Les archives et leurs empreintes sont aussi dans les{" "}
            <a href={telechargements.publications} className="underline underline-offset-2 hover:text-encre">
              publications GitHub
            </a>
            .
          </p>
        </section>
      </div>

      <section className="mt-4 rounded-[28px] bg-carte p-6 sm:p-8">
        <h2 className="text-xl font-extrabold">Désinstaller</h2>
        <p className="mt-2 text-encre-douce">La même commande, avec une option. Vos modèles et vos réglages restent.</p>
        <div className="mt-4">
          <Commande texte={commandeDesinstallation} />
        </div>
      </section>

      <p className="mt-10 text-center text-encre-douce">
        Vous préférez compiler ?{" "}
        <a href={telechargements.sources} className="font-bold text-encre underline underline-offset-2">
          Les instructions sont dans le README
        </a>
        . Un souci ?{" "}
        <a href={`${depot}/issues`} className="font-bold text-encre underline underline-offset-2">
          Ouvrez une issue
        </a>
        .
      </p>
    </div>
  );
}
