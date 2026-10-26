import { articles, minutesDeLecture, slug, trouverArticle } from "../../../../_blog";
import { dico } from "../../../../_i18n";
import { carte } from "../../../../_og/carte";

export function generateStaticParams() {
  return articles.map((a) => ({ slug: slug(a, "en") }));
}

export async function GET(_requete: Request, { params }: RouteContext<"/blog/[slug]/og.png">) {
  const article = trouverArticle("en", (await params).slug);
  if (!article) return new Response(null, { status: 404 });
  const d = dico("en").blog;
  return carte({ surtitre: `${d.surtitre} · ${minutesDeLecture(article, "en")} ${d.minutes}`, titre: article.en.titre });
}
