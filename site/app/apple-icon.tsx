import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { ImageResponse } from "next/og";

// L'icône des écrans d'accueil iOS : la plaque de l'app, en PNG (iOS ne
// prend pas le SVG). Elle remplit le carré : iOS arrondit les coins lui-même.

export const size = { width: 180, height: 180 };
export const contentType = "image/png";

// Sans la marge autour de la plaque : iOS pose son propre arrondi.
const svg = (await readFile(join(process.cwd(), "app/icon.svg"), "utf8")).replace(
  'viewBox="0 0 64 64"',
  'viewBox="6.25 6.25 51.5 51.5"',
);

export default function Icon() {
  return new ImageResponse(
    (
      // Le fond de la plaque sous l'image : iOS remplirait de noir les coins transparents.
      <div style={{ display: "flex", width: "100%", height: "100%", background: "#191919" }}>
        <img src={`data:image/svg+xml;base64,${Buffer.from(svg).toString("base64")}`} width={180} height={180} alt="" />
      </div>
    ),
    size,
  );
}
