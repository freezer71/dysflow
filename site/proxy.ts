import { NextResponse, type NextRequest } from "next/server";

// La langue par défaut est celle du navigateur. Les pages anglaises sont à la
// racine : un visiteur qui préfère le français y arrive, et part vers la même
// page en français. Une redirection, pas une réécriture : l'adresse affichée
// reste celle de la page servie, et le routeur côté client n'est pas trompé.
//
// Dès qu'on choisit une langue avec le sélecteur, le cookie `langue` est posé
// et le proxy ne tourne plus du tout (`missing` dans le `matcher`).

const enFrancais: Record<string, string> = {
  "/": "/fr",
  "/models": "/fr/modeles",
  "/download": "/fr/telecharger",
};

/** Vrai quand le navigateur met le français avant l'anglais. */
function prefereLeFrancais(entete: string | null): boolean {
  if (!entete) return false;
  const langues = entete
    .split(",")
    .map((morceau) => {
      const [code, ...params] = morceau.trim().toLowerCase().split(";");
      const q = params.find((p) => p.trim().startsWith("q="));
      return { code: code.split("-")[0], q: q ? Number(q.trim().slice(2)) : 1 };
    })
    .filter((l) => l.code === "fr" || l.code === "en")
    .sort((a, b) => b.q - a.q);
  return langues[0]?.code === "fr";
}

export function proxy(requete: NextRequest) {
  const cible = enFrancais[requete.nextUrl.pathname];
  if (!cible || !prefereLeFrancais(requete.headers.get("accept-language"))) return;
  const url = requete.nextUrl.clone();
  url.pathname = cible;
  return NextResponse.redirect(url, 307);
}

export const config = {
  matcher: [
    { source: "/", missing: [{ type: "cookie", key: "langue" }] },
    { source: "/models", missing: [{ type: "cookie", key: "langue" }] },
    { source: "/download", missing: [{ type: "cookie", key: "langue" }] },
  ],
};
