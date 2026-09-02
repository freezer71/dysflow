# Lire des PDF et des images

Conception validée le 2026-08-30.

## Pourquoi

Le lecteur n'accepte aujourd'hui que du texte. Or une partie des
spécifications arrive en PDF, et certaines pages ne sont accessibles qu'en
capture d'écran. Les ouvrir ailleurs, c'est perdre le suivi de lecture que ce
lecteur existe pour donner.

## Ce qui n'est pas le problème

L'extraction est trois appels de bibliothèque. Le travail est la
**reconstruction**.

Le lecteur attend du markdown structuré : les titres font le sommaire (`t`) et
la navigation `n`/`p`, les blocs font `↑`/`↓`, les phrases font l'unité de
synthèse. Un PDF ne donne que du texte mis en page. Injecté tel quel :

| Ce qu'on obtient | Ce que ça coûte |
|---|---|
| Lignes coupées à 70 colonnes | Chaque ligne devient un bloc, `↑`/`↓` inutilisable |
| Césures `exemp-` / `le` | La voix bafouille en plein mot |
| En-têtes répétés | « Confidentiel — page 12 » toutes les 30 secondes |
| Aucun titre | Pas de sommaire, donc on perd le fil |

## Moteurs retenus, et pourquoi

Mesuré au mot près sur une page française identique, avant de décider :

| | Tesseract | Vision (macOS) | `ocrs` (pur Rust) | `pdf-extract` |
|---|---|---|---|---|
| Exactitude | **100 %** | 97,6 % | 64,5 % | 100 % |
| Divergences | aucune | « puis » perdu | 14 | aucune |
| Accents | corrects | corrects | détruits — `Sp?cification` | corrects |
| `« »` et `€` | corrects | corrects | perdus → `(E)` | corrects |
| Ordre des lignes | correct | à retrier soi-même | correct | — |
| Durée | 0,95 s | ~1 s | 2,7 s | instantané |
| Portable | macOS + Linux | macOS seul | partout | partout |

Décisions :

- **PDF à couche texte → `pdf-extract`.** Pur Rust, restitution fidèle, léger.
  Reste dans la compilation par défaut.
- **OCR → Tesseract, seul, partout.** Le moteur portable est aussi le plus
  exact. Vision est écarté : il coûte une dépendance macOS et un tri de lignes
  manuel pour rendre 2,4 points de moins. `ocrs` est écarté : un moteur qui
  détruit les accents ne peut pas alimenter une voix française.

Une seule implémentation, donc, et pas de trait à deux têtes : le code reste
plus court et se comporte pareil sur les trois cibles.

### Ce que coûte Tesseract

| | Mesuré |
|---|---|
| Compilation de Tesseract 5.5.2 + Leptonica 1.87 | 59 s, une fois |
| `fra.traineddata` (tessdata_fast) | 1,1 Mo |
| Poids ajouté au binaire, mesuré en release strippé | ~4 Mo |

Le binaire distribué passerait donc d'environ 3,9 Mo à une dizaine de Mo, avec
les deux langues embarquées. Acceptable pour un téléchargement unique.

`tesseract-rs` télécharge ses sources depuis GitHub au moment du build, sauf
si `third_party/` les contient déjà. Un build de distribution ne peut pas
dépendre de GitHub : les sources sont **vendorées**, comme `vendor/audio.cpp`
l'est déjà.

Les données de langue, elles, sont **embarquées dans le binaire**
(`include_bytes!`) : 1,1 Mo pour le français, autant pour l'anglais. Rien à
télécharger, rien à installer, l'OCR marche à la première ouverture. Les autres
langues restent ajoutables via le gestionnaire de modèles existant.

Enfin, l'OCR est une **fonctionnalité optionnelle** (`--features ocr`), comme
`audio-cpp`. La promesse du projet — « par défaut le binaire se construit en
secondes » — est préservée ; les binaires distribués l'activent.

## Architecture

Une couche en amont de tout le reste :

```text
pdf, image, dossier ──▶ source ──▶ input ──▶ doc ──▶ tts ──▶ audio ──▶ tui
                     (extraire,    (blocs)
                      recomposer)
```

| Fichier | Rôle |
|---|---|
| `source/mod.rs` | Détection du type, routage, API publique |
| `source/pdf.rs` | Couche texte page par page ; page vide → OCR |
| `source/ocr.rs` | Image → lignes, via Tesseract |
| `source/layout.rs` | **La reconstruction.** Pur, sans PDF ni OCR |
| `source/clip.rs` | Presse-papiers via `arboard` |
| `source/cache.rs` | Mémorisation du markdown produit |

Une page est réputée scannée quand sa couche texte rend moins de 30 caractères
imprimables. En dessous, il s'agit d'un artefact — un numéro de page isolé, une
légende — et non d'un contenu lisible : la page part à l'OCR.

### API

```rust
pub enum Source {
    /// Texte disponible immédiatement : sommaire complet dès l'ouverture.
    Ready(String),
    /// Texte produit page par page : la lecture démarre avant la fin.
    Streaming(Receiver<InputEvent>),
}

pub fn open(path: &Path) -> Result<Source>;
```

`main.rs` remplace `read_to_string` par `source::open`. `Ready` suit le chemin
actuel ; `Streaming` se branche exactement comme stdin. **Aucune mécanique
nouvelle dans `App`.**

L'énumération `Source` n'apparaît qu'au lot D, avec le premier producteur de
flux. Jusque-là, le lot A expose la forme réduite qui suffit — pas de variante
construite nulle part, pas de code mort :

```rust
pub fn load(path: &Path) -> Result<String>;
```

### Détection du type

Les octets d'en-tête priment sur l'extension, qui ment souvent :

| Octets | Type |
|---|---|
| `%PDF-` | PDF |
| `\x89PNG`, `\xFF\xD8\xFF`, `II*\0`, `MM\0*`, `RIFF…WEBP`, `ftypheic` | Image |
| Rien de tout ça | Texte — comportement actuel inchangé |

Un dossier est lu comme une suite d'images triées en ordre naturel, pour que
`page2` précède `page10`.

## La reconstruction

Signature pure, donc entièrement testable sans fichier binaire :

```rust
pub struct Page { pub lines: Vec<String> }
pub fn to_markdown(pages: &[Page]) -> String;
```

Règles, chacune un test :

1. **Césures.** `exemp-\nle` est recollé. `arc-\nen-ciel` garde son trait : on
   ne recolle que si la suite commence en minuscule et que le préfixe n'est pas
   un mot autonome.
2. **Paragraphes.** Les lignes se rejoignent. Coupure si la précédente est
   courte — moins de 60 % de la largeur médiane — et finit par une ponctuation
   forte, ou si l'indentation change.
3. **En-têtes et pieds.** Une ligne présente en première ou dernière position
   sur au moins trois pages, à un chiffre près, disparaît.
4. **Titres.** Ligne isolée, courte, sans point final, numérotée
   (`3.2 Contraintes`) ou capitalisée → `#`, `##`, `###` selon la profondeur de
   la numérotation. C'est cette règle qui rend le sommaire.
5. **Listes.** `•`, `–`, `*`, `1.` en tête → liste markdown.
6. **Saut de page.** Jamais de blanc au milieu d'une phrase : la dernière ligne
   d'une page se recolle à la première de la suivante si la phrase est ouverte.
7. **Ligatures.** `ﬁ` → `fi`, guillemets typographiques normalisés, en
   s'appuyant sur `text::sanitizer` existant.

## L'attente

| Entrée | Comportement |
|---|---|
| PDF à couche texte | Instantané, `Ready`, sommaire complet d'emblée |
| Image seule | Une seconde, `Ready` |
| PDF scanné, dossier | `Streaming` : lecture dès la page 1, OCR derrière |

Une page scannée coûte de l'ordre de la seconde. Attendre 200 pages avant le
premier mot serait inacceptable ; le flux existant règle le problème sans code
nouveau.

## Cache

Relire une spec sur plusieurs jours ne doit pas refaire l'OCR. Le markdown
reconstruit est mémorisé dans `dirs::cache_dir()/sillon/`, sous une clé
dérivée de la taille, de la date de modification et des premiers kilo-octets du
fichier. Aucune dépendance nouvelle : `DefaultHasher` de la bibliothèque
standard suffit, il ne s'agit pas de cryptographie.

Effet de bord utile : le markdown devient exportable.

```sh
sillon spec.pdf --to-markdown > spec.md
```

## Données de langue

Aucun téléchargement. `fra.traineddata` et `eng.traineddata` sont embarqués
dans le binaire, écrits dans un répertoire de cache à la première utilisation
puisque Tesseract exige un chemin sur disque.

D'autres langues restent installables par le gestionnaire de modèles existant.
`ModelPackage` suppose aujourd'hui une adresse HuggingFace ; il lui faudra un
champ d'adresse directe pour pointer vers `tessdata_fast`.

## Interface

Ligne de commande :

| Commande | Effet |
|---|---|
| `sillon spec.pdf` | Inchangé dans l'usage |
| `sillon capture.png` | OCR puis lecture |
| `sillon scans/` | Dossier d'images, en ordre naturel |
| `sillon --clip` | Presse-papiers, image ou texte |
| `sillon spec.pdf --to-markdown` | Écrit le markdown et quitte |
| `--no-cache` | Refait l'extraction |

Configuration : une sixième rubrique **Documents**, pour ne pas mêler ces
réglages à la voix.

- Langue d'OCR, par défaut celle de la voix
- Suppression des en-têtes répétés
- Cache actif

Deux ajustements suivent : la touche d'accès directe passe de `1..=5` à
`1..=6` dans `keymap.rs`, et le résumé de rubrique reste sous 22 caractères.

## Erreurs

Chaque échec dit quoi faire, jamais seulement qu'il a échoué :

| Cas | Message |
|---|---|
| PDF chiffré | Le document est protégé, aucun texte extractible |
| Page sans texte, binaire compilé sans `ocr` | Dit que la lecture des scans exige cette option, et laisse lire le reste |
| Image illisible | Format non reconnu, avec la liste de ceux qui le sont |
| Rien d'extrait | Réutilise « aucun contenu lisible dans le fichier » |

## Tests

| Cible | Nature |
|---|---|
| `layout.rs` | Une vingtaine de tests purs — l'essentiel de l'effort |
| Détection de type | Tests sur octets d'en-tête, sans fichier |
| `pdf.rs` | Fixture PDF minimale : extraction et découpage en pages |
| `ocr.rs` | Fixture image française : **les accents doivent survivre**. Le test de non-régression qui compte, et il tourne sur les trois cibles |
| `cache.rs` | Écriture puis relecture |
| Bout en bout | `sillon fixture.pdf --to-markdown` |

## Lots

| Lot | Contenu | Gain |
|---|---|---|
| A | Socle `source`, PDF à couche texte, `--to-markdown` | Les specs exportées, tout de suite |
| B | Reconstruction complète | La qualité de lecture |
| C | OCR : image, dossier, presse-papiers | Capture d'écran lisible |
| D | PDF scanné en flux, cache | Les photocopies |

## Point ouvert

`pdf_extract::extract_text` rend le document d'un bloc. Le découpage par page
est nécessaire aux règles 3 et 6 et au flux. Deux voies : l'API bas niveau
`output_doc` avec un collecteur par page, ou le saut de page `\u{c}` inséré
entre les pages. Tranché en TDD au lot A, la première étant préférée.

## Portabilité et distribution

Cibles retenues : **macOS** (Intel et Apple Silicon), **Linux x86_64**,
**Linux ARM64**. Windows est hors périmètre pour le moment.

Ces trois cibles étant toutes Unix, les points `cfg(unix)` du projet —
`logging.rs`, `input.rs`, `models.rs` — restent valides tels quels. Rien à
doubler.

Deux dettes préexistantes, indépendantes des PDF, resteront à traiter dans un
second chantier :

| Point | État |
|---|---|
| TTS de repli | `say` sur macOS, `espeak-ng` sur Linux — couvert |
| Détection d'outil par `which` | Unix, couvert |
| Shim `audio.cpp` lié par `rpath` absolu vers `target/` | **Casse dès que le binaire est déplacé** — bloquant pour distribuer |

Contrainte de conception qui vaut dès maintenant : **aucun `cfg(target_os)`
nouveau**. Ce qui est écrit ici doit se comporter identiquement sur les trois
cibles, ce que le choix d'un moteur OCR unique rend possible.

## Hors périmètre

Colonnes multiples, formules mathématiques, reconnaissance de l'écriture
manuscrite, PDF de plus de mille pages, Windows.

Deux limites de format, à annoncer clairement plutôt qu'à échouer en silence :

| Cas | Raison |
|---|---|
| **HEIC** — photos d'iPhone | Aucun décodeur pur Rust fiable. Message conseillant de convertir en PNG |
| **PDF scanné en vectoriel** | Les images de page sont extraites par `lopdf` ; un scan sans image embarquée sortirait vide. MuPDF le ferait, mais sa licence AGPL est incompatible avec le MIT du projet |

Formats d'image lus : PNG, JPEG, TIFF, WebP, BMP.
