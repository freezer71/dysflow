# L'anglais dans sillage — site, app macOS et terminal

Date : 19 oct. 2026

## Pourquoi

sillage est public (GitHub, MIT) et son site est en ligne. Aujourd'hui tout
est en français : un visiteur qui arrive depuis GitHub sans lire le français
ne comprend ni le site ni l'app. L'anglais devient la langue par défaut ; le
français reste complet pour qui l'utilise.

## Objectifs

- **Site** : anglais par défaut, français complet en second.
- **App macOS** : toute l'interface en anglais et en français.
- **Terminal (Linux, Rust)** : toute l'interface en anglais et en français.
- Une traduction anglaise naturelle, pas mot à mot : même ton court et
  concret que le français.

## Non-objectifs

- D'autres langues que l'anglais et le français.
- La langue de **lecture** des documents (détection, voix, chiffres, sigles,
  phonèmes) : elle ne change pas. Seule la langue de l'**interface** change.
- Le README et les documents de `docs/` (décision du constructeur : hors de
  ce lot, à proposer ensuite).
- Les journaux (`log`, `Logger`) et les messages des tests : ils restent en
  français.

## Anti-objectifs (échec même si « ça marche »)

- Un utilisateur francophone qui perd du texte français, ou voit un mélange
  des deux langues sur un même écran.
- Une réécriture dans `proxy.ts` sur le site (voir les règles du
  `CLAUDE.md` global : boucles de préchargement).
- Des liens existants vers le site qui cassent (`/modeles`, `/telecharger`,
  `/install.sh`).

## Choix arrêtés

### Quelle langue s'affiche

| Où | Règle | Choisi par |
|----|-------|-----------|
| App macOS | Langue du Mac : français si le Mac (ou le réglage par app de macOS) est en français, anglais sinon | l'utilisateur |
| Terminal | `LC_ALL`, puis `LC_MESSAGES`, puis `LANG` : français s'il commence par `fr`, anglais sinon | décision du constructeur, même logique que le Mac |
| Site | L'adresse décide : `/` en anglais, `/fr` en français. Pas de redirection automatique selon le navigateur | décision du constructeur |

### Site (Next.js 16, `site/`)

Adresses :

| Anglais | Français |
|---------|----------|
| `/` | `/fr` |
| `/models` | `/fr/modeles` |
| `/download` | `/fr/telecharger` |

- Une vraie route par adresse publique ; le code de chaque page est partagé
  dans un dossier privé (`app/_pages/…`) et reçoit la langue en paramètre.
  Pas de `[lang]` dynamique, pas de réécriture.
- Deux mises en page racines (groupes de routes), pour un `<html lang="en">`
  et un `<html lang="fr">` justes.
- Textes : un dictionnaire par langue (`app/_i18n/en.ts`, `fr.ts`), typé
  pour qu'une clé manquante ne compile pas. Les données des modèles
  (`_data/modeles.ts`) portent leurs deux textes.
- Un sélecteur EN / FR dans l'en-tête, qui mène à la même page dans l'autre
  langue.
- `hreflang` (alternates) et métadonnées traduites sur chaque page.
- Anciennes adresses : `/modeles` → `/fr/modeles` et `/telecharger` →
  `/fr/telecharger`, redirections permanentes dans `next.config.ts`.
  `/install.sh` ne bouge pas.
- Vérification après le changement : compter les requêtes `?_rsc=` par lien
  sur 10 s depuis chaque page (règle du `CLAUDE.md` global).

### App macOS (`macos/`)

- Un catalogue de textes Xcode (`Localizable.xcstrings`), langue de
  développement anglaise, traduction française. Les textes du code passent
  en anglais ; le français part dans le catalogue.
- `project.yml` : langue de développement `en`, langues connues `en` et `fr`.
- Les textes d'autorisation (micro, reconnaissance vocale) sont traduits
  aussi (`InfoPlist.xcstrings`).
- Les textes qui viennent du moteur Rust (catalogue des modèles, erreurs)
  arrivent déjà traduits : l'app passe sa langue au moteur (voir plus bas).
- Les tests (unitaires et d'interface) qui cherchent un texte français
  doivent marcher : ils fixent la langue ou cherchent l'identifiant
  d'accessibilité, pas le texte.

### Moteur et terminal (Rust)

- Une langue d'interface (`UiLang::En` / `UiLang::Fr`) choisie une fois au
  démarrage : par l'environnement pour le terminal, par l'app via la FFI
  pour le Mac.
- Les textes d'interface passent par une fonction ou une table par langue,
  sans bibliothèque lourde (décision du constructeur ; une table `match`
  suffit à deux langues).
- Concerné : l'interface du terminal (`src/tui`, `src/app`), le catalogue
  des modèles (`src/models.rs` : descriptions, langues, licences, allures),
  les messages d'erreur et d'avis montrés à l'utilisateur, l'aide de la
  ligne de commande.
- Pas concerné : tout le traitement du texte lu (`src/text`, phonèmes,
  nombres, sigles), qui dépend de la langue du document.

### Script d'installation (`install.sh`)

Décision du constructeur : il passe aussi en anglais, sinon un anglophone
voit du français dès l'installation et pendant `sillage --update`.

- Même règle de langue que le terminal (`LC_ALL`, `LC_MESSAGES`, `LANG`),
  en `sh` POSIX (le script tourne sous `curl | sh`).
- `--uninstall` devient l'option affichée ; `--desinstaller` reste un alias
  pour ne pas casser les commandes déjà copiées.
- Le site montre `--uninstall` (`commandeDesinstallation` dans `liens.ts`).

### Mise en ligne

Choix de l'utilisateur : **tout ensemble**. Le site anglais part en même
temps qu'une nouvelle version (tag `v*`) dont l'app et le terminal sont
déjà en anglais. D'ici là, le travail du site reste sur sa branche (ou
fusionné sans déploiement) ; `install.sh` servi en production depuis `main`
ne change pas avant la sortie.

### Traduction

Choix de l'utilisateur : la traduction du constructeur est définitive, sans
étape de relecture. L'utilisateur corrigera ensuite ce qui le gêne.

## Laissé au constructeur

- L'ordre des lots (proposition : site, puis moteur Rust et terminal, puis
  app macOS, qui dépend du moteur).
- Construire l'aide de `clap` à l'exécution (aujourd'hui en commentaires
  de doc, donc figée en français).
- Vérifier côté Swift qu'aucun code ne compare un texte d'erreur venu de
  Rust comme une chaîne.
- Le découpage exact des clés et des fichiers de traduction.
- La façon dont la FFI reçoit la langue (au démarrage du lecteur ou par
  appel).
- Tester sur la CI, jamais `xcodebuild test` en local (l'app s'ouvre et
  bloque le Mac).
