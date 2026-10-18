import { cacheLife } from "next/cache";

// `curl -fsSL https://sillage.koumalabs.org/install.sh | sh` : le script tel
// qu'il est sur la branche main du dépôt. Relu sur GitHub au plus une fois
// par minute (cacheLife « minutes ») : un changement poussé sur main est
// servi dans la minute — plus les quelques minutes du cache de
// raw.githubusercontent.com —, sans qu'une fonction tourne à chaque
// installation. Le script, lui, va chercher la dernière publication.
const source = "https://raw.githubusercontent.com/freezer71/sillage/main/install.sh";

async function script(): Promise<string | null> {
  "use cache";
  cacheLife("minutes");
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
