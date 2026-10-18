import type { Metadata } from "next";
import { Atkinson_Hyperlegible_Next, Atkinson_Hyperlegible_Mono } from "next/font/google";
import { Entete, Pied } from "./_components/ui";
import "./globals.css";

const atkinson = Atkinson_Hyperlegible_Next({
  variable: "--font-atkinson",
  subsets: ["latin", "latin-ext"],
});

const atkinsonMono = Atkinson_Hyperlegible_Mono({
  variable: "--font-atkinson-mono",
  subsets: ["latin"],
});

export const metadata: Metadata = {
  title: {
    default: "sillon — le lecteur à voix haute qu'on suit des yeux",
    template: "%s · sillon",
  },
  description:
    "sillon lit vos documents à voix haute et surligne chaque mot. Voix neurales et dictée 100 % sur votre machine, gratuit et open source. macOS et Linux.",
};

export default function RootLayout({ children }: LayoutProps<"/">) {
  return (
    <html lang="fr" className={`${atkinson.variable} ${atkinsonMono.variable} h-full antialiased`}>
      <body className="flex min-h-full flex-col font-sans">
        <Entete />
        <main className="flex-1">{children}</main>
        <Pied />
      </body>
    </html>
  );
}
