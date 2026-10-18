import type { Metadata } from "next";
import { Bouton, IconeTelecharger, Pastille, TitreSection } from "../_components/ui";
import { dictee, lecture, type Modele } from "../_data/modeles";

export const metadata: Metadata = {
  title: "Les modèles",
  description:
    "Toutes les voix et tous les modèles de dictée de sillon : Kokoro, Qwen3-TTS, MagpieTTS, Parakeet, Phonon-2, Whisper, Canary, Voxtral. Tailles, langues, vitesse, licences.",
};

function Fiche({ m }: { m: Modele }) {
  const lignes = [
    ["Taille", m.taille],
    ["Précision", m.precision],
    ["Langues", m.langues],
    ["Calcul", m.calcul],
    ["Rythme", m.rythme],
    ...(m.voix ? [["Voix", m.voix]] : []),
    ["Licence", m.licence],
  ];
  return (
    <article className={`flex flex-col rounded-[22px] bg-carte p-6 ${m.recommande ? "ring-2 ring-citron" : ""}`}>
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-sm font-bold text-encre-pale">{m.famille}</span>
        {m.recommande && <Pastille className="bg-citron text-citron-encre">Recommandé</Pastille>}
        {m.lourd && <Pastille className="bg-carte-2 text-encre-douce">Lourd</Pastille>}
        <Pastille className="ml-auto bg-carte-2 text-encre-douce">{m.plateforme}</Pastille>
      </div>
      <h3 className="mt-3 text-xl font-extrabold">{m.nom}</h3>
      <p className="mt-2 text-[15px] leading-relaxed text-encre-douce">{m.description}</p>
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
          Le modèle sur Hugging Face
        </a>
      )}
    </article>
  );
}

function Tableau({ modeles }: { modeles: Modele[] }) {
  return (
    <div className="mt-8 overflow-x-auto rounded-[22px] bg-carte">
      <table className="w-full min-w-[640px] border-collapse text-left text-[15px]">
        <thead className="text-sm text-encre-pale">
          <tr>
            <th className="p-4 font-bold">Modèle</th>
            <th className="p-4 font-bold">Taille</th>
            <th className="p-4 font-bold">Langues</th>
            <th className="p-4 font-bold">Rythme</th>
          </tr>
        </thead>
        <tbody>
          {modeles.map((m) => (
            <tr key={m.id} className="border-t border-filet">
              <td className="p-4 font-extrabold">{m.nom}</td>
              <td className="p-4 tabular-nums">{m.taille}</td>
              <td className="p-4">{m.langues}</td>
              <td className="p-4 text-encre-douce">{m.rythme}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export default function Modeles() {
  return (
    <div className="mx-auto max-w-6xl px-4 pt-16 sm:px-6">
      <TitreSection
        surtitre="Modèles"
        titre="Choisissez votre voix, et votre dictée"
        texte="Tous les modèles tournent sur votre machine. Ils se téléchargent, se choisissent et se suppriment dans les réglages de l'app : vous gardez la main sur la place disque. Les vitesses sont mesurées, pas promises."
      />

      <section id="lecture" className="scroll-mt-20 pt-20">
        <h2 className="text-3xl font-extrabold tracking-tight">Lecture à voix haute</h2>
        <p className="mt-2 max-w-2xl text-lg text-encre-douce">
          Une seule voix pour toutes les langues. Kokoro s&apos;installe tout seul au premier lancement ;
          la voix du système lit en attendant.
        </p>
        <Tableau modeles={lecture} />
        <div className="mt-6 grid gap-3 md:grid-cols-2 lg:grid-cols-3">
          {lecture.map((m) => (
            <Fiche key={m.id} m={m} />
          ))}
        </div>
      </section>

      <section id="dictee" className="scroll-mt-20 pt-24">
        <h2 className="text-3xl font-extrabold tracking-tight">Dictée</h2>
        <p className="mt-2 max-w-2xl text-lg text-encre-douce">
          Dans l&apos;app macOS : un raccourci ouvre la dictée, et le texte s&apos;écrit dans l&apos;app au
          premier plan. Aucun modèle n&apos;est imposé ; un banc d&apos;essai au micro mesure la justesse
          avant de choisir.
        </p>
        <Tableau modeles={dictee} />
        <div className="mt-6 grid gap-3 md:grid-cols-2 lg:grid-cols-3">
          {dictee.map((m) => (
            <Fiche key={m.id} m={m} />
          ))}
        </div>
      </section>

      <section className="pt-24">
        <div className="rounded-[22px] bg-carte p-8">
          <h2 className="text-2xl font-extrabold">Reconnaissance de texte</h2>
          <p className="mt-2 max-w-3xl leading-relaxed text-encre-douce">
            Pour les images, les captures et les PDF photocopiés, sillon embarque Tesseract avec
            le français et l&apos;anglais : rien à installer. Les pages sont reconnues en parallèle et le
            résultat est mémorisé, si bien que la deuxième ouverture est immédiate.
          </p>
        </div>
        <p className="mt-6 text-sm text-encre-pale">
          Mesures faites sur un Mac M5. « Occupé x % du temps à 1× » : la part du temps de lecture
          passée à calculer la voix ; au-delà de 100 %, la lecture saccade.
        </p>
        <div className="mt-10 text-center">
          <Bouton href="/telecharger">
            <IconeTelecharger />
            Télécharger sillon
          </Bouton>
        </div>
      </section>
    </div>
  );
}
