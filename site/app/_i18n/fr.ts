// Le texte du site en français : la référence. `en.ts` doit en suivre
// exactement la forme (le type `Dico` en est tiré).

export const fr = {
  meta: {
    titre: "sillage — le lecteur à voix haute qu'on suit des yeux",
    modele: "%s · sillage",
    description:
      "sillage lit vos documents à voix haute et surligne chaque mot. Voix neurales et dictée 100 % sur votre machine, gratuit et open source. macOS et Linux.",
  },
  entete: {
    accueil: "sillage, accueil",
    avantages: "Avantages",
    comparaison: "Comparaison",
    modeles: "Modèles",
    github: "GitHub",
    telecharger: "Télécharger",
    langues: "Langue",
  },
  pied: {
    licence: "Logiciel libre, sous licence MIT. Fait en France.",
    modeles: "Modèles",
    telecharger: "Télécharger",
    code: "Code source",
  },
  commande: { copier: "Copier", copie: "Copié ✓" },
  demo: {
    fichier: "rapport-annuel.pdf",
    titre: "Introduction",
    lignes: [
      "sillage lit à voix haute ce que",
      "vous lui donnez, et montre chaque",
      "mot au moment où il est dit.",
    ],
    vitesse: "1,25×",
  },
  accueil: {
    pastilles: ["Gratuit", "Open source", "100 % local"],
    titre: "Écoutez vos documents. Suivez chaque mot.",
    intro:
      "sillage lit à voix haute vos PDF, vos notes et vos captures d'écran, et surligne chaque mot au moment où il est dit. Des voix naturelles, calculées sur votre machine : rien ne part en ligne, et rien n'est à payer.",
    telecharger: "Télécharger sillage",
    github: "Voir le code sur GitHub",
    commande: "Ou en une commande, sur Mac (puce Apple) et Linux :",
    chiffres: [
      { valeur: "0 €", legende: "pour toujours, sans abonnement" },
      { valeur: "0 octet", legende: "de vos textes envoyé en ligne" },
      { valeur: "15×", legende: "plus vite que la parole sur Mac" },
      { valeur: "#", legende: "modèles de voix et de dictée" },
    ],
    avantagesTitre: {
      surtitre: "Pourquoi sillage",
      titre: "Un lecteur pensé pour être suivi des yeux",
      texte:
        "La plupart des lecteurs vocaux se contentent de parler. sillage comprend ce qu'il lit, et vous montre où vous en êtes.",
    },
    avantages: [
      {
        titre: "Vos textes restent chez vous",
        texte:
          "Les voix et la dictée tournent sur votre machine. Pas de compte, pas de serveur, pas de quota : un contrat, un dossier médical ou un brouillon ne quittent jamais votre ordinateur.",
      },
      {
        titre: "Chaque mot suivi des yeux",
        texte:
          "Le mot dit porte une pastille, sa ligne une bande discrète : le mot dit où en est la voix, la ligne dit où revenir quand on relève les yeux. Pensé pour la dyslexie et la lecture longue.",
      },
      {
        titre: "Il lit tout ce que vous lui donnez",
        texte:
          "Markdown, texte, PDF, PDF photocopié, capture d'écran, photo de page, presse-papiers, sélection dans n'importe quelle app. Les pages scannées sont reconnues en parallèle et mémorisées.",
      },
      {
        titre: "Il ne lit pas le formatage",
        texte:
          "Jamais de « dièse titre » ni de « tiret tiret ». Un bloc de code est annoncé, pas récité. Un tableau se lit colonne par colonne : « Route : /users. Méthode : GET. »",
      },
      {
        titre: "Plus vite, sans voix de canard",
        texte:
          "La vitesse agit sur le son pendant qu'il sonne : la voix accélère sans monter dans les aigus, au milieu de la phrase, sans coupure. Avance et recul sont immédiats.",
      },
      {
        titre: "Il reprend là où vous étiez",
        texte:
          "La position se retient toute seule — la phrase elle-même, pas un numéro. Elle se retrouve même si le document a été retouché entre-temps.",
      },
      {
        titre: "La dictée, en plus",
        texte:
          "Un raccourci ouvre la dictée, et le texte s'écrit dans l'app au premier plan. Parakeet, Whisper, Voxtral, Canary : choisissez le vôtre, tout reste local.",
      },
      {
        titre: "Libre, pour de vrai",
        texte:
          "Code ouvert sous licence MIT, modèles sous licences ouvertes. Vous pouvez le lire, le modifier, le redistribuer. Personne ne pourra vous le retirer ni le faire payer.",
      },
    ],
    pourQui: {
      surtitre: "Pour qui",
      titre: "Pour ceux qui lisent beaucoup, et pour ceux à qui lire coûte.",
      publics: [
        {
          titre: "Dyslexie, TDAH, fatigue visuelle",
          texte:
            "Écouter et suivre des yeux à la fois : la lecture bimodale, avec la police de votre choix (OpenDyslexic, Atkinson Hyperlegible, Literata).",
        },
        {
          titre: "Étudiants et chercheurs",
          texte:
            "Les PDF sont reconstruits : titres au sommaire, mots coupés recollés, en-têtes et pieds de page retirés. On écoute l'article, pas la mise en page.",
        },
        {
          titre: "Développeurs",
          texte:
            "Documentation, README, réponses d'IA pipées dans le terminal : `llm \"…\" | sillage` démarre la lecture avant la fin du flux.",
        },
      ],
    },
    comparaisonTitre: {
      surtitre: "Comparaison",
      titre: "Tout ce que font les grands, sans l'abonnement ni le cloud",
      texte:
        "Les lecteurs commerciaux envoient vos textes à leurs serveurs et font payer les belles voix. sillage fait le calcul chez vous.",
    },
    comparaison: {
      critere: "Critère",
      // « @oui », « @non » et « @partiel » ne s'affichent pas tels quels :
      // ce sont les mêmes codes dans les deux langues.
      concurrents: ["sillage", "Speechify", "ElevenReader", "NaturalReader", "Contenu énoncé (macOS)"],
      oui: "Oui",
      non: "Non",
      partiel: "En partie",
      lignes: [
        { critere: "Gratuit, sans limite d'usage", cases: ["@oui", "Gratuit limité, puis abonnement", "Gratuit limité, puis abonnement", "Gratuit limité, puis abonnement", "@oui"] },
        { critere: "Code source ouvert", cases: ["@oui", "@non", "@non", "@non", "@non"] },
        { critere: "Vos textes restent sur votre machine", cases: ["@oui", "@non", "@non", "@non", "@oui"] },
        { critere: "Voix neurales naturelles", cases: ["@oui", "@oui", "@oui", "@oui", "@partiel"] },
        { critere: "Mot lu surligné", cases: ["@oui", "@oui", "@oui", "@oui", "@oui"] },
        { critere: "Ne lit ni balisage, ni code, ni bordures de tableau", cases: ["@oui", "Non annoncé", "Non annoncé", "Non annoncé", "@non"] },
        { critere: "Reconnaissance des pages scannées", cases: ["@oui", "@oui", "@oui", "@oui", "@non"] },
        { critere: "Dictée intégrée, hors ligne", cases: ["@oui", "Non annoncé", "@non", "@non", "@partiel"] },
        { critere: "Version Linux", cases: ["Terminal", "Navigateur", "Navigateur", "Navigateur", "@non"] },
      ],
      note: "D'après ce que les éditeurs annoncent sur leurs sites, en octobre 2026. Une erreur ?",
      noteLien: "Dites-le-nous",
    },
    modelesTitre: {
      surtitre: "Modèles",
      titre: "Les meilleurs modèles ouverts, au choix",
      texte:
        "Téléchargez, essayez, supprimez : vous gardez la main sur la qualité, la vitesse et la place disque.",
    },
    lecture: "Lecture",
    dictee: "Dictée",
    voirModeles: "Voir les # modèles",
    questionsTitre: { surtitre: "Questions", titre: "Ce qu'on nous demande" },
    faq: [
      {
        q: "C'est vraiment gratuit ?",
        r: "Oui, et sans contrepartie : pas de compte, pas de publicité, pas de version « Pro ». sillage est un logiciel libre sous licence MIT ; les modèles se téléchargent depuis leurs dépôts publics.",
      },
      {
        q: "Mes documents sont-ils envoyés quelque part ?",
        r: "Non. La lecture, la reconnaissance de texte et la dictée se calculent sur votre machine. Le seul accès au réseau est le téléchargement des modèles que vous choisissez.",
      },
      {
        q: "Quel Mac faut-il ?",
        r: "Un Mac à puce Apple (M1 et suivants). La dictée demande macOS 26. Kokoro, la voix par défaut, occupe 342 Mo et moins de 6 % du temps de calcul pendant la lecture.",
      },
      {
        q: "Et sous Linux ?",
        r: "Le lecteur au terminal tourne sous Linux x86_64 et ARM64 (glibc 2.39 ou plus), avec la même voix Kokoro et la reconnaissance de texte intégrée. La même commande d'installation que sur Mac l'installe.",
      },
      {
        q: "Quelles langues ?",
        r: "La lecture est faite pour le français et l'anglais, avec une seule voix pour les deux. La dictée va jusqu'à 99 langues selon le modèle choisi.",
      },
      {
        q: "Et Windows ?",
        r: "Pas pour l'instant. Le moteur est en Rust et pourrait y être porté : les contributions sont bienvenues sur GitHub.",
      },
    ],
    final: {
      titre: "Votre prochain document, écoutez-le.",
      texte: "Gratuit, libre, et installé en une commande.",
    },
  },
  modeles: {
    meta: {
      titre: "Les modèles",
      description:
        "Toutes les voix et tous les modèles de dictée de sillage : Kokoro, Qwen3-TTS, MagpieTTS, Parakeet, Phonon-2, Whisper, Canary, Voxtral. Tailles, langues, vitesse, licences.",
    },
    entete: {
      surtitre: "Modèles",
      titre: "Choisissez votre voix, et votre dictée",
      texte:
        "Tous les modèles tournent sur votre machine. Ils se téléchargent, se choisissent et se suppriment dans les réglages de l'app : vous gardez la main sur la place disque. Les vitesses sont mesurées, pas promises.",
    },
    champs: {
      taille: "Taille",
      precision: "Précision",
      langues: "Langues",
      calcul: "Calcul",
      rythme: "Rythme",
      voix: "Voix",
      licence: "Licence",
      modele: "Modèle",
    },
    recommande: "Recommandé",
    lourd: "Lourd",
    source: "Le modèle sur Hugging Face",
    lecture: {
      titre: "Lecture à voix haute",
      texte:
        "Une seule voix pour toutes les langues. Kokoro s'installe tout seul au premier lancement ; la voix du système lit en attendant.",
    },
    dictee: {
      titre: "Dictée",
      texte:
        "Dans l'app macOS : un raccourci ouvre la dictée, et le texte s'écrit dans l'app au premier plan. Aucun modèle n'est imposé ; un banc d'essai au micro mesure la justesse avant de choisir.",
    },
    ocr: {
      titre: "Reconnaissance de texte",
      texte:
        "Pour les images, les captures et les PDF photocopiés, sillage embarque Tesseract avec le français et l'anglais : rien à installer. Les pages sont reconnues en parallèle et le résultat est mémorisé, si bien que la deuxième ouverture est immédiate.",
    },
    mesures:
      "Mesures faites sur un Mac M5. « Occupé x % du temps à 1× » : la part du temps de lecture passée à calculer la voix ; au-delà de 100 %, la lecture saccade.",
    telecharger: "Télécharger sillage",
  },
  telecharger: {
    meta: {
      titre: "Télécharger",
      description:
        "Installez sillage en une commande sur Mac et Linux, ou téléchargez l'app macOS en .dmg.",
    },
    version: "Version",
    derniere: "Dernière version",
    titre: "Installé en une commande.",
    intro: "Ouvrez le Terminal, collez, Entrée. Sur Mac comme sous Linux.",
    etapes: [
      "Choisit la bonne version pour votre machine",
      "Vérifie l'empreinte du téléchargement",
      "Installe sans mot de passe administrateur",
      "Ouvre sillage — la voix s'installe toute seule",
    ],
    ou: "ou téléchargez",
    mac: {
      pastille: "Puce Apple",
      texte:
        "Ouvrez le .dmg et glissez Sillage dans Applications. macOS 14 ou plus ; la dictée demande macOS 26.",
      bouton: "Télécharger le .dmg",
      noteAvant: "L'app n'est pas encore notariée par Apple : au premier lancement, ouvrez",
      noteReglages: "Réglages Système › Confidentialité et sécurité",
      noteMilieu: "et cliquez sur",
      noteOuvrir: "Ouvrir quand même",
      noteApres: ". La commande ci-dessus évite ce détour.",
    },
    linux: {
      pastille: "Terminal",
      texte:
        "Le lecteur au terminal, avec Kokoro et la reconnaissance de texte. Ubuntu 24.04, Debian 13, Fedora 40 ou plus récents.",
      archivesAvant: "Les archives et leurs empreintes sont aussi dans les",
      archivesLien: "publications GitHub",
    },
    miseAJour: {
      titre: "Mettre à jour",
      texte:
        "Une commande, et la dernière version remplace la vôtre. Vos modèles et vos réglages restent. Déjà à jour ? Elle vous le dit, sans rien retélécharger.",
    },
    desinstaller: {
      titre: "Désinstaller",
      texte: "La même commande, avec une option. Vos modèles et vos réglages restent.",
    },
    compilerAvant: "Vous préférez compiler ?",
    compilerLien: "Les instructions sont dans le README",
    souciAvant: "Un souci ?",
    souciLien: "Ouvrez une issue",
  },
} as const;

/** Ce que doit suivre chaque langue : la forme de `fr`, avec des `string`. */
type ProfondeurChaine<T> = T extends string
  ? string
  : { [K in keyof T]: ProfondeurChaine<T[K]> };

export type Dico = ProfondeurChaine<typeof fr>;
