import type { Metadata } from "next";
import { Bouton, IconeTelecharger, Pastille, TitreSection } from "../_components/ui";
import { depot, telechargements, version } from "../_data/liens";

export const metadata: Metadata = {
  title: "Télécharger",
  description: "Téléchargez sillon pour Linux (x86_64, ARM64) ou installez l'app macOS depuis les sources.",
};

function Code({ children }: { children: string }) {
  return (
    <pre className="mt-4 overflow-x-auto rounded-2xl bg-fond p-4 font-mono text-[14px] leading-relaxed">
      <code>{children}</code>
    </pre>
  );
}

export default function Telecharger() {
  return (
    <div className="mx-auto max-w-5xl px-4 pt-16 sm:px-6">
      <TitreSection
        surtitre={`Version ${version}`}
        titre="Télécharger sillon"
        texte="Gratuit, sans compte. La voix Kokoro s'installe toute seule au premier lancement."
      />

      <div className="mt-14 grid gap-4 md:grid-cols-2">
        {/* Linux */}
        <section className="flex min-w-0 flex-col rounded-[28px] bg-carte p-6 sm:p-8">
          <div className="flex items-center gap-2">
            <h2 className="text-2xl font-extrabold">Linux</h2>
            <Pastille className="bg-citron text-citron-encre">Prêt à l&apos;emploi</Pastille>
          </div>
          <p className="mt-2 leading-relaxed text-encre-douce">
            Le lecteur au terminal, avec Kokoro et la reconnaissance de texte. glibc 2.39 ou plus
            (Ubuntu 24.04, Debian 13, Fedora 40…).
          </p>
          <div className="mt-6 flex flex-col gap-3">
            <Bouton href={telechargements.linuxX64}>
              <IconeTelecharger />
              Linux x86_64
            </Bouton>
            <Bouton href={telechargements.linuxArm64} variante="doux">
              <IconeTelecharger />
              Linux ARM64
            </Bouton>
          </div>
          <Code>{`arch=$(uname -m)   # x86_64 ou aarch64
tar -xzf sillon-*-linux-$arch.tar.gz
mkdir -p ~/.local/lib ~/.local/bin
mv sillon-*-linux-$arch ~/.local/lib/sillon
ln -sfn ../lib/sillon/sillon ~/.local/bin/sillon
sillon document.md`}</Code>
          <p className="mt-3 text-sm text-encre-pale">
            La marche à suivre complète et les empreintes SHA-256 sont dans les{" "}
            <a href={telechargements.publications} className="underline underline-offset-2 hover:text-encre">
              notes de la publication
            </a>
            .
          </p>
        </section>

        {/* macOS */}
        <section className="flex min-w-0 flex-col rounded-[28px] bg-carte p-6 sm:p-8">
          <div className="flex items-center gap-2">
            <h2 className="text-2xl font-extrabold">macOS</h2>
            <Pastille className="bg-carte-2 text-encre-douce">Depuis les sources</Pastille>
          </div>
          <p className="mt-2 leading-relaxed text-encre-douce">
            L&apos;app complète : fenêtre de lecture, lecture de la sélection, dictée. Mac à puce Apple ;
            la dictée demande macOS 26. Il faut Xcode, XcodeGen et Rust.
          </p>
          <div className="mt-6 flex flex-col gap-3">
            <Bouton href={telechargements.sources}>
              Instructions d&apos;installation
            </Bouton>
          </div>
          <Code>{`git clone ${depot}.git
cd sillon
./scripts/build-macos.sh
cp -R macos/build/Release/Sillon.app /Applications/`}</Code>
          <p className="mt-3 text-sm text-encre-pale">
            Une app signée, à glisser dans Applications, viendra avec une prochaine version.
          </p>
        </section>
      </div>

      <section className="mt-4 rounded-[28px] bg-carte p-8">
        <h2 className="text-xl font-extrabold">Le terminal sur Mac</h2>
        <p className="mt-2 leading-relaxed text-encre-douce">
          Le lecteur au terminal marche aussi sur Mac, à côté de l&apos;app :
        </p>
        <Code>{`git clone ${depot}.git && cd sillon
./scripts/install.sh     # Kokoro + reconnaissance de texte, dans ~/.local
cat notes.md | sillon`}</Code>
      </section>

      <p className="mt-10 text-center text-encre-douce">
        Une question, un souci ?{" "}
        <a href={`${depot}/issues`} className="font-bold text-encre underline underline-offset-2">
          Ouvrez une issue sur GitHub
        </a>
        .
      </p>
    </div>
  );
}
