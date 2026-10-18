import { cacheLife } from "next/cache";
import { depot } from "../_data/liens";

// `curl -fsSL https://sillage.koumalabs.org/install.sh | sh` : le script de
// la dernière publication (joint par release.yml), pas celui de main — il
// va avec les archives publiées. Prérendu, puis relu sur GitHub au plus une
// fois par heure : aucune fonction ne tourne à chaque installation.
const source = `${depot}/releases/latest/download/install.sh`;

async function script(): Promise<string | null> {
  "use cache";
  cacheLife("hours");
  try {
    const reponse = await fetch(source);
    if (!reponse.ok) return null;
    const texte = await reponse.text();
    // Une page d'erreur n'est pas un script : on ne la sert pas.
    return texte.startsWith("#!/bin/sh") ? texte : null;
  } catch {
    return null;
  }
}

export async function GET() {
  const texte = await script();
  if (!texte) {
    // GitHub injoignable : on y renvoie (curl -L suit la redirection).
    return Response.redirect(source, 302);
  }
  return new Response(texte, {
    headers: {
      "Content-Type": "text/x-shellscript; charset=utf-8",
      "Content-Disposition": 'inline; filename="install.sh"',
    },
  });
}
