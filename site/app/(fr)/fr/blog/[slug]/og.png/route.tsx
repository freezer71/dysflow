import { articles, minutesDeLecture, slug, trouverArticle } from "../../../../../_blog";
import { dico } from "../../../../../_i18n";
import { carte } from "../../../../../_og/carte";

export function generateStaticParams() {
  return articles.map((a) => ({ slug: slug(a, "fr") }));
}

export async function GET(_requete: Request, { params }: RouteContext<"/fr/blog/[slug]/og.png">) {
  const article = trouverArticle("fr", (await params).slug);
  if (!article) return new Response(null, { status: 404 });
  const d = dico("fr").blog;
  return carte({ surtitre: `${d.surtitre} · ${minutesDeLecture(article, "fr")} ${d.minutes}`, titre: article.fr.titre });
}
