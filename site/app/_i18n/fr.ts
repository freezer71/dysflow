// Le texte du site en français : la référence. `en.ts` doit en suivre
// exactement la forme (le type `Dico` en est tiré).

export const fr = {
  meta: {
    titre: "Dysflow — lire et écrire sans s'épuiser, pour les dyslexiques",
    modele: "%s · Dysflow",
    description:
      "Dysflow est fait pour les personnes dyslexiques et neuroatypiques à qui lire ou écrire coûte : il lit vos documents à voix haute en surlignant chaque mot, et écrit ce que vous dictez. Gratuit, libre, 100 % sur votre machine.",
  },
  entete: {
    accueil: "Dysflow, accueil",
    parcours: "Le parcours",
    modeles: "Modèles",
    blog: "Blog",
    github: "GitHub",
    telecharger: "Télécharger",
    langues: "Langue",
  },
  pied: {
    licence: "Logiciel libre, sous licence MIT. Fait en France.",
    modeles: "Modèles",
    blog: "Blog",
    telecharger: "Télécharger",
    code: "Code source",
    confidentialite: "Confidentialité",
  },
  evitement: "Aller au contenu",
  confort: {
    bouton: "Confort",
    titre: "Confort de lecture",
    modeDys: "Mode dys : tout activer",
    reinitialiser: "Revenir au réglage de base",
    fermer: "Fermer",
    police: { titre: "Police", atkinson: "Atkinson", lexend: "Lexend" },
    taille: { titre: "Taille du texte", normale: "Normale", grande: "Grande", tresgrande: "Très grande" },
    espacement: { titre: "Espacement", normal: "Normal", large: "Large" },
    fond: { titre: "Fond", creme: "Crème", gris: "Gris", sombre: "Sombre" },
    animations: { titre: "Animations", oui: "Oui", non: "Non" },
  },
  commande: { copier: "Copier", copie: "Copié ✓" },
  accueil: {
    pastilles: [
      "Pour les dys",
      "Gratuit",
      "100 % local",
    ],
    titre: "Votre dyslexie n'est pas une fatalité.",
    intro: "Dysflow lit vos textes à voix haute. Et il écrit ce que vous dites.",
    telecharger: "Télécharger Dysflow",
    parcours: {
      surtitre: "Le parcours",
      titre: "Voyez la différence",
      lire: {
        titre: "Lire, quand on est dys",
        texte: "Essayez de lire ce message.",
        note: "Une image, pas un diagnostic.",
      },
      cout: {
        titre: "Ce que ça coûte",
        texte: "Ce n'est pas un manque d'intelligence.",
        points: [
          "On relit trois fois.",
          "On s'épuise avant de comprendre.",
          "On finit par se croire derrière.",
        ],
      },
      avec: {
        titre: "Avec Dysflow",
        texte: "La voix lit. Vos yeux suivent.",
      },
      ecrire: {
        titre: "Écrire en parlant",
        texte: "Vous parlez. Le texte s'écrit.",
      },
      simulation: {
        lignes: [
          "Bonjour à tous, la réunion",
          "de demain est déplacée à",
          "quatorze heures. Pensez à relire",
          "le dossier avant de venir, et à",
          "préparer vos questions : nous",
          "déciderons ensemble de la suite.",
        ],
        sans: "Sans Dysflow",
        avec: "Avec Dysflow",
        groupe: "Choisir la lecture",
        voix: "La voix lit",
        legendeSans: "Les lettres bougent. On relit. On se fatigue.",
        legendeAvec: "Rien ne bouge. Le sens arrive tout seul.",
      },
      dictee: {
        clavier: "Au clavier",
        micro: "Vous parlez…",
        voix: "Dysflow a écrit",
        tapee: "Bonjour madame, je vous envoi mon devoire de géografie.",
        dite: "Bonjour madame, je vous envoie mon devoir de géographie.",
      },
    },
    modeEmploi: {
      surtitre: "Mode d'emploi",
      titre: "Deux gestes, c'est tout",
      lire: {
        titre: "Pour lire",
        etapes: [
          { texte: "Sélectionnez le texte, ou copiez-le." },
          { texte: "Faites le raccourci.", touches: [{ signe: "⌃", nom: "control" }, { signe: "⌥", nom: "option" }, { signe: "␣", nom: "espace" }] },
          { texte: "Dysflow le lit à voix haute." },
        ],
      },
      ecrire: {
        titre: "Pour écrire",
        etapes: [
          { texte: "Appuyez sur la touche option de droite.", touches: [{ signe: "⌥", nom: "option" }] },
          { texte: "Dites ce que vous voulez." },
          { texte: "Réappuyez : le texte s'écrit." },
        ],
      },
      note: "Les raccourcis se changent dans les réglages.",
    },
    partout: {
      surtitre: "Partout",
      titre: "Dans toutes vos applications",
      texte: "Là où il y a du texte, Dysflow lit. Là où on écrit, Dysflow écrit.",
      apps: [
        { nom: "Mail", icone: "mail" },
        { nom: "Safari", icone: "safari" },
        { nom: "Chrome", icone: "chrome" },
        { nom: "Word", icone: "word" },
        { nom: "Pages", icone: "pages" },
        { nom: "Notes", icone: "notes" },
        { nom: "Messages", icone: "messages" },
        { nom: "WhatsApp", icone: "whatsapp" },
        { nom: "Slack", icone: "slack" },
        { nom: "Aperçu", icone: "apercu" },
        { nom: "Claude", icone: "claude" },
        { nom: "Claude Code", icone: "claude-code" },
        { nom: "ChatGPT", icone: "chatgpt" },
        { nom: "Terminal", icone: "terminal" },
      ],
      plus: "Et tout le reste du Mac",
    },
    manifeste: {
      titre: "Ne soyez plus derrière.",
      lignes: [
        "Votre savoir compte plus que votre orthographe.",
        "Montrez au monde ce que vous savez.",
      ],
      texte: "Gratuit. Rien à vendre.",
    },
    avantagesTitre: {
      surtitre: "En plus",
      titre: "Simple, et à vous",
    },
    avantages: [
      {
        titre: "Tout se lit",
        texte: "PDF, courrier scanné, photo, capture d'écran.",
      },
      {
        titre: "À votre rythme",
        texte: "Plus lent, plus vite, sans couper.",
      },
      {
        titre: "Rien ne sort de chez vous",
        texte: "Tout se calcule sur votre ordinateur.",
      },
      {
        titre: "Gratuit, pour toujours",
        texte: "Logiciel libre. Pas d'abonnement.",
      },
    ],
    questionsTitre: {
      surtitre: "Questions",
      titre: "Ce qu'on nous demande",
    },
    faq: [
      {
        q: "C'est en quelle langue ?",
        r: "Celle de votre Mac : français ou anglais. Le site aussi suit votre navigateur.",
      },
      {
        q: "Ça remplace un orthophoniste ?",
        r: "Non. C'est un outil, comme des lunettes.",
      },
      {
        q: "C'est vraiment gratuit ?",
        r: "Oui. Pas de compte, pas de publicité.",
      },
      {
        q: "Mes textes partent sur internet ?",
        r: "Non. Tout reste sur votre ordinateur.",
      },
      {
        q: "Sur quel ordinateur ?",
        r: "Un Mac à puce Apple. Sous Linux, au terminal.",
      },
    ],
    final: {
      titre: "La prochaine page, écoutez-la.",
      texte: "Gratuit. Installé en une minute.",
    },
  },
  modeles: {
    meta: {
      titre: "Voix de synthèse et modèles de dictée",
      description:
        "Toutes les voix et tous les modèles de dictée de Dysflow : Kokoro, Qwen3-TTS, MagpieTTS, Parakeet, Phonon-2, Whisper, Canary, Voxtral. Tailles, langues, vitesse, licences.",
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
        "Pour les images, les captures et les PDF photocopiés, Dysflow embarque Tesseract avec le français et l'anglais : rien à installer. Les pages sont reconnues en parallèle et le résultat est mémorisé, si bien que la deuxième ouverture est immédiate.",
    },
    mesures:
      "Mesures faites sur un Mac M5. « Occupé x % du temps à 1× » : la part du temps de lecture passée à calculer la voix ; au-delà de 100 %, la lecture saccade.",
    telecharger: "Télécharger Dysflow",
  },
  telecharger: {
    meta: {
      titre: "Télécharger pour Mac et Linux",
      description:
        "Installez Dysflow en une commande sur Mac et Linux, ou téléchargez l'app macOS en .dmg.",
    },
    version: "Version",
    derniere: "Dernière version",
    titre: "Installé en une commande.",
    intro: "Ouvrez le Terminal, collez, Entrée. Sur Mac comme sous Linux.",
    etapes: [
      "Choisit la bonne version pour votre machine",
      "Vérifie l'empreinte du téléchargement",
      "Installe sans mot de passe administrateur",
      "Ouvre Dysflow — la voix s'installe toute seule",
    ],
    ou: "ou téléchargez",
    mac: {
      pastille: "Puce Apple",
      texte:
        "Ouvrez le .dmg et glissez Dysflow dans Applications. macOS 14 ou plus ; la dictée demande macOS 26.",
      bouton: "Télécharger le .dmg",
      note: "Signée et notariée par Apple : elle s'ouvre sans alerte.",
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
      texte: "La même commande, avec une option. L'app, les modèles et les réglages partent ensemble.",
      garder: "Pour garder les modèles en vue d'une réinstallation, ajoutez",
    },
    compilerAvant: "Vous préférez compiler ?",
    compilerLien: "Les instructions sont dans le README",
    souciAvant: "Un souci ?",
    souciLien: "Ouvrez une issue",
  },
  confidentialite: {
    meta: {
      titre: "Confidentialité",
      description:
        "Ce que le site de Dysflow fait de vos données : une mesure de fréquentation anonyme, sans publicité, et rien de plus. L'app, elle, n'envoie rien.",
    },
    titre: "Confidentialité",
    intro:
      "Dysflow s'utilise sans compte et fonctionne entièrement sur votre ordinateur : l'app n'envoie rien. Cette page dit ce que ce site, dysflow.koumalabs.org, fait de vos données quand vous le visitez.",
    maj: "Dernière mise à jour : 31 octobre 2026",
    responsable: {
      titre: "Qui est responsable",
      texte:
        "Tharshan VISHNUKUMAR EI (Kouma Studio), entrepreneur individuel, SIREN 984 733 204, 5 rue Jean-Baptiste Lully, 77340 Pontault-Combault, France.",
      contact: "Pour toute question :",
    },
    audience: {
      titre: "Mesure d'audience",
      paragraphes: [
        "Nous mesurons la fréquentation de dysflow.koumalabs.org avec Moka, notre propre outil de mesure d'audience, développé et exploité par Kouma Studio. Ces données servent uniquement à nos statistiques : elles ne sont ni vendues ni utilisées à d'autres fins.",
        "La mesure se limite à des statistiques de fréquentation, établies à l'aide d'identifiants pseudonymes stockés dans les cookies moka_aid (13 mois à compter du premier dépôt, sans prolongation) et moka_session_id (30 minutes, prolongées à chaque page vue) : pages vues, provenance, actions suivies sur le site (clics sur les boutons de téléchargement, copie de la commande d'installation), liens sortants, type d'appareil, navigateur, taille d'écran, pays, région et ville (l'adresse IP n'est pas conservée).",
        "Ces cookies servent uniquement à produire des statistiques de fréquentation pour notre compte : ils ne vous suivent pas sur d'autres sites et ne sont croisés avec aucune autre donnée. Ils sont donc dispensés de consentement (article 82 de la loi Informatique et Libertés). La mesure repose sur notre intérêt légitime à savoir comment le site est utilisé, pour l'améliorer (article 6.1.f du RGPD).",
        "Les données de navigation sont conservées 25 mois.",
      ],
      opposition: "Vous pouvez vous opposer à toute mesure en cliquant sur le lien ci-dessous.",
      bouton: "Ne plus être mesuré par Moka",
      desactive: "La mesure est désactivée sur ce navigateur.",
      reprendre: "Réactiver la mesure",
    },
    stockage: {
      titre: "Cookies et stockage",
      elements: [
        "moka_aid et moka_session_id : la mesure d'audience, décrite plus haut.",
        "langue : posé seulement si vous choisissez une langue avec le sélecteur, pour vous afficher le site dans cette langue. Il contient « en » ou « fr » et dure un an.",
        "Les réglages de confort (police, taille, espacement, fond, animations) restent dans votre navigateur (stockage local) : ils ne sont jamais envoyés.",
        "moka_optout : posé dans votre navigateur seulement si vous vous opposez à la mesure, pour s'en souvenir.",
      ],
    },
    hebergement: {
      titre: "Hébergement",
      paragraphes: [
        "Le site est hébergé par Cloudflare, Inc., 101 Townsend St, San Francisco, CA 94107, États-Unis. Pour vous servir les pages et protéger le site contre les abus, Cloudflare traite votre adresse IP et les informations techniques envoyées par votre navigateur, en tant que sous-traitant. Ce transfert vers les États-Unis est encadré par le Data Privacy Framework UE–États-Unis, auquel Cloudflare est certifiée.",
        "Les polices sont servies par ce site lui-même : aucune requête n'est envoyée à Google.",
      ],
    },
    liens: {
      titre: "Téléchargements et liens",
      texte:
        "Les fichiers à télécharger et le code source sont hébergés sur GitHub. En suivant un lien de téléchargement ou vers le dépôt, vous quittez ce site : GitHub traite alors vos données selon",
      lien: "sa propre déclaration de confidentialité",
    },
    droits: {
      titre: "Vos droits",
      paragraphes: [
        "Vous pouvez demander l'accès à vos données, leur rectification, leur effacement ou la limitation de leur traitement, et vous opposer à leur traitement. Nous répondons dans un délai d'un mois.",
        "Les statistiques de fréquentation reposent sur des identifiants pseudonymes : sans eux, nous ne pouvons pas retrouver vos visites. Le plus simple pour ne plus être mesuré reste le lien plus haut.",
      ],
      ecrire: "Pour exercer vos droits, écrivez à",
      cnilAvant: "Vous pouvez aussi adresser une réclamation à la CNIL :",
      cnilLien: "cnil.fr/fr/plaintes",
    },
  },
  blog: {
    meta: {
      titre: "Blog : lire et écrire avec une dyslexie",
      description:
        "Conseils concrets et études pour lire et écrire plus facilement avec une dyslexie : synthèse vocale, dictée, polices, documents scannés.",
    },
    surtitre: "Blog",
    titre: "Lire et écrire, sans s'épuiser",
    texte: "Des conseils concrets, et ce que disent les études. Écrits court, pour être lus — ou écoutés.",
    lire: "Lire l'article",
    minutes: "min de lecture",
    publie: "Publié le",
    accueil: "Accueil",
    autres: "À lire aussi",
    retour: "Tous les articles",
    flux: "Flux RSS",
    final: {
      titre: "Ce texte, vous auriez pu l'écouter.",
      texte: "Dysflow lit à voix haute et surligne chaque mot. Gratuit, sur votre ordinateur.",
    },
  },
} as const;

/** Ce que doit suivre chaque langue : la forme de `fr`, avec des `string`. */
type ProfondeurChaine<T> = T extends string
  ? string
  : { [K in keyof T]: ProfondeurChaine<T[K]> };

export type Dico = ProfondeurChaine<typeof fr>;
