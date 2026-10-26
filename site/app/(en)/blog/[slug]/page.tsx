import type { Metadata } from "next";
import { notFound } from "next/navigation";
import { articles, cheminsArticle, slug, trouverArticle } from "../../../_blog";
import { metadonnees } from "../../../_i18n";
import { Article } from "../../../_pages/Article";

export function generateStaticParams() {
  return articles.map((a) => ({ slug: slug(a, "en") }));
}

export async function generateMetadata({ params }: PageProps<"/blog/[slug]">): Promise<Metadata> {
  const article = trouverArticle("en", (await params).slug);
  if (!article) return {};
  return metadonnees("en", cheminsArticle(article), {
    titre: article.en.titre,
    description: article.en.description,
    article: { publie: article.publie, modifie: article.modifie },
  });
}

export default async function Page({ params }: PageProps<"/blog/[slug]">) {
  const article = trouverArticle("en", (await params).slug);
  if (!article) notFound();
  return <Article article={article} lang="en" />;
}
