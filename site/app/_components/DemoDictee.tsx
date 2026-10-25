"use client";

import { useEffect, useRef, useState } from "react";
import { useCalme } from "./reglages";

// La dictée, en miniature. En haut, ce que le clavier aurait donné, en
// doutant de chaque mot ; en bas, la même phrase dite à voix haute, écrite
// mot à mot. Une image animée, sans son ni micro.

type Props = {
  libelles: { clavier: string; voix: string; micro: string };
  /** La phrase tapée en doutant (fautes comprises). */
  tapee: string;
  /** La même phrase, dictée. */
  dite: string;
};

export function DemoDictee({ libelles, tapee, dite }: Props) {
  const mots = dite.split(" ");
  const [rang, setRang] = useState(mots.length);
  const [visible, setVisible] = useState(false);
  const carte = useRef<HTMLDivElement>(null);
  const calme = useCalme();

  useEffect(() => {
    const el = carte.current;
    if (!el) return;
    const obs = new IntersectionObserver(([e]) => setVisible(e.isIntersecting), { threshold: 0.25 });
    obs.observe(el);
    return () => obs.disconnect();
  }, []);

  useEffect(() => {
    if (!visible || calme) return;
    // La phrase reste entière quelque temps, puis se réécrit mot à mot.
    const minuteur = window.setInterval(() => setRang((r) => (r + 1) % (mots.length + 6)), 330);
    return () => window.clearInterval(minuteur);
  }, [visible, mots.length, calme]);

  // Figée, la phrase se montre entière.
  const vus = calme ? mots.length : rang;
  const parle = vus < mots.length;

  return (
    <div ref={carte} className="space-y-3">
      <div className="rounded-[22px] bg-carte-2/70 p-6">
        <p className="text-sm font-bold text-encre-pale">{libelles.clavier}</p>
        <p className="mt-2 text-lg leading-relaxed text-encre-pale line-through decoration-encre-pale/50 sm:text-xl">{tapee}</p>
      </div>
      <div className="rounded-[22px] bg-carte p-6 shadow-[0_30px_80px_-30px_rgba(0,0,0,0.25)]">
        <div className="flex items-center gap-3">
          <span className="relative grid size-10 place-items-center rounded-full bg-citron text-citron-encre" aria-hidden="true">
            {parle && <span className="absolute inset-0 animate-ping rounded-full bg-citron/60" />}
            <svg viewBox="0 0 24 24" className="relative size-5" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M12 3a3 3 0 0 0-3 3v6a3 3 0 0 0 6 0V6a3 3 0 0 0-3-3Zm-7 9a7 7 0 0 0 14 0M12 19v3" />
            </svg>
          </span>
          <p className="text-sm font-bold text-encre-pale">{parle ? libelles.micro : libelles.voix}</p>
        </div>
        <p className="sr-only">{dite}</p>
        <p aria-hidden="true" className="mt-4 min-h-[3.5em] text-lg leading-relaxed sm:text-xl">
          {mots.slice(0, Math.min(vus, mots.length)).join(" ")}
          <span className={`ml-0.5 inline-block h-[1.1em] w-[2px] translate-y-[0.2em] bg-encre ${parle ? "" : "animate-pulse"}`} />
        </p>
      </div>
    </div>
  );
}
