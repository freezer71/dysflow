"use client";

import { useEffect, useRef, useState } from "react";
import { useCalme } from "./reglages";

// Le même texte, lu de deux façons. « Sans » : les lettres glissent, les b
// deviennent des d, un mot se mélange — l'effort du déchiffrage, rendu
// visible. « Avec » : rien ne bouge, la voix lit et la pastille citron suit
// le mot dit. Ce n'est pas ce que voit chaque dyslexique : la dyslexie se
// vit plus qu'elle ne se voit, l'effet montre l'effort.
//
// Le serveur rend le texte intact ; le désordre ne naît qu'après coup, dans
// le navigateur, et seulement quand la carte est à l'écran.

export type Mode = "sans" | "avec";

type Props = {
  /** Les lignes du texte, mots séparés par une espace. */
  lignes: readonly string[];
  modeInitial: Mode;
  libelles: {
    sans: string;
    avec: string;
    legendeSans: string;
    legendeAvec: string;
    voix: string;
    groupe: string;
  };
};

const MIROIRS: Record<string, string> = { b: "d", d: "b", p: "q", q: "p", n: "u", u: "n", m: "w" };

/** Une lettre affichée : ce qu'elle est devenue, et comment elle bouge. */
type Lettre = { c: string; y: number; r: number };

function intact(mot: string): Lettre[] {
  return [...mot].map((c) => ({ c, y: 0, r: 0 }));
}

function trouble(mot: string): Lettre[] {
  const lettres = [...mot];
  const tirage = Math.random();
  if (tirage < 0.09 && lettres.length > 3) {
    // Le milieu du mot se mélange ; la première et la dernière tiennent.
    const milieu = lettres.slice(1, -1);
    for (let i = milieu.length - 1; i > 0; i--) {
      const j = Math.floor(Math.random() * (i + 1));
      [milieu[i], milieu[j]] = [milieu[j], milieu[i]];
    }
    lettres.splice(1, milieu.length, ...milieu);
  }
  return lettres.map((c) => {
    const miroir = MIROIRS[c] && Math.random() < 0.35 ? MIROIRS[c] : c;
    const bouge = Math.random() < 0.12;
    return { c: miroir, y: bouge ? (Math.random() - 0.5) * 7 : 0, r: bouge ? (Math.random() - 0.5) * 14 : 0 };
  });
}

export function Simulation({ lignes: textes, modeInitial, libelles }: Props) {
  const lignes = textes.map((t) => t.split(" "));
  const debuts = lignes.map((_, i) => lignes.slice(0, i).reduce((n, l) => n + l.length, 0));
  const total = lignes.reduce((n, l) => n + l.length, 0);

  const [mode, setMode] = useState<Mode>(modeInitial);
  const [visible, setVisible] = useState(false);
  const [image, setImage] = useState<Lettre[][][] | null>(null);
  const [floue, setFloue] = useState(-1);
  const [rang, setRang] = useState(0);
  const carte = useRef<HTMLDivElement>(null);
  // Calme : demandé au système, ou « Animations : non » dans le panneau.
  const calme = useCalme();

  // N'animer que ce qu'on regarde : le désordre fatigue, et coûte.
  useEffect(() => {
    const el = carte.current;
    if (!el) return;
    const obs = new IntersectionObserver(([e]) => setVisible(e.isIntersecting), { threshold: 0.25 });
    obs.observe(el);
    return () => obs.disconnect();
  }, []);

  useEffect(() => {
    if (mode !== "sans") return;
    const tirer = () => {
      setImage(textes.map((t) => t.split(" ").map(trouble)));
      setFloue(Math.random() < 0.3 ? Math.floor(Math.random() * textes.length) : -1);
    };
    // Sans mouvement demandé : un seul état troublé, figé.
    if (calme || !visible) {
      if (calme) tirer();
      return;
    }
    tirer();
    const minuteur = window.setInterval(tirer, 170);
    return () => window.clearInterval(minuteur);
  }, [mode, visible, textes, calme]);

  useEffect(() => {
    if (mode !== "avec" || !visible || calme) return;
    const minuteur = window.setInterval(() => setRang((r) => (r + 1) % (total + 4)), 380);
    return () => window.clearInterval(minuteur);
  }, [mode, visible, total, calme]);

  const choisir = (m: Mode) => {
    setMode(m);
    setRang(0);
  };

  return (
    <div ref={carte} className="rounded-[28px] bg-carte p-2 shadow-[0_30px_80px_-30px_rgba(0,0,0,0.25)]">
      <div className="flex flex-wrap items-center justify-between gap-3 px-3 pb-3 pt-1">
        <div role="group" aria-label={libelles.groupe} className="flex flex-wrap rounded-[20px] bg-carte-2 p-1 text-sm font-bold">
          {(["sans", "avec"] as const).map((m) => (
            <button
              key={m}
              type="button"
              aria-pressed={mode === m}
              onClick={() => choisir(m)}
              className={`rounded-full px-4 py-1.5 transition-colors ${
                mode === m ? (m === "avec" ? "bg-citron text-citron-encre" : "bg-encre text-sur-encre") : "text-encre-douce hover:text-encre"
              }`}
            >
              {m === "sans" ? libelles.sans : libelles.avec}
            </button>
          ))}
        </div>
        {mode === "avec" && (
          <span className="flex items-center gap-2 text-[13px] font-bold text-encre-douce">
            <span className="flex h-4 items-end gap-[3px]" aria-hidden="true">
              {[0, 1, 2, 3].map((i) => (
                <span key={i} className="egaliseur w-[3px] rounded-full bg-citron" style={{ animationDelay: `${i * 0.13}s` }} />
              ))}
            </span>
            {libelles.voix}
          </span>
        )}
      </div>

      <div className="rounded-[20px] bg-fond px-3 py-5 sm:px-8 sm:py-8">
        {/* Le texte vrai, pour les lecteurs d'écran : le dessin est muet. */}
        <p className="sr-only">{textes.join(" ")}</p>
        <div aria-hidden="true" className="space-y-1 text-[17px] leading-[1.9] sm:text-xl">
          {lignes.map((mots, i) => {
            const debut = debuts[i];
            const ligneActive = mode === "avec" && rang >= debut && rang < debut + mots.length;
            return (
              <p
                key={i}
                className={`-mx-2 rounded-xl px-2 transition-[background-color,filter] duration-300 ${ligneActive ? "bg-bande" : ""}`}
                style={mode === "sans" && floue === i ? { filter: "blur(1.2px)" } : undefined}
              >
                {mots.map((mot, j) => {
                  const k = debut + j;
                  if (mode === "avec") {
                    const actif = k === rang;
                    // Figé, le texte se montre entier : rien ne reste en gris.
                    const lu = calme || k < rang;
                    return (
                      <span key={j}>
                        <span
                          className={`rounded-lg px-0.5 transition-colors duration-200 ${
                            actif ? "bg-citron text-citron-encre" : lu ? "text-encre" : "text-encre-pale"
                          }`}
                        >
                          {mot}
                        </span>{" "}
                      </span>
                    );
                  }
                  const lettres = image?.[i]?.[j] ?? intact(mot);
                  return (
                    <span key={j}>
                      {/* Un mot ne se coupe jamais : ses lettres sont des blocs. */}
                      <span className="whitespace-nowrap px-0.5">
                        {lettres.map((l, n) => (
                          <span
                            key={n}
                            className="inline-block"
                            style={l.y || l.r ? { transform: `translateY(${l.y}px) rotate(${l.r}deg)` } : undefined}
                          >
                            {l.c}
                          </span>
                        ))}
                      </span>{" "}
                    </span>
                  );
                })}
              </p>
            );
          })}
        </div>
      </div>

      <p className="px-4 pb-2 pt-4 text-lg leading-relaxed text-encre-douce">
        {mode === "sans" ? libelles.legendeSans : libelles.legendeAvec}
      </p>
    </div>
  );
}
