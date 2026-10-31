"use client";

import { useState, useSyncExternalStore } from "react";

declare global {
  interface Window {
    moka?: { optOut?: () => void; optIn?: () => void };
  }
}

// Moka retient l'opposition dans le stockage local, sous cette clé.
const cle = "moka_optout";

function opposeDansLeNavigateur(): boolean {
  try {
    return localStorage.getItem(cle) !== null;
  } catch {
    return false;
  }
}

// Le stockage local ne prévient pas d'un changement dans le même onglet :
// le choix fait ici s'affiche tout de suite, par l'état local.
const aucunAbonnement = () => () => {};

/** Le lien pour ne plus être mesuré par Moka, et pour revenir sur ce choix. */
export function Opposition({ libelles }: { libelles: { bouton: string; desactive: string; reprendre: string } }) {
  const enregistre = useSyncExternalStore(aucunAbonnement, opposeDansLeNavigateur, () => false);
  const [choix, setChoix] = useState<boolean | null>(null);
  const oppose = choix ?? enregistre;

  function changer(opposer: boolean) {
    // Avant le chargement du script, la file d'attente de `Racine` garde l'appel.
    if (opposer) window.moka?.optOut?.();
    else window.moka?.optIn?.();
    setChoix(opposer);
  }

  const bouton = "rounded-full bg-carte-2 px-5 py-2.5 font-bold text-encre transition hover:bg-filet";
  if (oppose) {
    return (
      <p className="flex flex-wrap items-center gap-x-4 gap-y-2" aria-live="polite">
        <span className="font-bold">{libelles.desactive}</span>
        <button type="button" onClick={() => changer(false)} className={bouton}>
          {libelles.reprendre}
        </button>
      </p>
    );
  }
  return (
    <p aria-live="polite">
      <button type="button" onClick={() => changer(true)} className={bouton}>
        {libelles.bouton}
      </button>
    </p>
  );
}
