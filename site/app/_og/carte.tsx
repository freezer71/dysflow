import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { ImageResponse } from "next/og";
import { dico, type Lang, type Page } from "../_i18n";

// Les images de partage (Open Graph) : la carte que montrent les réseaux et
// les messageries quand on colle un lien. Servies à une adresse fixe,
// `<page>/og.png` (`imageDe`), que reprennent les balises et le JSON-LD.
// Les couleurs du site, sa police, son logo, et une ligne de lecture dont
// un mot est surligné en citron — ce que fait Dysflow.

export const taille = { width: 1200, height: 630 };

// ImageResponse ne lit ni le woff2 ni les polices variables : deux graisses
// fixes d'Atkinson Hyperlegible Next, en TTF (licence OFL, dans le dossier).
const ressources = Promise.all([
  readFile(join(process.cwd(), "assets/fonts/AtkinsonHyperlegibleNext-400.ttf")),
  readFile(join(process.cwd(), "assets/fonts/AtkinsonHyperlegibleNext-800.ttf")),
  readFile(join(process.cwd(), "app/icon.svg"), "utf8"),
]);

const couleurs = {
  fond: "#f8f3e6",
  encre: "#1f1c17",
  douce: "#4f4a40",
  pale: "#645e51",
  filet: "#e2d9c3",
  citron: "#c6f36b",
};

/** La ligne de lecture : des mots en barres grises, celui qui est dit en citron. */
const mots = [118, 74, 160, 96, 132, 58, 146, 88];
const motDit = 3;

export async function carte({ surtitre, titre, texte }: { surtitre: string; titre: string; texte?: string }) {
  const [normale, grasse, logo] = await ressources;
  // Le titre rapetisse quand il s'allonge, pour tenir sur trois lignes.
  const corps = titre.length > 70 ? 58 : titre.length > 45 ? 66 : 80;
  return new ImageResponse(
    (
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          width: "100%",
          height: "100%",
          padding: "60px 72px",
          background: couleurs.fond,
          color: couleurs.encre,
          fontFamily: "Atkinson",
        }}
      >
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
          <div style={{ display: "flex", alignItems: "center", gap: 16 }}>
            {/* eslint-disable-next-line @next/next/no-img-element */}
            <img src={`data:image/svg+xml;base64,${Buffer.from(logo).toString("base64")}`} width={72} height={72} alt="" />
            <span style={{ fontSize: 40, fontWeight: 800, letterSpacing: -1 }}>Dysflow</span>
          </div>
          <span
            style={{
              display: "flex",
              padding: "10px 24px",
              borderRadius: 999,
              background: couleurs.citron,
              fontSize: 26,
              fontWeight: 800,
            }}
          >
            {surtitre}
          </span>
        </div>

        <div style={{ display: "flex", flexDirection: "column", marginTop: "auto", marginBottom: "auto" }}>
          <div style={{ display: "flex", fontSize: corps, fontWeight: 800, lineHeight: 1.06, letterSpacing: -2 }}>{titre}</div>
          {texte && (
            <div style={{ display: "flex", marginTop: 24, fontSize: 32, lineHeight: 1.3, color: couleurs.douce }}>{texte}</div>
          )}
        </div>

        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
          <div style={{ display: "flex", gap: 12 }}>
            {mots.map((largeur, i) => (
              <div
                key={i}
                style={{
                  width: largeur * 0.62,
                  height: 18,
                  borderRadius: 9,
                  background: i === motDit ? couleurs.citron : couleurs.filet,
                  boxShadow: i === motDit ? `0 0 0 2px ${couleurs.encre}` : "none",
                }}
              />
            ))}
          </div>
          <span style={{ fontSize: 26, color: couleurs.pale }}>dysflow.koumalabs.org</span>
        </div>
      </div>
    ),
    {
      ...taille,
      fonts: [
        { name: "Atkinson", data: normale, weight: 400, style: "normal" },
        { name: "Atkinson", data: grasse, weight: 800, style: "normal" },
      ],
    },
  );
}

/** Le contenu de la carte d'une page du site. */
export function cartePage(lang: Lang, page: Page) {
  const d = dico(lang);
  switch (page) {
    case "accueil":
      return carte({ surtitre: d.accueil.pastilles.join(" · "), titre: d.accueil.titre, texte: d.accueil.intro });
    case "modeles":
      return carte({ surtitre: d.entete.modeles, titre: d.modeles.entete.titre, texte: d.modeles.meta.titre });
    case "telecharger":
      return carte({ surtitre: d.entete.telecharger, titre: d.telecharger.titre, texte: d.telecharger.intro });
    case "blog":
      return carte({ surtitre: d.blog.surtitre, titre: d.blog.titre, texte: d.blog.texte });
    case "confidentialite":
      return carte({ surtitre: d.pied.confidentialite, titre: d.confidentialite.titre, texte: d.confidentialite.intro });
  }
}

