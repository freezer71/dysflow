import { absolu } from "../_data/schema";
import { chemin, dico, nomDuSite, type Lang } from "../_i18n";
import { articles, cheminsArticle } from ".";

const echapper = (texte: string) =>
  texte.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

/** Le flux RSS 2.0 du blog dans une langue. */
export function flux(lang: Lang): Response {
  const d = dico(lang).blog;
  const items = articles
    .map((a) => {
      const url = absolu(cheminsArticle(a)[lang]);
      return `    <item>
      <title>${echapper(a[lang].titre)}</title>
      <link>${url}</link>
      <guid isPermaLink="true">${url}</guid>
      <description>${echapper(a[lang].description)}</description>
      <pubDate>${new Date(`${a.publie}T08:00:00Z`).toUTCString()}</pubDate>
    </item>`;
    })
    .join("\n");
  const xml = `<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:atom="http://www.w3.org/2005/Atom">
  <channel>
    <title>${echapper(`${d.surtitre} · ${nomDuSite}`)}</title>
    <link>${absolu(chemin(lang, "blog"))}</link>
    <description>${echapper(d.meta.description)}</description>
    <language>${lang}</language>
    <atom:link href="${absolu(`${chemin(lang, "blog")}/feed.xml`)}" rel="self" type="application/rss+xml"/>
${items}
  </channel>
</rss>
`;
  return new Response(xml, { headers: { "Content-Type": "application/rss+xml; charset=utf-8" } });
}
