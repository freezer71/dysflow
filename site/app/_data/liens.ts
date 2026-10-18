export const depot = "https://github.com/freezer71/sillon";
export const version = "0.2.1";

const publication = `${depot}/releases/download/v${version}`;

/** La commande d'installation, Mac et Linux (install.sh, à la racine du dépôt). */
export const commandeInstallation =
  "curl -fsSL https://raw.githubusercontent.com/freezer71/sillon/main/install.sh | sh";

export const commandeDesinstallation = `${commandeInstallation} -s -- --desinstaller`;

export const telechargements = {
  dmg: `${publication}/sillon-${version}.dmg`,
  linuxX64: `${publication}/sillon-${version}-linux-x86_64.tar.gz`,
  linuxArm64: `${publication}/sillon-${version}-linux-aarch64.tar.gz`,
  publications: `${depot}/releases`,
  sources: `${depot}#lapp-macos`,
};
