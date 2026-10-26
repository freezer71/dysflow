import type { Article } from "../types";

export const dicteeVocale: Article = {
  id: "dictee-vocale",
  publie: "2026-10-26",
  fr: {
    titre: "Dictée vocale et dyslexie : écrire en parlant",
    description:
      "Avec une dyslexie, écrire coûte deux fois : trouver les mots, puis leur orthographe. La dictée vocale sépare les deux. Conseils pour bien dicter et se relire.",
    chapo:
      "Avec une dyslexie, écrire coûte deux fois : trouver ses mots, puis leur orthographe. La dictée vocale sépare les deux. Vous parlez, le texte s'écrit.",
    corps: `
## Pourquoi écrire est si dur

Écrire, c'est faire plusieurs choses à la fois : avoir l'idée, la mettre en phrase, trouver l'orthographe de chaque mot, taper. Avec une dyslexie ou une dysorthographie, l'orthographe prend une grande part de l'effort. L'idée se perd en route.

Alors on écrit plus court. On choisit des mots plus simples que ceux qu'on a en tête, parce qu'on sait les écrire. Ce n'est pas un manque de vocabulaire. C'est le prix de l'orthographe.

## Ce que fait la dictée

La reconnaissance vocale transforme la parole en texte. Elle écrit les mots avec leur orthographe : vous dites « géographie », elle écrit « géographie ».

On retrouve ses vrais mots. Et on écrit à la vitesse où l'on parle.

## Ses limites, honnêtement

- **Elle se trompe parfois** : un nom propre, un mot rare, un bruit de fond.
- **Les homophones** restent un piège : « et » ou « est », « a » ou « à », « ces » ou « ses ». Le modèle choisit d'après le contexte. Souvent bien, pas toujours.
- **La ponctuation** : selon le modèle, elle s'ajoute toute seule ou non.

Donc on se relit. Et quand lire est difficile, le plus simple est de **se relire à l'oreille** : faire lire son texte par une synthèse vocale. Les erreurs s'entendent. Voir [écouter en lisant](/fr/blog/ecouter-en-lisant-dyslexie).

## Bien dicter : six conseils

1. **Pensez la phrase avant de parler.** Une phrase à la fois.
2. **Parlez à un rythme normal**, pas mot à mot. Les modèles comprennent mieux une phrase entière.
3. **Dictez par petits morceaux**, une ou deux phrases, puis vérifiez.
4. **Un micro proche** compte plus qu'un micro cher. Un casque-micro suffit.
5. **Évitez le bruit de fond** : télévision, conversation, ventilateur.
6. **Relisez à l'oreille.** Sélectionnez votre texte et faites-le lire.

## Ce qui change au quotidien

Un mail au travail. Un message à l'école de vos enfants. Un devoir. Une demande à l'administration. Avec la dictée, on n'évite plus d'écrire. On écrit comme on parle, avec ses vrais mots.

## Avec Dysflow

Dans l'app macOS (la dictée demande macOS 26) :

1. appuyez sur la touche option de droite ;
2. dites ce que vous voulez ;
3. réappuyez : le texte s'écrit dans l'app au premier plan — Mail, Word, Pages, Messages, votre navigateur…

Les modèles de dictée tournent sur votre Mac : rien n'est envoyé en ligne. Plusieurs sont au choix ([voir les modèles de dictée](/fr/modeles#dictee)), et un banc d'essai au micro mesure leur justesse avant de choisir. Le raccourci se change dans les réglages.

Pour vous relire, sélectionnez le texte et faites ⌃ ⌥ Espace : Dysflow le lit à voix haute.

## À retenir

- Pour une personne dys, l'orthographe absorbe l'effort d'écrire.
- La dictée écrit les mots avec leur orthographe et rend leurs vrais mots aux gens.
- Elle se trompe parfois, surtout sur les homophones : relisez à l'oreille.
- Dictez phrase par phrase, avec un micro proche et sans bruit de fond.
`,
  },
  en: {
    titre: "Speech-to-text for dyslexia: write by talking",
    description:
      "With dyslexia, writing costs twice: finding the words, then spelling them. Dictation separates the two. Tips for dictating well and checking your text.",
    chapo:
      "With dyslexia, writing costs twice: finding your words, then spelling them. Dictation separates the two. You talk, the text appears.",
    corps: `
## Why writing is so hard

Writing means doing several things at once: having the idea, turning it into a sentence, spelling every word, typing. With dyslexia, spelling takes a large share of the effort. The idea gets lost on the way.

So you write less. You pick simpler words than the ones in your head, because you know how to spell them. It isn't a lack of vocabulary. It's the price of spelling.

## What dictation does

Speech recognition turns speech into text. It writes words with their spelling: you say "geography", it writes "geography".

You get your real words back. And you write as fast as you talk.

## Its limits, honestly

- **It sometimes gets things wrong**: a name, a rare word, background noise.
- **Homophones** are still a trap: "their", "there" or "they're"; "your" or "you're". The model picks from context. Often right, not always.
- **Punctuation**: depending on the model, it's added automatically or not.

So you check your text. And when reading is hard, the easiest way is to **check it by ear**: have your text read aloud. Mistakes are easy to hear. See [listening while reading](/blog/listening-while-reading-dyslexia).

## Dictating well: six tips

1. **Think of the sentence before you speak.** One sentence at a time.
2. **Speak at a normal pace**, not word by word. Models understand whole sentences better.
3. **Dictate in small chunks**, one or two sentences, then check.
4. **A close microphone** matters more than an expensive one. A headset is enough.
5. **Avoid background noise**: TV, conversations, fans.
6. **Check by ear.** Select your text and have it read aloud.

## What changes day to day

An email at work. A note to your child's school. Homework. A form for the tax office. With dictation, you stop avoiding writing. You write the way you talk, with your real words.

## With Dysflow

In the macOS app (dictation needs macOS 26):

1. press the right option key;
2. say what you want;
3. press it again: the text is typed into the app in front — Mail, Word, Pages, Messages, your browser…

The dictation models run on your Mac: nothing is sent online. You can choose between several ([see the dictation models](/models#dictee)), and a microphone test bench measures their accuracy before you pick. The shortcut can be changed in the settings.

To check your text, select it and press ⌃ ⌥ Space: Dysflow reads it aloud.

## Key points

- For dyslexic people, spelling eats up the effort of writing.
- Dictation spells the words for you and gives people their real words back.
- It sometimes makes mistakes, especially with homophones: check by ear.
- Dictate sentence by sentence, with a close microphone and no background noise.
`,
  },
};
