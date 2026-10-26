import type { Article } from "../types";

export const pdfScanne: Article = {
  id: "pdf-scanne",
  publie: "2026-10-26",
  fr: {
    titre: "Lire à voix haute un PDF scanné ou une photo sur Mac",
    description:
      "Courrier scanné, photo de page, capture d'écran : comment les faire lire à voix haute sur Mac grâce à la reconnaissance de texte, sans rien envoyer en ligne.",
    chapo:
      "Un courrier scanné, une photo de page, une capture d'écran : ce sont des images. Pour les écouter, il faut d'abord retrouver le texte dedans. Voici comment.",
    corps: `
## Pourquoi ça ne marche pas tout seul

Un PDF scanné n'est souvent qu'une image : pas de texte dedans, seulement des pixels. Une synthèse vocale ordinaire n'a rien à lire.

Il faut d'abord la **reconnaissance optique de caractères** (OCR, en anglais) : un programme retrouve les lettres dans l'image et en refait du texte.

## Votre PDF a-t-il du texte ?

Ouvrez-le dans Aperçu et essayez de sélectionner un mot.

- La sélection marche : le PDF a une couche texte. Il peut être lu directement.
- Rien ne se sélectionne : c'est une image. Il faut la reconnaissance de texte.

## Avec Dysflow, en trois gestes

1. **Ouvrez le document** : ⌘O dans Dysflow, ou glissez-le sur la fenêtre. Une image copiée se colle avec ⌘V.
2. **Dysflow reconnaît le texte.** La reconnaissance (Tesseract, en français et en anglais) est livrée avec l'app : rien à installer. Les pages sont traitées en parallèle.
3. **La lecture commence**, et chaque mot est surligné au moment où il est dit.

Le résultat est mémorisé : la deuxième ouverture est immédiate. Et Dysflow rouvre le document là où vous l'aviez laissé.

## Un PDF avec du texte : encore mieux

Quand le PDF a une couche texte, Dysflow la remet en ordre avant de la lire :

- les titres sont repris dans le sommaire ;
- les mots coupés en fin de ligne sont recollés ;
- les en-têtes et les pieds de page sont retirés.

La voix ne lit donc pas « page 3 sur 12 » à chaque page.

## Pour une bonne reconnaissance

- Une photo **bien éclairée**, prise à plat, sans ombre.
- La page **entière et droite** dans le cadre.
- Un scan **net** : 300 points par pouce est un bon repère.
- L'**écriture à la main** est en général mal reconnue. Le texte imprimé passe bien mieux.

## Vos documents restent chez vous

Courrier de la banque, ordonnance, avis d'impôt : ce sont des documents privés. Dysflow calcule tout sur votre Mac, la reconnaissance comme la voix. Rien n'est envoyé en ligne, et il n'y a pas de compte à créer.

## Sous Linux

Le lecteur au terminal lit aussi les images et les PDF scannés, avec la même reconnaissance de texte : donnez-lui le fichier, ou le presse-papiers.

## À retenir

- Un PDF scanné est une image : il faut la reconnaissance de texte pour l'écouter.
- Le test simple : essayez de sélectionner un mot dans Aperçu.
- Dysflow reconnaît le texte et le lit en surlignant chaque mot, sur votre Mac, sans rien envoyer.
- Une bonne photo, à plat et bien éclairée, donne une bonne reconnaissance.

Envie de comprendre pourquoi écouter aide autant ? Lisez [écouter en lisant](/fr/blog/ecouter-en-lisant-dyslexie).
`,
  },
  en: {
    titre: "How to read a scanned PDF or photo aloud on a Mac",
    description:
      "Scanned letters, photos of pages, screenshots: how to have them read aloud on a Mac with text recognition, without sending anything online.",
    chapo:
      "A scanned letter, a photo of a page, a screenshot: they're all images. To listen to them, you first need to find the text inside. Here's how.",
    corps: `
## Why it doesn't just work

A scanned PDF is often just an image: no text inside, only pixels. Ordinary text-to-speech has nothing to read.

You first need **optical character recognition** (OCR): a program finds the letters in the image and turns them back into text.

## Does your PDF have text?

Open it in Preview and try to select a word.

- Selection works: the PDF has a text layer. It can be read directly.
- Nothing gets selected: it's an image. It needs text recognition.

## With Dysflow, in three steps

1. **Open the document**: ⌘O in Dysflow, or drag it onto the window. A copied image can be pasted with ⌘V.
2. **Dysflow recognises the text.** Recognition (Tesseract, in English and French) ships with the app: nothing to install. Pages are processed in parallel.
3. **Reading starts**, and each word is highlighted as it's spoken.

The result is remembered: the second time, it opens instantly. And Dysflow reopens the document where you left off.

## A PDF with text: even better

When the PDF has a text layer, Dysflow tidies it up before reading:

- headings go into the table of contents;
- words split across lines are joined back together;
- page headers and footers are removed.

So the voice doesn't read "page 3 of 12" on every page.

## For good recognition

- A **well-lit** photo, taken flat, with no shadow.
- The **whole page**, straight, in the frame.
- A **sharp** scan: 300 dots per inch is a good benchmark.
- **Handwriting** is usually poorly recognised. Printed text works much better.

## Your documents stay with you

Bank letters, prescriptions, tax notices: these are private documents. Dysflow does everything on your Mac, recognition and voice alike. Nothing is sent online, and there's no account to create.

## On Linux

The terminal reader also reads images and scanned PDFs, with the same text recognition: give it the file, or the clipboard.

## Key points

- A scanned PDF is an image: it needs text recognition before you can listen to it.
- The simple test: try selecting a word in Preview.
- Dysflow recognises the text and reads it while highlighting each word, on your Mac, without sending anything.
- A good photo, flat and well lit, gives good recognition.

Curious why listening helps so much? Read [listening while reading](/blog/listening-while-reading-dyslexia).
`,
  },
};
