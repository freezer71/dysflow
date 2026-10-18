export const depot = "https://github.com/freezer71/sillon";
export const version = "0.2.0";

const publication = `${depot}/releases/download/v${version}`;

export const telechargements = {
  linuxX64: `${publication}/sillon-${version}-linux-x86_64.tar.gz`,
  linuxArm64: `${publication}/sillon-${version}-linux-aarch64.tar.gz`,
  publications: `${depot}/releases`,
  sources: `${depot}#lapp-macos`,
};
