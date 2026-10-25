"use client";

import { useEffect, useId, useRef, useState } from "react";
import { appliquer, DEFAUT, MODE_DYS, useReglages, type Reglages } from "./reglages";

// Le panneau « Confort de lecture », ouvert depuis l'en-tête. Chaque réglage
// est un groupe de boutons : on voit tout de suite ce qui est choisi, sans
// menu déroulant à déchiffrer.

type Choix<K extends keyof Reglages> = { titre: string } & Record<Reglages[K], string>;

export type LibellesConfort = {
  bouton: string;
  titre: string;
  modeDys: string;
  reinitialiser: string;
  fermer: string;
  police: Choix<"police">;
  taille: Choix<"taille">;
  espacement: Choix<"espacement">;
  fond: Choix<"fond">;
  animations: Choix<"animations">;
};

const VALEURS: { [K in keyof Reglages]: Reglages[K][] } = {
  police: ["atkinson", "lexend"],
  taille: ["normale", "grande", "tresgrande"],
  espacement: ["normal", "large"],
  fond: ["creme", "gris", "sombre"],
  animations: ["oui", "non"],
};

export function Confort({ libelles }: { libelles: LibellesConfort }) {
  const [ouvert, setOuvert] = useState(false);
  const reglages = useReglages();
  const bouton = useRef<HTMLButtonElement>(null);
  const panneau = useRef<HTMLDivElement>(null);
  const id = useId();

  useEffect(() => {
    if (!ouvert) return;
    panneau.current?.focus();
    const touche = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setOuvert(false);
        bouton.current?.focus();
      }
    };
    const clic = (e: MouseEvent) => {
      const cible = e.target as Node;
      if (!panneau.current?.contains(cible) && !bouton.current?.contains(cible)) setOuvert(false);
    };
    document.addEventListener("keydown", touche);
    document.addEventListener("mousedown", clic);
    return () => {
      document.removeEventListener("keydown", touche);
      document.removeEventListener("mousedown", clic);
    };
  }, [ouvert]);

  const changer = <K extends keyof Reglages>(cle: K, valeur: Reglages[K]) => appliquer({ ...reglages, [cle]: valeur });
  const modeDys = (Object.keys(MODE_DYS) as (keyof Reglages)[]).every((k) => reglages[k] === MODE_DYS[k]);

  return (
    <div className="relative">
      <button
        ref={bouton}
        type="button"
        aria-expanded={ouvert}
        aria-controls={id}
        onClick={() => setOuvert((o) => !o)}
        className={`flex items-center gap-1.5 rounded-full px-3 py-1.5 font-bold ${ouvert ? "bg-encre text-sur-encre" : "bg-carte-2 text-encre hover:bg-filet"}`}
      >
        <span aria-hidden="true" className="text-base leading-none">Aa</span>
        <span className="sr-only sm:not-sr-only">{libelles.bouton}</span>
      </button>

      {ouvert && (
        <div
          ref={panneau}
          id={id}
          role="dialog"
          aria-label={libelles.titre}
          tabIndex={-1}
          className="absolute right-0 top-12 z-30 w-[min(23rem,calc(100vw-2rem))] rounded-[24px] bg-carte p-5 text-encre shadow-[0_30px_80px_-20px_rgba(0,0,0,0.35)] outline-none"
        >
          <div className="flex items-center justify-between gap-3">
            <h2 className="text-xl font-extrabold">{libelles.titre}</h2>
            <button
              type="button"
              onClick={() => {
                setOuvert(false);
                bouton.current?.focus();
              }}
              className="grid size-9 place-items-center rounded-full bg-carte-2 text-lg"
              aria-label={libelles.fermer}
            >
              ×
            </button>
          </div>

          <button
            type="button"
            aria-pressed={modeDys}
            onClick={() => appliquer(modeDys ? DEFAUT : MODE_DYS)}
            className={`mt-4 w-full rounded-2xl px-4 py-3 text-left text-lg font-extrabold ${modeDys ? "bg-citron text-citron-encre" : "bg-encre text-sur-encre"}`}
          >
            {modeDys ? "✓ " : ""}
            {libelles.modeDys}
          </button>

          <div className="mt-5 space-y-4">
            {(Object.keys(VALEURS) as (keyof Reglages)[]).map((cle) => {
              const groupe = libelles[cle] as { titre: string } & Record<string, string>;
              return (
                <div key={cle} role="group" aria-label={groupe.titre}>
                  <p className="text-sm font-bold text-encre-douce">{groupe.titre}</p>
                  <div className="mt-1.5 flex flex-wrap gap-1.5">
                    {VALEURS[cle].map((v) => {
                      const actif = reglages[cle] === v;
                      return (
                        <button
                          key={v}
                          type="button"
                          aria-pressed={actif}
                          onClick={() => changer(cle, v as never)}
                          className={`rounded-full px-3.5 py-1.5 text-[15px] font-bold ${
                            actif ? "bg-encre text-sur-encre" : "bg-carte-2 text-encre hover:bg-filet"
                          }`}
                          style={cle === "police" ? { fontFamily: v === "lexend" ? "var(--font-lexend)" : "var(--font-atkinson)" } : undefined}
                        >
                          {groupe[v]}
                        </button>
                      );
                    })}
                  </div>
                </div>
              );
            })}
          </div>

          <button type="button" onClick={() => appliquer(DEFAUT)} className="mt-5 text-[15px] font-bold text-encre-douce underline underline-offset-2">
            {libelles.reinitialiser}
          </button>
        </div>
      )}
    </div>
  );
}
