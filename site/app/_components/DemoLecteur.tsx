"use client";

import { useEffect, useState } from "react";

// Le lecteur, en miniature : la pastille citron suit le mot dit, la bande
// tient la ligne où revenir. Ce n'est qu'une image animée, sans son.
const lignes = [
  ["sillage", "lit", "à", "voix", "haute", "ce", "que"],
  ["vous", "lui", "donnez,", "et", "montre", "chaque"],
  ["mot", "au", "moment", "où", "il", "est", "dit."],
];

const debuts = lignes.map((_, i) => lignes.slice(0, i).reduce((n, l) => n + l.length, 0));
const total = lignes.reduce((n, l) => n + l.length, 0);

export function DemoLecteur() {
  const [rang, setRang] = useState(0);

  useEffect(() => {
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    const minuteur = window.setInterval(() => setRang((r) => (r + 1) % (total + 3)), 420);
    return () => window.clearInterval(minuteur);
  }, []);

  return (
    <div className="rounded-[28px] bg-carte p-3 shadow-[0_30px_80px_-30px_rgba(0,0,0,0.25)]">
      <div className="flex items-center gap-1.5 px-3 pb-3 pt-1">
        <span className="size-3 rounded-full bg-[#ff5f57]" />
        <span className="size-3 rounded-full bg-[#febc2e]" />
        <span className="size-3 rounded-full bg-[#28c840]" />
        <span className="ml-3 text-[13px] text-encre-pale">rapport-annuel.pdf</span>
      </div>
      <div className="rounded-[20px] bg-fond px-6 py-7 sm:px-8">
        <p className="text-sm font-bold text-encre-pale">Introduction</p>
        <div className="mt-3 space-y-1 text-xl leading-[1.9] sm:text-2xl">
          {lignes.map((mots, i) => {
            const debut = debuts[i];
            const ligneActive = rang >= debut && rang < debut + mots.length;
            return (
              <p
                key={i}
                className={`-mx-2 rounded-xl px-2 transition-colors duration-300 ${ligneActive ? "bg-bande" : ""}`}
              >
                {mots.map((mot, j) => {
                  const actif = debut + j === rang;
                  const lu = debut + j < rang;
                  return (
                    <span key={j}>
                      <span
                        className={`rounded-lg px-1 transition-all duration-200 ${
                          actif ? "bg-citron text-citron-encre" : lu ? "text-encre" : "text-encre-pale"
                        }`}
                      >
                        {mot}
                      </span>{" "}
                    </span>
                  );
                })}
              </p>
            );
          })}
        </div>
      </div>
      <div className="flex items-center gap-4 px-4 pb-2 pt-4">
        <span className="grid size-10 place-items-center rounded-full bg-encre text-sur-encre" aria-hidden="true">
          <svg viewBox="0 0 24 24" className="size-4" fill="currentColor">
            <rect x="6" y="5" width="4" height="14" rx="1" />
            <rect x="14" y="5" width="4" height="14" rx="1" />
          </svg>
        </span>
        <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-carte-2">
          <div
            className="h-full rounded-full bg-citron transition-all duration-300"
            style={{ width: `${Math.min(100, (rang / total) * 100)}%` }}
          />
        </div>
        <span className="text-[13px] font-bold tabular-nums text-encre-douce">1,25×</span>
      </div>
    </div>
  );
}
