import Link from "next/link";
import { Commande } from "./_components/Commande";
import { DemoLecteur } from "./_components/DemoLecteur";
import { Bouton, IconeTelecharger, Pastille, TitreSection } from "./_components/ui";
import { commandeInstallation, depot } from "./_data/liens";
import { dictee, lecture } from "./_data/modeles";

const chiffres = [
  { valeur: "0 €", legende: "pour toujours, sans abonnement" },
  { valeur: "0 octet", legende: "de vos textes envoyé en ligne" },
  { valeur: "15×", legende: "plus vite que la parole sur Mac" },
  { valeur: `${lecture.length + dictee.length}`, legende: "modèles de voix et de dictée" },
];

const avantages = [
  {
    titre: "Vos textes restent chez vous",
    texte:
      "Les voix et la dictée tournent sur votre machine. Pas de compte, pas de serveur, pas de quota : un contrat, un dossier médical ou un brouillon ne quittent jamais votre ordinateur.",
    icone: "M12 3 5 6v6c0 4.2 3 7.6 7 9 4-1.4 7-4.8 7-9V6l-7-3Zm-3 9 2 2 4-4",
  },
  {
    titre: "Chaque mot suivi des yeux",
    texte:
      "Le mot dit porte une pastille, sa ligne une bande discrète : le mot dit où en est la voix, la ligne dit où revenir quand on relève les yeux. Pensé pour la dyslexie et la lecture longue.",
    icone: "M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12Zm10 3a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z",
  },
  {
    titre: "Il lit tout ce que vous lui donnez",
    texte:
      "Markdown, texte, PDF, PDF photocopié, capture d'écran, photo de page, presse-papiers, sélection dans n'importe quelle app. Les pages scannées sont reconnues en parallèle et mémorisées.",
    icone: "M7 3h7l5 5v13H7V3Zm7 0v5h5M10 13h6M10 17h6",
  },
  {
    titre: "Il ne lit pas le formatage",
    texte:
      "Jamais de « dièse titre » ni de « tiret tiret ». Un bloc de code est annoncé, pas récité. Un tableau se lit colonne par colonne : « Route : /users. Méthode : GET. »",
    icone: "M8 7 3 12l5 5M16 7l5 5-5 5M14 4l-4 16",
  },
  {
    titre: "Plus vite, sans voix de canard",
    texte:
      "La vitesse agit sur le son pendant qu'il sonne : la voix accélère sans monter dans les aigus, au milieu de la phrase, sans coupure. Avance et recul sont immédiats.",
    icone: "M13 2 4 14h7l-1 8 9-12h-7l1-8Z",
  },
  {
    titre: "Il reprend là où vous étiez",
    texte:
      "La position se retient toute seule — la phrase elle-même, pas un numéro. Elle se retrouve même si le document a été retouché entre-temps.",
    icone: "M6 3h12v18l-6-4-6 4V3Z",
  },
  {
    titre: "La dictée, en plus",
    texte:
      "Un raccourci ouvre la dictée, et le texte s'écrit dans l'app au premier plan. Parakeet, Whisper, Voxtral, Canary : choisissez le vôtre, tout reste local.",
    icone: "M12 3a3 3 0 0 0-3 3v6a3 3 0 0 0 6 0V6a3 3 0 0 0-3-3Zm-7 9a7 7 0 0 0 14 0M12 19v3",
  },
  {
    titre: "Libre, pour de vrai",
    texte:
      "Code ouvert sous licence MIT, modèles sous licences ouvertes. Vous pouvez le lire, le modifier, le redistribuer. Personne ne pourra vous le retirer ni le faire payer.",
    icone: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18Zm3-12a4 4 0 1 0 0 6",
  },
];

const publics = [
  {
    titre: "Dyslexie, TDAH, fatigue visuelle",
    texte: "Écouter et suivre des yeux à la fois : la lecture bimodale, avec la police de votre choix (OpenDyslexic, Atkinson Hyperlegible, Literata).",
  },
  {
    titre: "Étudiants et chercheurs",
    texte: "Les PDF sont reconstruits : titres au sommaire, mots coupés recollés, en-têtes et pieds de page retirés. On écoute l'article, pas la mise en page.",
  },
  {
    titre: "Développeurs",
    texte: "Documentation, README, réponses d'IA pipées dans le terminal : `llm \"…\" | sillon` démarre la lecture avant la fin du flux.",
  },
];

type Case = "oui" | "non" | "partiel" | string;

const concurrents = ["sillon", "Speechify", "ElevenReader", "NaturalReader", "Contenu énoncé (macOS)"];

const comparaison: { critere: string; cases: Case[] }[] = [
  { critere: "Gratuit, sans limite d'usage", cases: ["oui", "Gratuit limité, puis abonnement", "Gratuit limité, puis abonnement", "Gratuit limité, puis abonnement", "oui"] },
  { critere: "Code source ouvert", cases: ["oui", "non", "non", "non", "non"] },
  { critere: "Vos textes restent sur votre machine", cases: ["oui", "non", "non", "non", "oui"] },
  { critere: "Voix neurales naturelles", cases: ["oui", "oui", "oui", "oui", "partiel"] },
  { critere: "Mot lu surligné", cases: ["oui", "oui", "oui", "oui", "oui"] },
  { critere: "Ne lit ni balisage, ni code, ni bordures de tableau", cases: ["oui", "Non annoncé", "Non annoncé", "Non annoncé", "non"] },
  { critere: "Reconnaissance des pages scannées", cases: ["oui", "oui", "oui", "oui", "non"] },
  { critere: "Dictée intégrée, hors ligne", cases: ["oui", "Non annoncé", "non", "non", "partiel"] },
  { critere: "Version Linux", cases: ["Terminal", "Navigateur", "Navigateur", "Navigateur", "non"] },
];

const faq = [
  {
    q: "C'est vraiment gratuit ?",
    r: "Oui, et sans contrepartie : pas de compte, pas de publicité, pas de version « Pro ». sillon est un logiciel libre sous licence MIT ; les modèles se téléchargent depuis leurs dépôts publics.",
  },
  {
    q: "Mes documents sont-ils envoyés quelque part ?",
    r: "Non. La lecture, la reconnaissance de texte et la dictée se calculent sur votre machine. Le seul accès au réseau est le téléchargement des modèles que vous choisissez.",
  },
  {
    q: "Quel Mac faut-il ?",
    r: "Un Mac à puce Apple (M1 et suivants). La dictée demande macOS 26. Kokoro, la voix par défaut, occupe 342 Mo et moins de 6 % du temps de calcul pendant la lecture.",
  },
  {
    q: "Et sous Linux ?",
    r: "Le lecteur au terminal tourne sous Linux x86_64 et ARM64 (glibc 2.39 ou plus), avec la même voix Kokoro et la reconnaissance de texte intégrée. La même commande d'installation que sur Mac l'installe.",
  },
  {
    q: "Quelles langues ?",
    r: "La lecture est faite pour le français et l'anglais, avec une seule voix pour les deux. La dictée va jusqu'à 99 langues selon le modèle choisi.",
  },
  {
    q: "Et Windows ?",
    r: "Pas pour l'instant. Le moteur est en Rust et pourrait y être porté : les contributions sont bienvenues sur GitHub.",
  },
];

function Valeur({ v, sillon }: { v: Case; sillon?: boolean }) {
  if (v === "oui")
    return (
      <span className={`inline-grid size-7 place-items-center rounded-full ${sillon ? "bg-citron text-citron-encre" : "bg-carte-2 text-encre"}`} aria-label="Oui">
        <svg viewBox="0 0 24 24" className="size-4" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round"><path d="m5 12 5 5 9-10" /></svg>
      </span>
    );
  if (v === "non")
    return <span className="text-xl text-encre-pale" aria-label="Non">—</span>;
  if (v === "partiel")
    return <span className="text-sm text-encre-douce">En partie</span>;
  return <span className="text-sm leading-snug text-encre-douce">{v}</span>;
}

export default function Accueil() {
  const vitrine = [lecture[0], dictee[0], dictee[1]];
  return (
    <>
      {/* Accroche */}
      <section className="mx-auto grid max-w-6xl items-center gap-12 px-4 pb-20 pt-14 sm:px-6 lg:grid-cols-[1.05fr_1fr] lg:pt-24">
        <div className="min-w-0">
          <div className="flex flex-wrap gap-2">
            <Pastille className="bg-citron text-citron-encre">Gratuit</Pastille>
            <Pastille className="bg-carte-2">Open source</Pastille>
            <Pastille className="bg-carte-2">100 % local</Pastille>
          </div>
          <h1 className="mt-6 text-balance text-5xl font-extrabold leading-[1.02] tracking-tight sm:text-6xl lg:text-7xl">
            Écoutez vos documents. Suivez chaque mot.
          </h1>
          <p className="mt-6 max-w-xl text-pretty text-xl leading-relaxed text-encre-douce">
            sillon lit à voix haute vos PDF, vos notes et vos captures d&apos;écran, et surligne
            chaque mot au moment où il est dit. Des voix naturelles, calculées sur votre machine :
            rien ne part en ligne, et rien n&apos;est à payer.
          </p>
          <div className="mt-9 flex flex-wrap gap-3">
            <Bouton href="/telecharger">
              <IconeTelecharger />
              Télécharger sillon
            </Bouton>
            <Bouton href={depot} variante="doux">
              Voir le code sur GitHub
            </Bouton>
          </div>
          <div className="mt-8 max-w-xl">
            <p className="mb-2 text-[15px] text-encre-pale">Ou en une commande, sur Mac (puce Apple) et Linux :</p>
            <Commande texte={commandeInstallation} />
          </div>
        </div>
        <DemoLecteur />
      </section>

      {/* Chiffres */}
      <section className="mx-auto max-w-6xl px-4 sm:px-6">
        <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
          {chiffres.map((c) => (
            <div key={c.legende} className="rounded-[22px] bg-carte p-6">
              <p className="text-4xl font-extrabold tracking-tight">{c.valeur}</p>
              <p className="mt-1 text-[15px] text-encre-douce">{c.legende}</p>
            </div>
          ))}
        </div>
      </section>

      {/* Avantages */}
      <section id="avantages" className="mx-auto max-w-6xl scroll-mt-20 px-4 pt-28 sm:px-6">
        <TitreSection
          surtitre="Pourquoi sillon"
          titre="Un lecteur pensé pour être suivi des yeux"
          texte="La plupart des lecteurs vocaux se contentent de parler. sillon comprend ce qu'il lit, et vous montre où vous en êtes."
        />
        <div className="mt-14 grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
          {avantages.map((a) => (
            <article key={a.titre} className="rounded-[22px] bg-carte p-6">
              <span className="grid size-11 place-items-center rounded-2xl bg-citron text-citron-encre">
                <svg viewBox="0 0 24 24" className="size-5" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                  <path d={a.icone} />
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
          <p className="text-sm font-bold uppercase tracking-[0.14em] opacity-60">Pour qui</p>
          <h2 className="mt-3 max-w-2xl text-balance text-3xl font-extrabold tracking-tight sm:text-4xl">
            Pour ceux qui lisent beaucoup, et pour ceux à qui lire coûte.
          </h2>
          <div className="mt-10 grid gap-8 md:grid-cols-3">
            {publics.map((p) => (
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
          surtitre="Comparaison"
          titre="Tout ce que font les grands, sans l'abonnement ni le cloud"
          texte="Les lecteurs commerciaux envoient vos textes à leurs serveurs et font payer les belles voix. sillon fait le calcul chez vous."
        />
        <div className="mt-14 overflow-x-auto rounded-[22px] bg-carte">
          <table className="w-full min-w-[760px] border-collapse text-left">
            <thead>
              <tr>
                <th className="w-[28%] p-5 text-sm font-bold text-encre-pale">Critère</th>
                {concurrents.map((c, i) => (
                  <th key={c} className={`p-5 text-center text-[15px] font-extrabold ${i === 0 ? "bg-bande" : ""}`}>
                    {c}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {comparaison.map((ligne) => (
                <tr key={ligne.critere} className="border-t border-filet">
                  <td className="p-5 font-bold">{ligne.critere}</td>
                  {ligne.cases.map((v, i) => (
                    <td key={i} className={`p-5 text-center ${i === 0 ? "bg-bande" : ""}`}>
                      <Valeur v={v} sillon={i === 0} />
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <p className="mt-4 text-center text-sm text-encre-pale">
          D&apos;après ce que les éditeurs annoncent sur leurs sites, en octobre 2026. Une erreur ?{" "}
          <a href={`${depot}/issues`} className="underline underline-offset-2 hover:text-encre">Dites-le-nous</a>.
        </p>
      </section>

      {/* Modèles */}
      <section className="mx-auto max-w-6xl px-4 pt-28 sm:px-6">
        <TitreSection
          surtitre="Modèles"
          titre="Les meilleurs modèles ouverts, au choix"
          texte="Téléchargez, essayez, supprimez : vous gardez la main sur la qualité, la vitesse et la place disque."
        />
        <div className="mt-14 grid gap-3 md:grid-cols-3">
          {vitrine.map((m, i) => (
            <div key={m.id} className="rounded-[22px] bg-carte p-6">
              <p className="text-sm font-bold text-encre-pale">{i === 0 ? "Lecture" : "Dictée"}</p>
              <h3 className="mt-2 text-xl font-extrabold">{m.nom}</h3>
              <p className="mt-2 text-[15px] leading-relaxed text-encre-douce">{m.description}</p>
              <div className="mt-5 flex flex-wrap gap-2">
                <Pastille className="bg-carte-2">{m.taille}</Pastille>
                <Pastille className="bg-carte-2">{m.langues}</Pastille>
              </div>
            </div>
          ))}
        </div>
        <div className="mt-8 text-center">
          <Bouton href="/modeles" variante="doux">
            Voir les {lecture.length + dictee.length} modèles
          </Bouton>
        </div>
      </section>

      {/* Questions */}
      <section className="mx-auto max-w-3xl px-4 pt-28 sm:px-6">
        <TitreSection surtitre="Questions" titre="Ce qu'on nous demande" />
        <div className="mt-12 space-y-3">
          {faq.map((f) => (
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
            Votre prochain document, écoutez-le.
          </h2>
          <p className="mx-auto mt-4 max-w-xl text-lg opacity-75">
            Gratuit, libre, et installé en une commande.
          </p>
          <div className="mt-8 flex flex-wrap justify-center gap-3">
            <Link
              href="/telecharger"
              className="inline-flex items-center gap-2 rounded-full bg-[#191919] px-6 py-3.5 font-bold text-white hover:opacity-90"
            >
              <IconeTelecharger />
              Télécharger sillon
            </Link>
          </div>
        </div>
      </section>
    </>
  );
}
