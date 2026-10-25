"use client";

import { useSyncExternalStore } from "react";

// Le confort de lecture du visiteur : police, taille, espacement, fond,
// animations. Rangé dans le navigateur, posé en attributs sur <html> (le
// CSS fait le reste), et relu avant la première image par le script de
// `Racine` : pas d'éclair de l'ancienne police au chargement.

export type Reglages = {
  police: "atkinson" | "lexend";
  taille: "normale" | "grande" | "tresgrande";
  espacement: "normal" | "large";
  fond: "creme" | "gris" | "sombre";
  animations: "oui" | "non";
};

export const DEFAUT: Reglages = {
  police: "atkinson",
  taille: "normale",
  espacement: "normal",
  fond: "creme",
  animations: "oui",
};

/** Tout ce qui aide, d'un coup : le « mode dys ». La police n'en fait pas
 *  partie : aucune police « pour dyslexiques » n'a prouvé qu'elle aidait ;
 *  la taille, l'espacement et le fond, si. */
export const MODE_DYS: Reglages = {
  police: "atkinson",
  taille: "grande",
  espacement: "large",
  fond: "creme",
  animations: "non",
};

export const CLE = "dysflow-confort";
const EVENEMENT = "dysflow-confort";

export function lire(): Reglages {
  try {
    const brut = window.localStorage.getItem(CLE);
    const r: Reglages = brut ? { ...DEFAUT, ...JSON.parse(brut) } : { ...DEFAUT };
    // OpenDyslexic (« dys ») a été retirée le 25 oct. 2026, et le fond
    // « normal » est devenu « gris » quand la crème est passée par défaut :
    // ce qui n'existe plus revient au défaut.
    if (r.police !== "lexend") r.police = "atkinson";
    if (r.fond !== "gris" && r.fond !== "sombre") r.fond = "creme";
    return r;
  } catch {
    return DEFAUT;
  }
}

export function appliquer(r: Reglages) {
  const html = document.documentElement;
  for (const [cle, valeur] of Object.entries(r)) html.dataset[cle] = valeur;
  try {
    window.localStorage.setItem(CLE, JSON.stringify(r));
  } catch {
    // Navigation privée : le réglage vaut pour la page, sans plus.
  }
  window.dispatchEvent(new Event(EVENEMENT));
}

function abonner(rappel: () => void) {
  const requete = window.matchMedia("(prefers-reduced-motion: reduce)");
  window.addEventListener(EVENEMENT, rappel);
  requete.addEventListener("change", rappel);
  return () => {
    window.removeEventListener(EVENEMENT, rappel);
    requete.removeEventListener("change", rappel);
  };
}

/** Les réglages en cours, relus à chaque changement. */
export function useReglages(): Reglages {
  const brut = useSyncExternalStore(
    abonner,
    () => JSON.stringify(lire()),
    () => JSON.stringify(DEFAUT),
  );
  return JSON.parse(brut) as Reglages;
}

/** Vrai quand rien ne doit bouger : demandé au système, ou dans le panneau. */
export function useCalme(): boolean {
  return useSyncExternalStore(
    abonner,
    () => window.matchMedia("(prefers-reduced-motion: reduce)").matches || lire().animations === "non",
    () => false,
  );
}
