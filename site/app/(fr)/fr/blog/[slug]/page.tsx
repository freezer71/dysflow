import type { Metadata } from "next";
import { notFound } from "next/navigation";
import { articles, cheminsArticle, slug, trouverArticle } from "../../../../_blog";
import { metadonnees } from "../../../../_i18n";
import { Article } from "../../../../_pages/Article";

export function generateStaticParams() {
  return articles.map((a) => ({ slug: slug(a, "fr") }));
}

export async function generateMetadata({ params }: PageProps<"/fr/blog/[slug]">): Promise<Metadata> {
  const article = trouverArticle("fr", (await params).slug);
  if (!article) return {};
  return metadonnees("fr", cheminsArticle(article), {
    titre: article.fr.titre,
    description: article.fr.description,
    article: { publie: article.publie, modifie: article.modifie },
  });
}

export default async function Page({ params }: PageProps<"/fr/blog/[slug]">) {
  const article = trouverArticle("fr", (await params).slug);
  if (!article) notFound();
  return <Article article={article} lang="fr" />;
}
