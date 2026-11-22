# Procédure de port d'un modèle de Swift vers `mlx-rs`

À lire en entier avant toute tâche marquée **Port** du plan
`2026-11-22-moteurs-modeles.md`. La tâche donne les fichiers, les types, les
constantes et les cas propres au modèle ; ce document donne ce qui est commun.

## Ordre obligatoire

Chaque port fait deux commits au moins, et le code Swift ne part qu'au dernier.

- [ ] **P1. Figer la référence avant d'écrire du Rust.** Un test Swift, ignoré
  sans `DYSFLOW_WRITE_FIXTURES`, lance le modèle Swift actuel sur les entrées
  de la tâche et écrit `tests/fixtures/mlx/<modèle>.json` : commit d'origine,
  révision du modèle, SHA-256 de chaque entrée, graine, sorties. Dictée :
  texte **et** jetons, plus les formes et la moyenne du mel et de la sortie de
  l'encodeur. Voix : durées ou codes, et audio en PCM 16 bits base64. Deux
  générations de suite donnent le même fichier, sinon s'arrêter et expliquer.
  Committer la fixture et son générateur.
- [ ] **P2. Écrire le test de parité Rust et le voir échouer.** Il est
  `#[ignore]` (il demande les poids), lit le dossier des modèles dans
  `DYSFLOW_MODELS` et **échoue** si la variable ou le modèle manque : lancé
  exprès, il ne passe jamais à vide.
  `DYSFLOW_MODELS=… cargo test --release -p dysflow --features mlx --lib <module>::parity -- --ignored`
- [ ] **P3. Traduire, de l'entrée vers la sortie** : pré-traitement, encodeur,
  décodeur, post-traitement. Chaque étage est comparé à la fixture avant
  d'écrire le suivant. Mêmes opérations dans le même ordre, mêmes types
  numériques, mêmes points d'`eval`.
- [ ] **P4. Porter chaque test Swift du modèle** (fumée et unitaires) en Rust,
  sous le même nom en snake_case, avec les mêmes valeurs attendues. Le rapport
  donne la table « test Swift → test Rust ».
- [ ] **P5. Mesurer Swift contre Rust** en Release, machine au calme
  (charge < 2, pas de swap, relevé joint) : premier appel, médiane à chaud de
  10 passages, pic MLX, mémoire résidente. Seuils : médiane ≤ 1,10 × Swift et
  pic ≤ 1,10 × Swift.
- [ ] **P6. Basculer.** `Runtime::loadable` rend vrai sans calcul Swift pour ce
  moteur, l'app appelle le cœur, les tests de l'app passent.
- [ ] **P7. Essayer l'app Release** : charger le modèle, l'utiliser, le laisser
  se libérer, le réutiliser.
- [ ] **P8. Supprimer le Swift du modèle** et les fichiers communs qui n'ont
  plus d'appelant (`grep` joint au rapport). Reporter dans `NOTICE` et
  `ORIGINE.md` la provenance et la licence du code traduit : une traduction
  garde la licence de sa source.

## Règles

- **Un tableau MLX ne sort jamais du fil MLX.** `mlx_rs::Array` n'est pas
  `Send`. Un modèle vit dans un `compute::mlx::Held<T>` et se touche par
  `Held::with`. Jamais d'`unsafe impl Send` ou `Sync` pour le contourner.
- Tout résultat est évalué puis copié (`Vec<f32>`, `String`) avant de sortir
  de `with` : une évaluation paresseuse hors du verrou calculerait hors du
  passage unique.
- Aucune panique : le terminal est en `panic = "abort"`. Chaque opération MLX
  rend son `Result` ; pas d'`unwrap`, d'`expect` ni d'indexation non vérifiée
  sur ce qui vient d'un fichier de modèle.
- Un fichier de poids tronqué, une clé absente ou une forme inattendue rend
  une erreur bilingue qui nomme le fichier ou la clé. Un test par cas, sur des
  fichiers synthétiques, sans poids réels.
- `cancel` est consulté aux mêmes points qu'en Swift et rend `tts::Cancelled`,
  jamais un échec.
- Les plafonds mémoire passent par `compute::mlx::budget`, avec les valeurs du
  Swift.
- Les identifiants de paquet, les dossiers et les fichiers lus ne changent
  pas : rien à retélécharger.
- Un fichier par étage (`config`, `audio`, `encoder`, `decoder`, `tokenizer`),
  environ 600 lignes au plus.

## Quand s'arrêter

- Les jetons ou le texte diffèrent de la fixture, ou l'écart audio atteint
  0,01, sans cause trouvée : statut BLOCKED, avec le premier étage qui diverge.
  Ne pas élargir un seuil, ne pas régénérer la fixture, ne pas ignorer le test.
- Un seuil de P5 est dépassé : donner les mesures au contrôleur. Le Swift
  reste en place tant que ce n'est pas tranché.
- Une opération manque dans `mlx-rs` : le dire, avec l'appel `mlx-sys`
  équivalent proposé, avant d'écrire de l'`unsafe`.
