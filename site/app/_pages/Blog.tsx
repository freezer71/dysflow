import Link from "next/link";
import { articles, cheminsArticle, dateLongue, minutesDeLecture, type Article } from "../_blog";
import { JsonLd } from "../_components/JsonLd";
import { Cadre, TitreSection } from "../_components/ui";
import { absolu, filAriane, organisation } from "../_data/schema";
import { chemin, dico, type Lang } from "../_i18n";

/** La carte d'un article : son titre est le lien, toute la carte est cliquable. */
export function CarteArticle({ article, lang, niveau = "h2" }: { article: Article; lang: Lang; niveau?: "h2" | "h3" }) {
  const d = dico(lang).blog;
  const t = article[lang];
  const Titre = niveau;
  return (
    <article className="relative flex flex-col rounded-[28px] bg-carte p-6 transition hover:ring-2 hover:ring-citron sm:p-8">
      <p className="text-[15px] text-encre-pale">
        <time dateTime={article.publie}>{dateLongue(article.publie, lang)}</time> · {minutesDeLecture(article, lang)} {d.minutes}
      </p>
      <Titre className="mt-3 text-balance text-2xl font-extrabold leading-tight tracking-tight">
        <Link href={cheminsArticle(article)[lang]} className="after:absolute after:inset-0 after:rounded-[28px]">
          {t.titre}
        </Link>
      </Titre>
      <p className="mt-3 text-pretty text-lg leading-relaxed text-encre-douce">{t.chapo}</p>
      <p className="mt-auto pt-5 font-bold" aria-hidden="true">
        {d.lire} →
      </p>
    </article>
  );
}

export function Blog({ lang }: { lang: Lang }) {
  const d = dico(lang).blog;
  const donnees = {
    "@context": "https://schema.org",
    "@graph": [
      {
        "@type": "Blog",
        "@id": absolu(`${chemin(lang, "blog")}#blog`),
        name: d.meta.titre,
        description: d.meta.description,
        url: absolu(chemin(lang, "blog")),
        inLanguage: lang,
        publisher: { "@id": organisation["@id"] },
        blogPost: articles.map((a) => ({
          "@type": "BlogPosting",
          headline: a[lang].titre,
          url: absolu(cheminsArticle(a)[lang]),
          datePublished: a.publie,
        })),
      },
      organisation,
      filAriane([
        { nom: d.accueil, chemin: chemin(lang, "accueil") },
        { nom: d.surtitre, chemin: chemin(lang, "blog") },
      ]),
    ],
  };

  return (
    <Cadre lang={lang} page="blog">
      <JsonLd donnees={donnees} />
      <div className="mx-auto max-w-6xl px-4 pt-16 sm:px-6">
        <TitreSection surtitre={d.surtitre} titre={d.titre} texte={d.texte} principal />
        <div className="mt-14 grid gap-4 md:grid-cols-2">
          {articles.map((a) => (
            <CarteArticle key={a.id} article={a} lang={lang} />
          ))}
        </div>
        <p className="mt-10 text-center text-encre-douce">
          <a href={`${chemin(lang, "blog")}/feed.xml`} className="font-bold underline underline-offset-4 hover:text-encre">
            {d.flux}
          </a>
        </p>
      </div>
    </Cadre>
  );
}
