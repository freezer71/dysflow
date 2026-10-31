"use client";

import { useState } from "react";

/** Une commande à copier d'un clic. */
export function Commande({
  texte,
  libelles,
  grande,
  objectif,
}: {
  texte: string;
  /** Les mots du bouton, dans la langue de la page. */
  libelles: { copier: string; copie: string };
  grande?: boolean;
  /** L'objectif Moka envoyé quand on copie la commande. */
  objectif?: string;
}) {
  const [copie, setCopie] = useState(false);

  async function copier() {
    try {
      await navigator.clipboard.writeText(texte);
      setCopie(true);
      window.setTimeout(() => setCopie(false), 1800);
    } catch {
      // Presse-papiers refusé : la commande reste sélectionnable à la main.
    }
  }

  return (
    <div className={`flex min-w-0 items-center gap-2 rounded-2xl bg-[#191919] text-white ring-1 ring-white/10 ${grande ? "p-2 pl-5" : "p-1.5 pl-4"}`}>
      <span className="select-none font-mono text-citron" aria-hidden="true">$</span>
      {/* Défilable, donc atteignable au clavier : les flèches font défiler. */}
      <code tabIndex={0} className={`min-w-0 flex-1 overflow-x-auto whitespace-nowrap rounded-md font-mono ${grande ? "py-2 text-[15px] sm:text-base" : "py-1.5 text-sm"}`}>
        {texte}
      </code>
      <button
        type="button"
        onClick={copier}
        data-moka-goal={objectif}
        className={`shrink-0 rounded-xl font-bold transition ${grande ? "px-4 py-2.5" : "px-3 py-2 text-sm"} ${
          copie ? "bg-citron text-citron-encre" : "bg-white/10 hover:bg-white/20"
        }`}
      >
        {copie ? libelles.copie : libelles.copier}
      </button>
    </div>
  );
}
