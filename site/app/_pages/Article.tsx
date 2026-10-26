import Link from "next/link";
import { autresArticles, cheminsArticle, dateLongue, minutesDeLecture, type Article as A } from "../_blog";
import { Texte } from "../_blog/Texte";
import { JsonLd } from "../_components/JsonLd";
import { Bouton, Cadre, IconeTelecharger } from "../_components/ui";
import { absolu, filAriane, imageAbsolue, organisation } from "../_data/schema";
import { chemin, dico, type Lang } from "../_i18n";
import { CarteArticle } from "./Blog";

// Un article : une colonne étroite, un grand texte aéré. La page s'adresse à
// des lecteurs pour qui chaque ligne coûte.

export function Article({ article, lang }: { article: A; lang: Lang }) {
  const d = dico(lang).blog;
  const t = article[lang];
  const chemins = cheminsArticle(article);
  const donnees = {
    "@context": "https://schema.org",
    "@graph": [
      {
        "@type": "BlogPosting",
        "@id": absolu(`${chemins[lang]}#article`),
        headline: t.titre,
        description: t.description,
        image: imageAbsolue(chemins[lang]),
        datePublished: article.publie,
        dateModified: article.modifie ?? article.publie,
        inLanguage: lang,
        url: absolu(chemins[lang]),
        mainEntityOfPage: absolu(chemins[lang]),
        author: { "@id": organisation["@id"] },
        publisher: { "@id": organisation["@id"] },
        isPartOf: { "@id": absolu(`${chemin(lang, "blog")}#blog`) },
        wordCount: t.corps.split(/\s+/).filter(Boolean).length,
      },
      organisation,
      filAriane([
        { nom: d.accueil, chemin: chemin(lang, "accueil") },
        { nom: d.surtitre, chemin: chemin(lang, "blog") },
        { nom: t.titre, chemin: chemins[lang] },
      ]),
    ],
  };

  return (
    <Cadre lang={lang} page={chemins}>
      <JsonLd donnees={donnees} />
      <article className="mx-auto max-w-2xl px-4 pt-12 sm:px-6">
        <nav aria-label={d.surtitre} className="text-[15px] text-encre-pale">
          <ol className="flex flex-wrap items-center gap-2">
            <li>
              <Link href={chemin(lang, "accueil")} className="hover:text-encre">{d.accueil}</Link>
            </li>
            <li aria-hidden="true">›</li>
            <li>
              <Link href={chemin(lang, "blog")} className="hover:text-encre">{d.surtitre}</Link>
            </li>
          </ol>
        </nav>

        <header className="mt-6">
          <h1 className="text-balance break-words text-4xl font-extrabold leading-[1.1] tracking-tight sm:text-5xl">{t.titre}</h1>
          <p className="mt-5 text-[15px] text-encre-pale">
            {d.publie} <time dateTime={article.publie}>{dateLongue(article.publie, lang)}</time> ·{" "}
            {minutesDeLecture(article, lang)} {d.minutes}
          </p>
          <p className="mt-6 text-pretty text-2xl leading-relaxed text-encre-douce">{t.chapo}</p>
        </header>

        <div className="text-xl leading-relaxed">
          <Texte corps={t.corps} />
        </div>

        <aside className="mt-16 rounded-[28px] bg-citron px-6 py-10 text-center text-citron-encre sm:px-10">
          <p className="text-balance text-3xl font-extrabold tracking-tight">{d.final.titre}</p>
          <p className="mx-auto mt-3 max-w-md text-lg opacity-80">{d.final.texte}</p>
          <div className="mt-6 flex justify-center">
            <Link
              href={chemin(lang, "telecharger")}
              className="inline-flex items-center gap-2 rounded-full bg-[#191919] px-6 py-3.5 font-bold text-white hover:opacity-90"
            >
              <IconeTelecharger />
              {dico(lang).accueil.telecharger}
            </Link>
          </div>
        </aside>
      </article>

      <section className="mx-auto max-w-4xl px-4 pt-20 sm:px-6">
        <h2 className="text-center text-3xl font-extrabold tracking-tight">{d.autres}</h2>
        <div className="mt-8 grid gap-4 md:grid-cols-2">
          {autresArticles(article).map((a) => (
            <CarteArticle key={a.id} article={a} lang={lang} niveau="h3" />
          ))}
        </div>
        <div className="mt-8 text-center">
          <Bouton href={chemin(lang, "blog")} variante="doux">
            {d.retour}
          </Bouton>
        </div>
      </section>
    </Cadre>
  );
}
