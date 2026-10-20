import { cacheLife } from "next/cache";

export const depot = "https://github.com/freezer71/sillage";

// /releases/latest/download/<nom> mène toujours à la dernière publication :
// les noms des archives ne portent pas la version (release.yml), si bien que
// ces liens n'ont jamais à changer.
const derniere = `${depot}/releases/latest/download`;

/** La commande d'installation, Mac et Linux (install.sh, à la racine du dépôt). */
export const commandeInstallation =
  "curl -fsSL https://sillage.koumalabs.org/install.sh | sh";

export const commandeDesinstallation = `${commandeInstallation} -s -- --uninstall`;

export const telechargements = {
  dmg: `${derniere}/sillage-macos.dmg`,
  linuxX64: `${derniere}/sillage-linux-x86_64.tar.gz`,
  linuxArm64: `${derniere}/sillage-linux-aarch64.tar.gz`,
  publications: `${depot}/releases`,
  sources: `${depot}#lapp-macos`,
};

/**
 * Le numéro de la dernière publication, lu sur GitHub et gardé une heure :
 * au plus un appel à l'API par heure. `null` si GitHub ne répond pas — la
 * page s'affiche alors sans numéro.
 */
export async function derniereVersion(): Promise<string | null> {
  "use cache";
  cacheLife("hours");
  try {
    const reponse = await fetch("https://api.github.com/repos/freezer71/sillage/releases/latest", {
      headers: { Accept: "application/vnd.github+json" },
    });
    if (!reponse.ok) return null;
    const { tag_name } = (await reponse.json()) as { tag_name?: string };
    return tag_name ? tag_name.replace(/^v/, "") : null;
  } catch {
    return null;
  }
}
