# reader

Lecteur audio en terminal : il prend du markdown — pipé ou en fichier —, le
hiérarchise, et le lit à voix haute avec les contrôles d'un lecteur audio.

```bash
reader                                      # ouvre le menu de configuration
cat notes.md | reader
reader rapport.md --speed 1.5
llm "explique-moi les monades" | reader     # la lecture démarre avant la fin du flux
```

**Lancé sans argument, `reader` ouvre l'écran de configuration.** Depuis la
lecture, la touche `s` y mène aussi ; la barre d'état le rappelle en
permanence.

## Ce qu'il fait

- **Lit un flux au fil de l'eau.** Le texte est mis en voix bloc par bloc ; il
  n'attend ni la fin de l'entrée, ni la fin de la synthèse.
- **Accepte un PDF** qui porte une couche texte, et le reconstruit : titres
  rendus au sommaire, mots coupés recollés, en-têtes et pieds de page retirés.
  `--to-markdown` écrit le texte extrait sans lancer la lecture.
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
- **Garde la page du fichier.** Les lignes, l'indentation et l'alignement des
  colonnes sont ceux du document : ce qui est à l'écran se reconnaît.

## Installation

```bash
cargo install --path .          # voix du système, build en quelques secondes
```

### Distribuer le binaire

Le shim audio.cpp est une bibliothèque partagée, que le binaire charge au
démarrage. `scripts/package.sh` assemble les deux dans `dist/` et vérifie que
l'ensemble s'exécute hors de l'arbre de compilation :

```bash
./scripts/package.sh                 # audio-cpp + ocr par défaut
FEATURES=audio-cpp ./scripts/package.sh
```

Distribuez le dossier entier : le binaire cherche la bibliothèque à côté de
lui, puis dans un `lib/` voisin. Un binaire copié seul ne démarre pas.

Pour la **reconnaissance de texte** — images, dossiers de scans, images
collées —, Tesseract est compilé dans le binaire. Comptez une minute au
premier build ; ensuite rien n'est à installer, les données de langue
(français et anglais) sont embarquées :

```bash
cargo build --release --features ocr
```

Pour la synthèse par **audio.cpp**, liée directement dans le binaire :

```bash
git submodule add https://github.com/0xShug0/audio.cpp vendor/audio.cpp
git submodule update --init --recursive
cargo build --release --features audio-cpp
```

Le build ne compile que les familles de modèles utiles — `supertonic,pocket_tts`
par défaut, sur les 62 que porte le projet :

```bash
RS_READER_TTS_FAMILIES=supertonic,magpie_tts cargo build --release --features audio-cpp
RS_READER_AUDIOCPP_DIR=/chemin/vers/audio.cpp cargo build --features audio-cpp
```

### Installer un modèle

Lancez `reader` sans argument : l'écran s'ouvre sur la rubrique **Modèles**, qui
liste ce qui est installable avec la taille et l'espace disque libre. `Entrée`
installe, puis sélectionne. Rien n'est jamais téléchargé sans cette action.

| Modèle | Taille | Langues | Voix |
|---|---|---|---|
| MagpieTTS Multilingual · Q8_0 | 1,6 Go | 11, dont le français | Aria · Jason · John · Leo · Sofia |
| Supertonic 3 · F16 | 298 Mo | 31, dont le français | `M1`–`M5`, `F1`–`F5` |
| Supertonic 3 · Q8_0 | 433 Mo | 31, dont le français | idem |
| Supertonic 3 · précision d'origine | 433 Mo | 31, dont le français | idem |

Le catalogue ne retient que des modèles **à voix nommées** : aucun n'exige qu'on
lui fabrique une voix — enregistrement à cloner ou description à composer —
avant de pouvoir l'écouter. Un test le vérifie à chaque compilation.

Les modèles vont dans `~/.local/share/rs-reader/models/<famille>/` (sur macOS,
`~/Library/Application Support/rs-reader/models/`). Un modèle déjà installé est
adopté automatiquement au lancement suivant.

Le binaire ne sait charger que les familles compilées en lui — `supertonic` et
`magpie_tts` par défaut. Pour en ajouter une autre parmi la soixantaine que porte
audio.cpp :

```bash
RS_READER_TTS_FAMILIES=supertonic,magpie_tts,qwen3_tts \
  cargo build --release --features audio-cpp
```

Le catalogue ne propose que ce que le binaire sait charger : une famille absente
n'apparaît pas dans la liste.

Le modèle se charge **en arrière-plan** : l'interface s'ouvre en 0,05 s et la
barre de lecture annonce l'étape en cours — `⠹ chargement du modèle
supertonic… 2,4 s`, puis `synthèse de la première phrase…` — jusqu'à ce que le
son démarre, où elle redevient la barre de progression. Rien n'est perdu : la
première phrase n'aurait de toute façon pas pu être prononcée avant. Ce délai
ne se raccourcit pas depuis le lecteur, il appartient au moteur ; ce qui se
corrige, c'est de le traverser sans savoir s'il se passe quelque chose.

Mesuré sur un Apple M5 en Metal, sur la même phrase française :

| | Supertonic 3 F16 | MagpieTTS Q8_0 |
|---|---|---|
| Chargement du modèle | 3,0 s | 3,2 s |
| Ouverture d'une session | **0,3 s** | 2,4 s |
| Synthèse | **4,0× le temps réel** | **0,6×** |
| Échantillonnage | **44,1 kHz** | 22,05 kHz |
| Contrôle du débit | **oui** | aucun |

Ces chiffres valent à chaud. Au tout premier lancement, cache disque vide, le
chargement monte à 9,6 s — d'où le chrono affiché plutôt qu'une estimation, qui
se tromperait d'un facteur trois au moment où elle compterait le plus.

Le budget jusqu'au premier son se mesure :

```bash
cargo test --release --features audio-cpp --test startup_timing -- --ignored --nocapture
```

Les deux modèles ne jouent pas dans la même catégorie. Supertonic synthétise
**six fois plus vite** que MagpieTTS et reste largement en avance sur la
lecture. MagpieTTS, à 0,6× le temps réel sur une session, est **plus lent que la
parole** : les trois sessions par défaut suffisent à lire d'un trait, mais
chaque saut coûte près de deux fois la durée de la phrase visée avant que le son
reprenne. Il n'expose par ailleurs aucun réglage de débit.

Choisissez donc MagpieTTS pour ses voix, Supertonic pour parcourir un long
document.

Les sessions s'ouvrent l'une après l'autre — voir la note sur le verrou plus
bas — si bien que la mise en route coûte autant de fois qu'il y a de sessions
simultanées. Baisser *Sessions simultanées* à 1 ou 2 dans la rubrique **Moteur**
la raccourcit.

### Ce qui est dit au moteur

Supertonic lit **caractère par caractère** : il n'a aucune normalisation de
nombres, et « 1 » lui parvient comme un signe dont il doit deviner la
prononciation. Le lecteur écrit donc en toutes lettres, avant la synthèse, tout
ce qui se lit autrement qu'il ne s'écrit :

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

Rubrique **Voix**, champ *Voix*, `←` `→` pour parcourir les voix du modèle, `t`
pour écouter celle qui est retenue. Rien d'autre à faire.

Supertonic 3 en propose dix, `M1`–`M5` et `F1`–`F5`. MagpieTTS en propose cinq,
nommés — Aria, Jason, John, Leo, Sofia — relevés dans le `speakers.json`
embarqué de son paquet. Je ne les ai pas écoutés : `t` le dira mieux qu'une
étiquette.

Sans modèle installé — ou sans la feature — le lecteur se replie sur la voix du
système (`say` sur macOS, `espeak-ng` ailleurs). La barre d'état affiche alors
`⚠ voix système` et la marche à suivre, en permanence.

## Contrôles

| Touche | Effet |
|---|---|
| `Espace` | Lecture / pause |
| `←` `→` | Phrase précédente / suivante |
| `Maj+←` `Maj+→` | Recul / avance rapide |
| `↑` `↓` | Bloc précédent / suivant |
| `n` `p` | Titre suivant / précédent |
| `Début` `Fin` | Début / fin du document |
| `r` | Reprendre la phrase |
| `+` `-` | Vitesse |
| `1` `2` `3` `4` | 0,75× · 1× · 1,5× · 2× |
| `[` `]` · `m` | Volume · couper le son |
| `Pg↑` `Pg↓` | Défiler sans déplacer la lecture |
| `/` | Chercher dans le document |
| `N` · `P` | Résultat suivant · précédent |
| `b` | Poser / retirer un signet |
| `t` · `B` | Sommaire · signets |
| `s` | Configuration et modèles |
| `t` *(dans la configuration)* | Écouter un aperçu de la voix |
| `--config` | Ouvrir la configuration au lancement |
| `f` | Revenir à la lecture après un défilement |
| `?` | Aide |
| `q` | Quitter |

La vitesse agit **à la synthèse**, pas par ré-échantillonnage : la voix accélère
sans monter dans les aigus. Changer de vitesse relance donc la phrase en cours.


## Lire en écoutant

Le lecteur est fait pour être suivi des yeux, pas seulement entendu. Trois
réglages, dans la rubrique **Lecture**, gouvernent ce que l'écran demande à
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
alors recomposé d'un seul tenant.

**Ce qui n'est pas lu reste montré.** Un bloc de code annoncé — le réglage par
défaut — se dit « bloc de code rust » et s'affiche pourtant en entier, avec son
indentation ; une ligne trop longue est repliée sous un filet `╎`, jamais
tronquée. Un tableau garde ses colonnes alignées à l'écran, et se dit colonne
par colonne :

```
  │ Route        Méthode  Auth          « Route : slash users.
  │ /users       GET      JWT             Méthode : GET.
  │ /users/{id}  DELETE   JWT admin       Auth : JWT. »
```

Sans cet appariement, une ligne de tableau se disait « slash users, GET, JWT » :
trois valeurs sans leur colonne. Chaque cellule est par ailleurs une phrase,
donc `←` `→` parcourent le tableau cellule par cellule.

**Défiler sans perdre la voix.** `Pg↑` et `Pg↓` déplacent la vue seule : la
lecture continue là où elle en était, et la barre d'état affiche `⏸ vue figée`
tant qu'on ne l'a pas rejointe. `f` y ramène, et tout saut de lecture aussi.

**Chercher.** `/` ouvre la saisie, `Entrée` envoie la lecture sur la première
occurrence à partir du curseur, `N` et `P` parcourent les suivantes. La
recherche ignore la casse **et les accents** — `specification` trouve
« Spécification » — et porte sur ce qui est affiché : les chiffres tels qu'ils
sont écrits comme le source des blocs de code.

## Configuration

Tout se règle depuis l'écran de configuration — `reader` sans argument, ou `s`
en cours de lecture :

| Rubrique | Contenu |
|---|---|
| **Modèles** | Installer, choisir, supprimer. Taille, langues, espace libre. |
| **Voix** | Voix, vitesse, volume, langue. |
| **Échantillonnage** | Température, top-p, top-k, guidage, étapes, graine. |
| **Lecture** | Largeur de colonne, interlignage, règle, blocs de code, pas d'avance. |
| **Moteur** | Moteur, backend, threads, sessions, préchargement, modèle. |

Le menu porte l'état de chaque rubrique en face de son nom — modèles installés,
voix et débit retenus, moteur chargé — pour ne pas avoir à l'ouvrir pour le
savoir.

L'écran fonctionne **par pages**, jamais en colonnes côte à côte. Il s'ouvre sur
un menu qui n'affiche que les cinq rubriques et leur état :

```
   ▸ 1  Modèles            3 installés
     2  Voix               Aria · 1.00×
     3  Échantillonnage    défauts
     4  Lecture            plein écran · announce
     5  Moteur             magpie_tts
```

Un chiffre ouvre sa rubrique en plein écran ; `Échap` ramène au menu, `Échap`
encore ferme l'écran. Les flèches n'ont donc qu'un sens à la fois.

| Touche | Effet |
|---|---|
| `1` … `5` | Ouvrir une rubrique directement |
| `↑` `↓` | Se déplacer dans la liste |
| `Entrée` | Ouvrir · éditer · installer · choisir |
| `←` `→` | Modifier une valeur |
| `Échap` | Revenir au menu, puis fermer |
| `t` | Écouter un aperçu de la voix |
| `a` | Montrer les réglages sans effet |
| `w` | Enregistrer |

**Les réglages inertes sont masqués par défaut.** Sur Supertonic, cinq des sept
réglages d'échantillonnage ne servent à rien : les afficher noyait les deux qui
agissent. Une ligne discrète dit combien sont cachés, et `a` les rend visibles,
marqués `(ignoré par ce modèle)`. Le relevé vient du code de chaque famille :

| | Supertonic 3 | MagpieTTS |
|---|---|---|
| Voix | dix préréglages | cinq locuteurs nommés |
| Langue · graine | honorées | honorées |
| Vitesse | **honorée** | aucun contrôle |
| Étapes d'inférence | honorée | — |
| Température · top-k · guidage | ignorés | **honorés** |
| Hauteur · énergie · intonation · style | ignorés | ignorés |

Un réglage marqué inerte le dit aussi dans son explication, au lieu de décrire
ce qu'il ferait s'il agissait.

Les options sont **filtrées par famille** avant d'atteindre le modèle. Ce n'est
pas cosmétique : MagpieTTS valide sa requête et refuse tout net une option qu'il
ne connaît pas — « unknown MagpieTTS request option: speaking_rate » — au lieu
de l'ignorer.

Le filtrage par famille ne suffit pourtant pas, parce que la liste des options
acceptées est portée par le **paquet** et non par la famille : deux variantes du
même modèle n'embarquent pas la même spécification. La session **négocie** donc.
Quand le modèle refuse une option en la nommant, elle est écartée et la requête
repart, jusqu'à quatre fois. La voix, elle, a plusieurs orthographes connues —
`voice_id`, `speaker` — essayées l'une après l'autre : renoncer ferait parler
tous les locuteurs de la même voix, en silence.

Les préréglages, eux, sont propres à chaque modèle : `F1` ne veut rien dire pour
MagpieTTS, qui n'y voit ni un locuteur ni un index et le laisse descendre
jusqu'à `std::stoll` — d'où un « stoll: no conversion » pour toute explication.
**La voix suit donc la famille** : changer de modèle la remplace par celle du
nouveau, et une voix inconnue n'atteint jamais le moteur.

C'est aussi pourquoi la variante MagpieTTS en précision d'origine n'est pas au
catalogue. Sa spécification nomme l'option `speaker`, alors que le code
d'audio.cpp ne lit que `voice_id` : le paquet se charge, parle, et rend un
signal identique au bit près pour ses cinq locuteurs.

Le nom de l'option de débit diffère d'une famille à l'autre — `speaking_rate`
chez Supertonic, `speed` ailleurs. Les deux clés sont émises quand la famille
les accepte.

Les valeurs prennent effet immédiatement : la synthèse déjà préparée est
régénérée au besoin.

Le fichier vit dans `~/.config/rs-reader/config.toml`, et peut aussi être écrit
par `reader --write-config`.

```toml
[engine]
engine = "auto"           # auto | audio_cpp | system
model_path = "~/modeles/supertonic"
family = "supertonic"
compute = "metal"         # cpu | metal | cuda | vulkan
threads = 8
sessions = 3              # synthèses simultanées
prefetch = 6              # phrases préparées en avance

[voice]
voice = ""                # préréglage, ou chemin d'un WAV à cloner
language = "fr"
speed = 1.0
pitch = 0.0
energy = 1.0
emotion = ""              # transmis en StyleCondition::emotion
instruction = ""          # consigne de style en langage naturel
volume = 1.0

[sampling]
temperature = 0.7
top_p = 0.9
guidance_scale = 3.0
inference_steps = 16
seed = 42

[reading]
code_blocks = "announce"  # announce | skip | read
autoplay = true
seek_step = 5
line_width = 0            # largeur de la colonne de texte ; 0 = plein écran
line_spacing = 1          # lignes vides entre deux lignes de texte
reading_ruler = true      # fond sous la ligne en cours de lecture

# Tout paramètre propre à une famille, transmis tel quel au modèle.
[extra]
cfg_scale = "2.5"
```

Les mêmes réglages existent en ligne de commande (`--voice`, `--speed`,
`--emotion`, `--instruction`, `--set cle=valeur`…). Voir `reader --help`.

## Architecture

```
entrée ──▶ doc ──▶ tts ──▶ audio ──▶ tui
(flux,    (blocs,  (pool    (sortie,  (rendu,
 blocs)   phrases) parallèle) position) contrôles)
```

- `input` — lecture non bloquante, regroupement en blocs markdown.
- `models` — catalogue, installation et suppression des modèles.
- `tts::lazy` — charge le moteur en arrière-plan pour que l'interface soit immédiate.
- `tts::preview` — aperçus de voix, sur une session tenue ouverte à part.
- `logging` — détourne les sorties natives vers `reader.log`.
- `doc` — hiérarchie append-only : sections, blocs, phrases, mots.
- `text` — nettoyage du formatage et découpage en phrases (`sanitize`), mise en mots des dates, des nombres et des sigles pour la voix seule (`verbalize`).
- `tts` — contrat moteur/session, pool parallèle, FFI audio.cpp, repli système.
- `audio` — file de sortie et position déduite du son réellement joué.
- `tui` — rendu fenêtré, contrôles, panneaux.

Les sorties de ggml sont détournées vers
`~/.local/share/rs-reader/reader.log` : en plein écran, elles disloqueraient le
rendu.

Le FFI vit dans `native/` : `audiocpp_shim.{h,cpp}` referme l'API C++ d'audio.cpp
derrière une façade C, sans laisser passer d'exception.

L'ouverture et la fermeture des sessions sont sérialisées par un verrou.
audio.cpp partage les poids entre sessions derrière un `shared_ptr<const>`,
mais ce `const` ne rend rien thread-safe : le lecteur de tenseurs porte un cache
rempli paresseusement (`mutable BinaryBlob bytes_`). Deux sessions qui
téléversent leurs poids en même temps mutent le même cache sans verrou, et le
`memmove` de `BackendWeightStore::upload` part hors de la mémoire cartographiée.

Mesuré : sans verrou, trois plantages sur vingt fermetures ; avec un verrou
laissant les ouvertures concurrentes, six sur vingt-cinq ; avec le verrou
exclusif, aucun sur quatre-vingt-douze. La synthèse, elle, ne prend aucun verrou
— chaque session a son propre graphe.

## Tests

```bash
cargo test
```
