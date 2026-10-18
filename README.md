<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/logo/sillon-mark-dark.svg">
  <img src="assets/logo/sillon-mark.svg" alt="" width="76">
</picture>

# sillon

Lecteur audio en terminal : il prend du markdown — pipé ou en fichier —, le
hiérarchise, et le lit à voix haute avec les contrôles d'un lecteur audio.

```bash
sillon                                      # dit comment lire, puis rend la main
cat notes.md | sillon
sillon rapport.md --speed 1.5
llm "explique-moi les monades" | sillon     # la lecture démarre avant la fin du flux
sillon --config                             # les réglages
```

**Lancé sans argument, `sillon` affiche les quatre manières de lire** — un
fichier, un tube, `--clip`, l'app — et rend la main. `s` ouvre les réglages
en cours de lecture, `?` l'aide.

## Ce qu'il fait

- **Lit un flux au fil de l'eau.** Le texte est mis en voix bloc par bloc ; il
  n'attend ni la fin de l'entrée, ni la fin de la synthèse.
- **Accepte un PDF** qui porte une couche texte, et le reconstruit : titres
  rendus au sommaire, mots coupés recollés, en-têtes et pieds de page retirés.
  `--to-markdown` écrit le texte extrait sans lancer la lecture.
  Une page qui n'est qu'une image — une couverture photographiée — ne bloque
  pas le reste : sans l'option `ocr`, elle le dit et la lecture continue.
- **Lit aussi les images**, avec reconnaissance de texte : une capture d'écran,
  une photo de page, un dossier de scans, un **PDF photocopié**, ou une image
  collée. Les pages sont reconnues en parallèle et le résultat mémorisé : la
  deuxième ouverture est immédiate. `--no-cache` refait l'extraction. Demande
  l'option de compilation `ocr`.
- **Lit le presse-papiers** avec `--clip`, sans rien exiger de plus : coller du
  texte n'a rien à reconnaître. Seule une image collée demande l'option `ocr`.
- **Synthétise en parallèle**, sur plusieurs sessions, dans une fenêtre glissante
  autour du curseur — ce qui rend l'avance et le recul immédiats et borne la
  mémoire.
- **Montre ce qui est lu** : mot surligné, ligne soulignée, phrase en cours,
  bloc actif, titre courant, et la ponctuation prononcée.
- **Se lit en même temps qu'il se dit.** Interlignage, règle de lecture,
  largeur de colonne réglable : l'écran est fait pour être suivi des yeux
  pendant que la voix avance.
- **Montre ce qu'il ne lit pas.** Un bloc de code est annoncé sans être récité,
  mais il reste affiché tel qu'il est écrit ; un tableau garde ses colonnes.
- **Ne lit pas le formatage.** Bordures de tableaux, filets, flèches, marqueurs
  de titres et syntaxe markdown sont retirés ; la hiérarchie passe par la
  typographie et par la durée des silences, jamais par un « dièse titre ».
- **Rouvre un document là où on l'a laissé.** La position se retient toute
  seule, et se retrouve même si le document a été retouché entre-temps.
- **Garde la page du fichier.** Les lignes, l'indentation et l'alignement des
  colonnes sont ceux du document : ce qui est à l'écran se reconnaît.

## Installation

**Sous Linux (x86_64, ARM64)**, le paquet tout prêt est joint à chaque
publication GitHub (`sillon-<version>-linux-<arch>.tar.gz`, glibc 2.39 ou
plus) ; la marche à suivre est dans les notes de la publication. Un tag `v*`
le construit et le publie (`.github/workflows/release.yml`).

Depuis les sources :

```bash
./scripts/install.sh            # Kokoro + reconnaissance de texte, dans ~/.local
cargo install --path .          # voix du système seule, build en quelques secondes
```

`install.sh` assemble le paquet (`package.sh`, ci-dessous) et le copie en
entier dans `$PREFIX/lib/sillon` — le binaire, ONNX Runtime, sillon-espeak et
ses données, les licences —, puis pose `$PREFIX/bin/sillon`, un lien vers
`../lib/sillon/sillon` : le binaire suit ce lien pour trouver ses voisins.
`~/.local` par défaut, sans droits ; `PREFIX=/usr/local sudo -E
./scripts/install.sh` pour toute la machine. Il vérifie ensuite, depuis la
racine, que `sillon` démarre et que Kokoro se charge (`sillon --check-kokoro` :
ONNX Runtime et sillon-espeak, sans le modèle), et dit s'il manque le `bin/` au
`PATH`. Modèles et configuration vivent dans le répertoire de données. Pour
désinstaller, `rm -r $PREFIX/lib/sillon $PREFIX/bin/sillon`.

Sur Mac, avec l'app dans `/Applications`, `./scripts/install-app-command.sh`
pose à la place une commande `sillon` qui ouvre l'app : `sillon document.pdf`
ouvre le panneau sur le document et le lit (reprise à la dernière position),
`sillon` seul ouvre le lecteur. Le lecteur au terminal n'est alors plus
la commande `sillon` (`$PREFIX/lib/sillon` reste, à retirer à la main) ; pour
désinstaller la commande de l'app, `rm $PREFIX/bin/sillon`. Chacun des deux
scripts prévient avant de remplacer l'autre, et ne le fait qu'après
confirmation (ou avec `REMPLACER=1`).

### Distribuer le binaire

`scripts/package.sh` assemble dans `dist/` le binaire et ce qu'il charge —
ONNX Runtime et sillon-espeak pour Kokoro —, avec les licences, et vérifie que
l'ensemble s'exécute hors de l'arbre de compilation. ONNX Runtime 1.31.0 est
téléchargé depuis sa publication officielle, empreinte SHA-256 vérifiée, pour
Linux x86_64 et ARM64 et pour macOS sur Apple Silicon :

```bash
./scripts/package.sh                 # ocr + kokoro-onnx par défaut
FEATURES=kokoro-onnx ./scripts/package.sh
```

Pour la **reconnaissance de texte** — images, dossiers de scans, images
collées —, Tesseract est compilé dans le binaire. Comptez une minute au
premier build ; ensuite rien n'est à installer, les données de langue
(français et anglais) sont embarquées :

```bash
cargo build --release --features ocr
```

### La voix

**Kokoro s'installe tout seul** au premier usage, avec un binaire construit
avec `kokoro-onnx` (`./scripts/install.sh` le fait) : le téléchargement
démarre au lancement, la voix du système lit en attendant, et la ligne d'état
montre la progression (`⬇ voix  Installation de la voix Kokoro 82M · int8 :
34,0 Mo / 92,4 Mo`). Le téléchargement fini, Kokoro prend la suite à la phrase
en cours, et le choix s'écrit dans `config.toml`. Un échec — réseau coupé —
se dit dans la ligne d'état ; le lancement suivant réessaie, en reprenant ce
qui était déjà reçu. Rien ne se télécharge avec `--engine system`, ni avec un
binaire sans `kokoro-onnx`.

| Modèle | Taille | Langues | Voix |
|---|---|---|---|
| Kokoro 82M · int8 (terminal, feature `kokoro-onnx`) | 108 Mo | français, anglais | `p-fenrir60` · `af_bella`, `am_fenrir`, `af_heart` |
| Kokoro 82M · MLX (app macOS seulement) | 342 Mo | français, anglais | idem |

Le catalogue ne propose **que Kokoro** : une voix française (`p-fenrir60`,
livrée avec Sillon) et trois anglaises (`af_bella`, recommandée, `am_fenrir`,
`af_heart`). Sans lui, la voix du système lit. Supertonic 3 et MagpieTTS en
sont sortis le 14 octobre 2026, et leur moteur, audio.cpp, avec eux : une
configuration qui les nomme encore fait lire la voix du système, et le dit.
Le correctif local d'audio.cpp (lecture des GGUF en bloc) est gardé dans
`patches/`.

Les modèles vont dans `~/.local/share/sillon/models/<famille>/` (sur macOS,
`~/Library/Application Support/sillon/models/`). Un modèle déjà installé est
adopté automatiquement au lancement suivant.

Le modèle se charge **en arrière-plan** : l'interface s'ouvre en 0,05 s, le
document s'analyse pendant que les poids se chargent, et la barre de lecture
annonce l'étape en cours — `⠹ chargement du modèle kokoro… 0,2 s`, puis
`synthèse de la première phrase…` — jusqu'à ce que le son démarre, où elle
redevient la barre de progression.

Le budget jusqu'au premier son se mesure :

```bash
cargo test --release --features kokoro-onnx --test startup_timing -- --ignored --nocapture
```

**Le nombre de sessions suit la famille.** Kokoro calcule en série : une
session, *Sessions simultanées* valant `auto` par défaut. Un nombre posé à la
main l'emporte toujours — y compris celui qu'une version antérieure a pu
écrire dans `config.toml` : retirez la ligne `sessions` (ou mettez `0`) pour
en profiter.

### Ce qui est dit au moteur

Un modèle de voix devine mal la prononciation d'un signe : « 1 » lui parvient
comme un caractère, pas comme un nombre. Le lecteur écrit donc en toutes
lettres, avant la synthèse, tout ce qui se lit autrement qu'il ne s'écrit :

| Écrit | Dit |
|---|---|
| `2026-08-29`, `26/08/2029` | vingt-neuf août deux mille vingt-six |
| `lot 1`, `page 47` | lot un, page quarante-sept |
| `1 234 567`, `8080` | un million deux cent trente-quatre mille cinq cent soixante-sept, huit mille quatre-vingts |
| `06 12 34 56 78` | zéro six, douze, trente-quatre, cinquante-six, soixante-dix-huit |
| `984 733 204 00015` | neuf huit quatre, sept trois trois, deux zéro quatre, zéro zéro zéro un cinq |
| `00015` | zéro zéro zéro un cinq |
| `14h30` | quatorze heures trente |
| `3,14`, `100 %` | trois virgule quatorze, cent pour cent |
| `le 1er`, `la 21ème` | le premier, la vingt et unième |
| `-5 degrés` | moins cinq degrés |
| `55 €`, `1 €`, `30 km`, `25 °C` | cinquante-cinq euros, un euro, trente kilomètres, vingt-cinq degrés |
| `### 3.5 Tarif de repli` | trois point cinq, Tarif de repli |
| `HT`, `TTC`, `TVA`, `RCS` | hache té, té té cé, té vé a, erre cé esse |
| `SIRET`, `SIREN`, `CNIL` | sirette, sirène, knil |

Une date se dit toujours à la française — le jour, le mois en toutes lettres,
l'année entière — quel que soit l'ordre sous lequel elle est écrite : `2026-08-29`,
`29/08/2026` et `08/29/2026` se disent tous « vingt-neuf août deux mille
vingt-six ».

La numérotation d'un titre se dit ; celle d'un item de liste se retire, sa
structure étant déjà portée par le rang de l'item.

Cette mise en mots ne concerne que la voix : **l'écran garde la colonne
« Écrit »**. On entend « deux mille vingt-sept » en lisant `2027` — c'est le
document qu'on suit des yeux, pas sa transcription. Le curseur reste calé pour
autant : un nombre pèse, dans le surlignage, le temps qu'il faut pour le dire.

Ce qui n'est pas une quantité reste intact : `version 2.1`, `M5`, `F1`, `v3`,
`3/4`. Un numéro suit la forme sous laquelle il est écrit — groupé par milliers,
il se dit comme une quantité ; en dix chiffres derrière un `0` ou un `+33`, il
se dit par paires.

Un **identifiant se dit chiffre par chiffre**, en respirant à chaque groupe
écrit : c'est en l'écoutant qu'on le recopie, et « neuf cent quatre-vingt-quatre
milliards » ne se note pas. Deux indices le trahissent, la forme et le libellé :
un groupement qui n'est pas celui des milliers (`984 733 204 00015`) ou un zéro
de tête (`00015`) suffisent ; neuf chiffres groupés par trois ne se distinguant
pas d'un nombre de millions, c’est alors le libellé qui tranche — `SIRET`,
`SIREN`, `RCS`, `TVA`, `intracommunautaire`, `IBAN`, `BIC`, quand il annonce
directement le nombre. Un montant cité plus loin dans la même phrase reste une
quantité.

### Les sigles

Un sigle ne se lit pas comme il s'écrit, et le moteur n'a pas de lexique pour
en décider : `HT` lui parvient comme deux consonnes qu'aucune bouche française
n'enchaîne, `SIRET` comme un mot dont le T final tomberait. Le lecteur tranche
donc avant lui, **pour la voix seule** — l'écran garde le sigle.

La règle est simple : ce qui ne se laisse pas dire s'épelle. Un mot en
capitales sans voyelle (`HT`, `TTC`, `RCS`, `RGPD`) ou dont les consonnes
n'ouvrent aucune syllabe française (`TVA`, `CDI`, `SNCF`, `PME`) est rendu avec
les noms des lettres. Le reste est laissé au moteur, qui le dira comme un mot :
`RIB`, `NAF`, `SARL`, `IBAN`, `SMIC`.

Restent ceux que seul l'usage décide, et qu'un dictionnaire couvre :
`SIRET` → sirette, `SIREN` → sirène, `CNIL` → knil, `URSSAF`, `INSEE`, `SEPA`,
`SAS`, `KBIS` d'un côté ; `SA`, `API`, `IA`, `SAV`, `IP`, `UE`, `URL`, `ONG`,
`APE`, `RATP` de l'autre, qui s'épellent malgré une graphie prononçable.

Deux garde-fous : un mot d'une seule lettre n'est jamais épelé — `A` est aussi
bien un article qu'un rang — et un mot de plus de six capitales non plus, un
titre en capitales étant fait de mots et non de sigles. `MENTIONS LÉGALES` se
lit donc comme il s'écrit.

### Choisir une voix

Dans les réglages (`s`), lignes *Voix française* et *Voix anglaise* : `←` `→`
parcourent les voix de la langue, `t` fait écouter celle de la ligne. Le choix
s'enregistre aussitôt.

Kokoro a une voix par langue de document : `p-fenrir60` en français (la seule),
`af_bella`, `am_fenrir` ou `af_heart` en anglais. Une voix écrite à la main qui
n'est plus proposée parle avec la recommandée de sa langue, sans que le fichier
soit réécrit.

Sans modèle installé — ou sans la feature — le lecteur se replie sur la voix du
système (`say` sur macOS, `espeak-ng` ailleurs). La ligne d'état montre alors l'installation
en cours, ou pourquoi elle a échoué.

## Contrôles

| Touche | Effet |
|---|---|
| `Espace` | Lecture / pause |
| `←` `→` | Phrase précédente / suivante |
| `↑` `↓` | Bloc précédent / suivant |
| `n` `p` | Titre suivant / précédent |
| `Début` | Début du document |
| `/` | Chercher dans le document |
| `N` · `P` | Résultat suivant · précédent |
| `+` `-` | Vitesse (enregistrée) |
| `t` | Sommaire — `↑` `↓` puis `Entrée` pour y aller |
| `s` | Réglages |
| `Pg↑` `Pg↓` | Relire sans déplacer la lecture |
| `f` | Reprendre le suivi |
| `?` | Aide : toutes ces touches |
| `Échap` | Ferme le sommaire ; sinon quitte |
| `q` | Quitter |

La vitesse agit **sur le son, à la lecture**, sans changer la hauteur : la voix
accélère sans monter dans les aigus. La synthèse se fait toujours à 1×, et
[Sonic](https://github.com/waywardgeek/sonic) étire le son pendant qu'il sonne.
Changer de vitesse s'entend en quelques millisecondes, au milieu de la phrase,
sans resynthèse ni coupure, et avec toutes les voix. Le modèle ne reçoit jamais
la vitesse : la lui donner lui faisait avaler des mots à 2× et plus. Les pauses
entre blocs raccourcissent moins vite que la parole (÷ √vitesse).


## Lire en écoutant

Le lecteur est fait pour être suivi des yeux, pas seulement entendu. Trois
réglages, dans les réglages (`s`), gouvernent ce que l'écran demande à
l'œil :

| Réglage | Défaut | Ce qu'il change |
|---|---|---|
| Largeur de colonne | plein écran | Bornée, la colonne se cale au centre : l'œil retrouve mieux le début de la ligne suivante, au prix de marges vides. Réglable de 40 à 120 caractères. |
| Interlignage | une ligne | Une ligne vide entre deux lignes de texte, la recommandation courante pour la lecture accompagnée. |
| Règle de lecture | oui | La ligne où se trouve le mot prononcé porte un fond discret. Le mot, lui, garde son surlignage plein. |

La règle et le surlignage ne disent pas la même chose : le mot dit **où en est
la voix**, la ligne dit **où revenir** quand on relève les yeux.

**Le formatage du fichier se voit.** Le gras est du gras, l'italique de
l'italique, le barré du barré ; un lien et un fragment de code prennent une
teinte froide, loin de l'accent réservé au curseur de lecture. Rien de tout
cela ne se prononce — la voix en dirait le marqueur — et rien n'en porte de
fond : le mot prononcé reste le seul de l'écran à en avoir un. Hors du bloc
lu, la couleur s'efface avec le reste ; la graisse et le soulignement, eux,
restent lisibles.

La phrase en cours s'éclaircit désormais sans s'épaissir : la graisse dit le
gras du document, et la phrase lue se reconnaît à sa clarté, à la règle qui la
souligne et au mot que la voix y tient.

**La page reste celle du fichier.** L'écran suit la composition du document —
mêmes lignes, même indentation, colonnes alignées — et n'en retire que le
balisage, que la typographie du terminal rend autrement. Un document plié à
quatre-vingts colonnes, lu dans une colonne plus étroite, ferait exception :
suivre ses lignes y laisserait un mot orphelin sous chacune, et le bloc est
alors recomposé d'un seul tenant. Ce qui prolonge une ligne pend alors sous son
texte et non sous sa puce, et le filet d'une citation tient toute sa hauteur :
une ligne de suite ne se confond jamais avec une ligne neuve.

**Ce qui n'est pas lu reste montré.** Un bloc de code annoncé — le réglage par
défaut — se dit « bloc de code rust » et s'affiche pourtant en entier, avec son
indentation ; une ligne trop longue est repliée sous un filet `╎`, jamais
tronquée. Un tableau garde ses colonnes alignées à l'écran, un filet
léger entre elles et sous son en-tête, et se dit colonne par colonne :

```
    Route       │ Méthode │ Auth         « Route : slash users.
    ────────────┼─────────┼──────────      Méthode : GET.
    /users      │ GET     │ JWT            Auth : JWT. »
    /users/{id} │ DELETE  │ JWT admin
```

Sans cet appariement, une ligne de tableau se disait « slash users, GET, JWT » :
trois valeurs sans leur colonne. Chaque cellule est par ailleurs une phrase,
donc `←` `→` parcourent le tableau cellule par cellule.

**Reprendre où l'on s'était arrêté.** Rouvrir un fichier déjà lu remet la voix
sur sa dernière phrase, et le dit en bas de l'écran ; `Début` ramène au début. Ce
qui est retenu n'est pas seulement un rang, c'est la phrase elle-même : un
paragraphe ajouté en tête décale tout, et la reprise tomberait au milieu d'un
autre passage. Un document lu jusqu'au bout oublie sa position — le rouvrir le
reprend au début. Un flux et un collage n'en gardent aucune : ils ne sont pas
les mêmes d'une fois sur l'autre.

**Défiler sans perdre la voix.** `Pg↑` et `Pg↓` déplacent la vue seule : la
lecture continue là où elle en était, et la barre d'état affiche `⏸ vue figée`
tant qu'on ne l'a pas rejointe. `f` y ramène, et tout saut de lecture aussi.

**Chercher.** `/` ouvre la saisie, `Entrée` envoie la lecture sur la première
occurrence à partir du curseur, `N` et `P` parcourent les suivantes. La
recherche ignore la casse **et les accents** — `specification` trouve
« Spécification » — et porte sur ce qui est affiché : les chiffres tels qu'ils
sont écrits comme le source des blocs de code.

## Configuration

Les réglages tiennent sur **une page** — `s` en cours de lecture, ou
`sillon --config` :

```
  Réglages   moteur : Kokoro · ONNX

  ▸ Voix française          p-fenrir60 · française, recommandée
    Voix anglaise           af_bella · américaine, femme, recommandée
    Vitesse                 1.00×
    Largeur de colonne      plein écran
    Interlignage            une ligne
    Règle de lecture        oui
```

| Touche | Effet |
|---|---|
| `↑` `↓` | Choisir un réglage |
| `←` `→` · `Entrée` | Changer sa valeur |
| `t` | Écouter la voix |
| `q` · `Échap` · `s` | Fermer les réglages (sans quitter la lecture) |
| `?` | Aide |

**Chaque changement s'enregistre aussitôt** dans `config.toml`, sans touche
à presser, et s'entend tout de suite : la vitesse sur-le-champ, une voix en
reprenant la phrase. Seul le réglage changé s'écrit : une option de la ligne
de commande (`--speed 2`) reste à ce lancement. Un `config.toml` illisible
n'est jamais réécrit ; la page dit alors que le réglage n'est pas enregistré.

Le reste du fichier — moteur, fils de calcul, sessions, blocs de code,
langue, hauteur — ne s'affiche plus : il garde sa valeur, et se règle à la
main.

Le fichier vit dans `~/.config/sillon/config.toml`, et peut aussi être écrit
par `sillon --write-config`.

```toml
[engine]
engine = "auto"           # auto | system
model_path = "~/Library/Application Support/sillon/models/kokoro/kokoro-v1_0.safetensors"
family = "kokoro"
threads = 8               # Kokoro au terminal (ONNX Runtime), borné à 4
sessions = 0              # synthèses simultanées ; 0 = ce que la famille demande
prefetch = 6              # phrases préparées en avance

[voice]
voice = ""
language = "auto"         # auto | fr | en…
speed = 1.0
pitch = 0.0               # voix du système seulement
volume = 1.0

[voice.per_language]      # la voix Kokoro de chaque langue
fr = "p-fenrir60"
en = "af_bella"

[reading]
code_blocks = "announce"  # announce | skip | read
autoplay = true
seek_step = 5
line_width = 0            # largeur de la colonne de texte ; 0 = plein écran
line_spacing = 1          # lignes vides entre deux lignes de texte
reading_ruler = true      # fond sous la ligne en cours de lecture

[app]
keep_voice_minutes = 10   # voix gardée chargée après la lecture ; -1 = toujours
output_device = ""        # sortie audio par son nom ; "" = celle du système.
                          # Choisie, le son n'en sort jamais : débranchée, la
                          # lecture se met en pause au lieu de passer ailleurs.
```

Les sections `[sampling]` et `[extra]`, et les clés `compute`, `energy`,
`emotion` et `instruction`, servaient aux modèles retirés le 14 octobre 2026 :
une configuration qui les porte encore s'ouvre sans erreur, et elles
disparaissent au prochain enregistrement.

`sillon --help` montre les options de tous les jours (`--clip`, `--speed`,
`--voice`, `--to-markdown`, `--no-cache`, `--config`). Les autres restent
acceptées : `--pitch`, `--language`, `--engine`, `--model`, `--family`,
`--sessions`, `--prefetch`, `--code-blocks`, `--no-autoplay`,
`--write-config`, `--show-config`, `--check-kokoro`.

## Architecture

```
entrée ──▶ doc ──▶ tts ──▶ audio ──▶ app ──▶ tui
(flux,    (blocs,  (pool    (sortie,  (état,   (rendu,
 blocs)   phrases) parallèle) position) commandes) touches)
```

- `input` — lecture non bloquante, regroupement en blocs markdown.
- `models` — catalogue, installation et suppression des modèles.
- `tts::lazy` — charge le moteur en arrière-plan pour que l'interface soit immédiate.
- `tts::preview` — aperçus de voix, sur une session tenue ouverte à part.
- `logging` — détourne les sorties natives vers `sillon.log`.
- `doc` — hiérarchie append-only : sections, blocs, phrases, mots.
- `text` — nettoyage du formatage et découpage en phrases (`sanitize`), mise en mots des dates, des nombres et des sigles pour la voix seule (`verbalize`).
- `tts` — contrat moteur/session, pool parallèle, Kokoro (MLX ou ONNX Runtime), repli système.
- `audio` — file de sortie et position déduite du son réellement joué.
- `app` — le contrôleur : document, lecture, recherche, page des réglages. Il ne connaît aucun écran.
- `tui` — rendu ratatui du contrôleur, adaptation des touches.

Les sorties natives (ONNX Runtime, espeak-ng) sont détournées vers
`~/.local/share/sillon/sillon.log` : en plein écran, elles disloqueraient le
rendu.

L'ouverture et la fermeture des sessions sont sérialisées par un verrou ; la
synthèse, elle, n'en prend aucun — chaque session a son propre état.

## Tests

```bash
cargo test                 # tout, sauf ce que la reconnaissance de texte demande
cargo test --features ocr  # y ajoute image, dossier, presse-papiers, PDF scanné
```

La seconde ligne n'est pas un supplément : `tests/image_input.rs` est compilé
sous `#![cfg(feature = "ocr")]`, comme le test du PDF photocopié de
`tests/pdf_input.rs`. Sans elle, ces chemins ne sont pas *ignorés* — ils
n'existent pas dans le binaire de test, et rien ne signale leur absence.

Deux sondes ne tournent qu'à la demande, faute d'être vérifiables par une
assertion : elles rendent un son ou une mesure, à écouter ou à lire.

```bash
# Ce qu'un document donne à l'oreille, phrase par phrase
DOC=chemin.md cargo test --test document_probe -- --ignored --nocapture

# Le temps du premier son, du lancement à la voix
cargo test --release --features kokoro-onnx --test startup_timing -- --ignored --nocapture
```
