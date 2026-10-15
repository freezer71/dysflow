# Simplification de l'expérience, plan 2 : l'app macOS — plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal :** l'app macOS ne sert plus qu'à écouter un texte en le suivant des yeux : Notes et signets retirés, panneau à deux formes (lecteur, mini), texte sélectionnable et copiable sans entrer en édition, clavier réduit, menu de cinq lignes, réglages sur une page, voix installée toute seule.

**Architecture :** `DisplayMode` devient `mini / reader / hidden`, avec des transitions pures (`DisplayState`) : ⌘M bascule la taille, fermer (⌘W, Échap, bouton, ⌃⌥S) retire la fenêtre et met en pause, toute ouverture rend la dernière taille. Le lecteur (`ReaderView`) porte un en-tête à trois boutons et une barre de transport par phrase ; le mini garde la même barre réduite. `ReadingTextView` devient sélectionnable ; la copie, le menu contextuel et « Lire à partir d'ici » passent par `LineNavigator`. Les réglages tiennent sur une page ; police et taille passent dans `config.toml` (`SettingsView.readingFont / readingSize`, plan 1), avec reprise des préférences. Au lancement, `ensureDefaultVoice()` (plan 1) installe Kokoro ; une ligne d'état n'apparaît qu'en cas de problème, avec « Réessayer ». En fin de plan, le pont FFI perd les signets et les commandes que plus personne n'envoie.

**Tech Stack :** Swift 6.x en mode de langage 5, SwiftUI + AppKit (TextKit 2), macOS 14+, XcodeGen, KeyboardShortcuts 2.x, XCTest / XCUITest ; Rust (crate `sillon-ffi`, UniFFI 0.32) pour la dernière tâche.

**Spec :** `docs/superpowers/specs/2026-10-15-simplification-ux-design.md`, **section 1 en entier**, plus les lignes « FFI » de la section 3 et « macOS » de la section 4. À lire avant toute tâche. Contexte : `docs/superpowers/specs/2026-10-08-modes-affichage-design.md` (les quatre modes que ce plan remplace) et `docs/superpowers/plans/2026-10-15-plan-1-moteur-ffi.md` (ce que le plan 1 a livré).

## Global Constraints

- macOS 14 minimum, Apple Silicon (`arm64`) seul ; Swift en **mode de langage 5** (`SWIFT_VERSION: "5.0"`), concurrence stricte : **aucun nouvel avertissement**.
- Textes d'interface, commentaires, noms de tests et messages de commit en **français**, accents corrects, comme le code existant. Interface en police système 13 pt.
- L'app reste sans icône de Dock (`LSUIElement`), **sans exception** : plus aucun `setActivationPolicy(.regular)`.
- Deux raccourcis globaux seulement : **⌃⌥Espace** (`.readSelection`) et **⌃⌥S** (`.showPanel`, « afficher / masquer le lecteur »). Les noms `KeyboardShortcuts` gardent leurs identifiants (`"readSelection"`, `"showPanel"`) : un raccourci personnalisé survit.
- Invitation du lecteur vide, mot pour mot : « Collez un texte (⌘V) ou déposez un fichier ».
- Menu de vitesse : **0,75 · 1 · 1,25 · 1,5 · 2×** ; ± 0,1 au clavier.
- Fermer (⌘W, Échap, bouton fermer, ⌃⌥S quand le lecteur est affiché) **met en pause** ; fermer ne transforme **jamais** le lecteur en mini. Réduire / agrandir (⌘M, boutons) **ne touche pas** à la lecture.
- Taille à l'ouverture : toujours la dernière (mini ou lecteur) ; lecteur vide au lancement manuel : toujours le lecteur.
- Les fichiers `~/Library/Application Support/sillon/Notes/` et `bookmarks.json` ne sont **ni lus ni supprimés**.
- Les tests ne touchent jamais aux données de l'utilisateur : lecteur muet (`SillonReader.silent()`), suites `UserDefaults` isolées et vidées, presse-papiers nommé pour les tests unitaires (`NSPasteboard(name:)`). Les tests d'interface qui écrivent le presse-papiers général le **sauvent et le rendent** (`tearDown`).
- **Tests d'interface** : écrits et compilés (`build-for-testing`), **jamais exécutés par l'implémenteur** ; leur exécution est une étape à part, soumise au feu vert explicite de l'utilisateur (Tâche 12).
- Ne jamais journaliser une touche ni un texte lu.
- `git add` de **chemins explicites** seulement ; `git rm` pour un fichier retiré. Jamais `macos/Generated`, `macos/Libraries`, `macos/*.xcodeproj`, `macos/build` (ignorés).
- Commandes (depuis la racine du dépôt) :
  - **`TEST`** `<filtre>` : `(cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' -derivedDataPath build/derived test -only-testing:SillonTests<filtre>)` — `<filtre>` vaut `/NomDeClasse` pendant une tâche ; la suite entière (filtre vide) avant chaque commit.
  - **`UIBUILD`** : `(cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' -derivedDataPath build/derived build-for-testing)`.
  - **`FFI`** : `./scripts/build-ffi.sh` (liaison Swift `macos/Generated/sillon_ffi.swift` et `macos/Libraries/libsillon_ffi.a`) ; à relancer après tout changement Rust, avant `xcodebuild` (l'étape de pré-construction refuse une bibliothèque périmée).
  - Rust : `cargo test --workspace --locked` et `cargo clippy --workspace --all-targets --locked -- -D warnings`.
- Noms fournis par le plan 1, utilisés tels quels : `SettingsView.readingFont: String`, `SettingsView.readingSize: Float`, `SettingsPatch.readingFont: String?`, `SettingsPatch.readingSize: Float?`, valeurs de police `"literata" | "atkinson" | "openDyslexic"` (les `rawValue` de `ReadingFont`), taille par défaut 22, bornée 14–40 ; `SillonReader.ensureDefaultVoice() throws`, appelé **après** `migrateVoice()`, rappelé par « Réessayer », progression dans `Snapshot.download`, échec dans `Snapshot.download.failed`.
- **Pas de `ReaderCommand.seekToFraction`** (le plan 1 ne l'a pas ajoutée) : la barre cliquable convertit la fraction en phrase, `min(Int(f × sentenceCount), sentenceCount − 1)`, et envoie `.seekTo(sentence:)` (Tâche 3, `ProgressSeek`).
- Messages de commit : style du dépôt (`feat(macos): …`, `refactor(macos): …`, `test(macos): …`, `refactor(ffi): …`), en français, terminés par la ligne `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

Cinq situations que la spec implique sans les tester, les plus susceptibles de mordre un utilisateur ; chacune reçoit son test dans la tâche indiquée.

1. **Préférences d'affichage écrites par la version précédente** (`affichage.mode = "large"`, `affichage.dernierVisible = "small"`, `"background"`) : attendu, elles se traduisent (petit → mini, moyen et grand → lecteur, arrière-plan → masqué) au lieu de retomber sur rien ou de planter. → Tâche 2, `testModesStoredBeforeTheUpdateAreTranslated`.
2. **Cadre du lecteur retenu sur un écran débranché, ou plus grand que l'écran actuel** : attendu, le lecteur revient entier sur l'écran principal, jamais hors de vue ni plus grand que l'écran. → Tâche 2, `testAReaderFrameOffEveryScreenComesBackOnTheMainScreen` et `testAReaderFrameLargerThanTheScreenIsShrunk`.
3. **Fermer (⌘W, ⌃⌥S) pendant une modification du texte, alors que la voix lisait avant l'édition** : attendu, la saisie est gardée (le texte modifié remplace le document) et la lecture reste en pause — la reprise différée de l'édition ne relance pas la voix fenêtre fermée. → Tâche 2, `testClosingWhileEditingKeepsTheTextAndStaysPaused`.
4. **Sélectionner pendant que la voix avance** : le suivi déplace le trait et fait défiler le texte à chaque mot ; attendu, la sélection reste intacte, et ⌘C copie la sélection, pas la phrase. → Tâche 6, `testFollowingTheVoiceKeepsTheSelection` et `testCommandCCopiesTheSelectionThenTheSentence`.
5. **Reprise de la police et de la taille quand l'écriture échoue** (`config.toml` illisible) **ou que les préférences portent une valeur aberrante** (taille 400, police inconnue) : attendu, aucune valeur perdue (les clés restent pour le lancement suivant), la taille est bornée, une police inconnue est ignorée. → Tâche 8, `testAFailedWriteKeepsThePreferencesForTheNextLaunch` et `testOddValuesAreBoundedOrIgnored`.

Couverts aussi : presse-papiers vide ou protégé pour ⌘V (`ReadFlowTests` existants, inchangés) ; dépôt d'autre chose qu'un fichier (Tâche 7, `testOnlyFilesAreOpenedAndTheLastOneWins`) ; vitesse hors du menu, par exemple 1,1× (Tâche 3, `testSpeedsAreWrittenTheFrenchWayWithoutUselessZeros`).

## Préalable : l'état laissé par le plan 1

- [ ] **Vérifier la liaison du plan 1.**

```bash
./scripts/build-ffi.sh
grep -c "readingFont\|readingSize\|func ensureDefaultVoice() throws" macos/Generated/sillon_ffi.swift
```

Attendu : un compte ≥ 3. Sinon, s'arrêter : le plan 1 n'est pas fusionné.

- [ ] **Vérifier que l'app et ses tests passent avant toute modification** : **`TEST`** (filtre vide). Attendu : tout passe. Un échec ici n'est pas à corriger dans ce plan : le signaler.

## Structure des fichiers

| Fichier | Rôle |
|---|---|
| `macos/Sources/Notes/*` | retirés (Tâche 1) |
| `macos/Sources/App/LaunchKind.swift` (nouveau) | `LaunchKind` déplacé + `LaunchPresentation` : que montrer au lancement et à la réouverture |
| `macos/Sources/Panel/DisplayMode.swift` | `DisplayMode` (`mini`, `reader`, `hidden`), `DisplayEvent`, `DisplayState` |
| `macos/Sources/Panel/DisplayModeStore.swift` | formes retenues, place du mini, cadre du lecteur ; traduction des anciennes valeurs |
| `macos/Sources/Panel/WindowPlacements.swift` (nouveau) | `ScreenFit`, `MiniPlacement` (ex-`SmallPlacement`), `ReaderPlacement` |
| `macos/Sources/Panel/PanelPlacement.swift` | applique les cadres, retient déplacements et redimensionnements |
| `macos/Sources/Panel/PanelController.swift` | formes, fermeture avec pause, actions du clavier, copie, édition |
| `macos/Sources/Panel/ReaderView.swift` (nouveau, remplace `ExpandedReader.swift`) | le lecteur : en-tête, texte, barre, ligne d'état |
| `macos/Sources/Panel/BarControls.swift` (nouveau) | transport par phrase, timeline cliquable, menu de vitesse, boutons d'icône, formats |
| `macos/Sources/Panel/PlayerBar.swift` | ne garde que `PlayButton`, le style des boutons, `Timeline`, `PlayerBar.time` |
| `macos/Sources/Panel/MiniPlayerView.swift` | mini : temps, ⏮ ▶ ⏭, agrandir |
| `macos/Sources/Panel/PanelContent.swift` | contenu par forme, aide complète, dépôt de fichiers |
| `macos/Sources/Panel/Sidebar.swift` | sommaire seul |
| `macos/Sources/Input/KeyMap.swift` | table de la spec 1.5 |
| `macos/Sources/Reading/ReadingTextView.swift` | sélectionnable, menu contextuel, plus de double clic vers l'édition |
| `macos/Sources/Reading/LineNavigator.swift` | sélection et « Lire à partir d'ici » |
| `macos/Sources/Reading/ReadingCopy.swift` (nouveau) | ce que copie ⌘C |
| `macos/Sources/Settings/ReglagesView.swift` | la page unique |
| `macos/Sources/Settings/SettingsSections.swift` (nouveau) | sections Voix, Lecture, Raccourcis, Démarrage |
| `macos/Sources/Settings/ReadingPreferencesMigration.swift` (nouveau) | police et taille : UserDefaults → `config.toml` |
| `macos/Sources/App/VoiceStatus.swift` (nouveau) | la ligne d'état de la voix et « Réessayer » |
| `macos/Sources/MenuContent.swift` | cinq lignes |
| `macos/Sources/AppDelegate.swift` | lancement, réouverture, ouverture de fichiers, raccourcis, installation de la voix |
| `ffi/src/lib.rs` | signets et commandes inutilisées retirés (Tâche 11) |

---

### Tâche 1 : Notes retirées, `LaunchKind` déplacé, aucune icône de Dock

Le lancement manuel et la réouverture montrent désormais le lecteur, plus la page de notes. Cette tâche garde encore les quatre anciens modes : le lecteur vide s'ouvre au mode moyen (la Tâche 2 en fera le lecteur).

**Files:**
- Delete : `macos/Sources/Notes/NotesStore.swift`, `macos/Sources/Notes/NotesView.swift`, `macos/Sources/Notes/NotesWindow.swift`, `macos/Tests/NotesStoreTests.swift`
- Create : `macos/Sources/App/LaunchKind.swift`, `macos/Tests/LaunchKindTests.swift`
- Modify : `macos/Sources/AppDelegate.swift`, `macos/Sources/App/AppServices.swift:16`, `macos/Sources/MenuContent.swift:23-24`, `README.md:77`, `scripts/sillon-app.sh:4`
- `macos/project.yml` : rien à changer (la cible prend `Sources` en entier) ; **`TEST`** relance `xcodegen generate`.

**Interfaces:**
- Produces : `enum LaunchPresentation: Equatable { case nothing, emptyReader, lastSize }` ; `LaunchKind.presentation(atLogin: Bool, withFiles: Bool, hasDocument: Bool) -> LaunchPresentation` ; `LaunchKind.reopenPresentation(hasDocument: Bool) -> LaunchPresentation` ; `LaunchKind.launchedAtLogin(event:uptime:)` et `launchedWithFiles(event:)` inchangés ; `AppDelegate.present(_ p: LaunchPresentation)` (privé) ; arguments de test `--ui-testing-vide` (pas de texte d'exemple) et `--ui-testing-lancement-manuel` (même chemin qu'un lancement manuel).

- [ ] **Step 1 : écrire les tests qui échouent** — `macos/Tests/LaunchKindTests.swift` :

```swift
import AppKit
import XCTest
@testable import Sillon

/// Ce que montre sillon au lancement et à la réouverture.
@MainActor
final class LaunchKindTests: XCTestCase {
    func testALaunchRightAfterBootCountsAsLogin() {
        XCTAssertTrue(LaunchKind.launchedAtLogin(event: nil, uptime: 30))
        XCTAssertFalse(LaunchKind.launchedAtLogin(event: nil, uptime: 3600))
    }

    func testALaunchToOpenFilesIsRecognised() {
        let open = NSAppleEventDescriptor(eventClass: kCoreEventClass, eventID: kAEOpenDocuments,
                                          targetDescriptor: nil, returnID: AEReturnID(kAutoGenerateReturnID),
                                          transactionID: AETransactionID(kAnyTransactionID))
        let plain = NSAppleEventDescriptor(eventClass: kCoreEventClass, eventID: kAEOpenApplication,
                                           targetDescriptor: nil, returnID: AEReturnID(kAutoGenerateReturnID),
                                           transactionID: AETransactionID(kAnyTransactionID))
        XCTAssertTrue(LaunchKind.launchedWithFiles(event: open))
        XCTAssertFalse(LaunchKind.launchedWithFiles(event: plain))
        XCTAssertFalse(LaunchKind.launchedWithFiles(event: nil))
    }

    /// À la connexion, rien ; avec des fichiers, c'est l'ouverture du
    /// fichier qui montre le lecteur.
    func testLoginAndFilesShowNothingAtLaunch() {
        XCTAssertEqual(LaunchKind.presentation(atLogin: true, withFiles: false, hasDocument: false), .nothing)
        XCTAssertEqual(LaunchKind.presentation(atLogin: false, withFiles: true, hasDocument: false), .nothing)
        XCTAssertEqual(LaunchKind.presentation(atLogin: true, withFiles: true, hasDocument: true), .nothing)
    }

    /// Lancé à la main : le lecteur vide, toujours en grand ; un document
    /// déjà là, la dernière taille.
    func testAManualLaunchOpensTheReader() {
        XCTAssertEqual(LaunchKind.presentation(atLogin: false, withFiles: false, hasDocument: false), .emptyReader)
        XCTAssertEqual(LaunchKind.presentation(atLogin: false, withFiles: false, hasDocument: true), .lastSize)
    }

    /// Rouvert depuis le Dock, le Finder ou Spotlight : comme un lancement manuel.
    func testReopeningShowsTheReader() {
        XCTAssertEqual(LaunchKind.reopenPresentation(hasDocument: false), .emptyReader)
        XCTAssertEqual(LaunchKind.reopenPresentation(hasDocument: true), .lastSize)
    }

    /// Aucune icône de Dock, jamais : `LSUIElement`, et l'app reste accessoire.
    func testSillonStaysOutOfTheDock() {
        XCTAssertEqual(Bundle.main.object(forInfoDictionaryKey: "LSUIElement") as? Bool, true)
        XCTAssertEqual(NSApp.activationPolicy(), .accessory)
    }
}
```

- [ ] **Step 2 : vérifier l'échec** — **`TEST`** `/LaunchKindTests`. Attendu : échec de compilation, « type 'LaunchKind' has no member 'presentation' » (et `LaunchPresentation` inconnu).

- [ ] **Step 3 : implémenter.**

Créer `macos/Sources/App/LaunchKind.swift` (le `LaunchKind` de `NotesWindow.swift`, déplacé, et la décision) :

```swift
import AppKit

/// Que montrer en arrivant : rien, le lecteur vide (toujours en grand), ou
/// la dernière taille utilisée.
enum LaunchPresentation: Equatable {
    case nothing, emptyReader, lastSize
}

/// Ouvert par l'utilisateur, par macOS à l'ouverture de session, ou pour
/// ouvrir des fichiers ?
enum LaunchKind {
    /// Lu dans l'événement Apple du lancement (à appeler pendant
    /// `applicationWillFinishLaunching`). Sans lui, un démarrage dans les
    /// deux premières minutes de la machine passe pour une ouverture de
    /// session : rien ne s'impose au démarrage.
    @MainActor
    static func launchedAtLogin(event: NSAppleEventDescriptor? = NSAppleEventManager.shared().currentAppleEvent,
                                uptime: TimeInterval = ProcessInfo.processInfo.systemUptime) -> Bool {
        if let event, event.eventID == kAEOpenApplication,
           event.paramDescriptor(forKeyword: keyAEPropData)?.enumCodeValue == keyAELaunchedAsLogInItem {
            return true
        }
        return uptime < 120
    }

    /// Lancé pour ouvrir des fichiers (`sillon fichier.pdf`, « Ouvrir
    /// avec ») : le lecteur les montre à leur arrivée, par
    /// `application(_:open:)`, parfois après la fin du lancement.
    @MainActor
    static func launchedWithFiles(event: NSAppleEventDescriptor? = NSAppleEventManager.shared().currentAppleEvent) -> Bool {
        event?.eventID == kAEOpenDocuments
    }

    /// Au lancement : rien à la connexion ni avec des fichiers ; à la main,
    /// le lecteur.
    static func presentation(atLogin: Bool, withFiles: Bool, hasDocument: Bool) -> LaunchPresentation {
        if atLogin || withFiles { return .nothing }
        return reopenPresentation(hasDocument: hasDocument)
    }

    /// Rouvert (Dock, Finder, Spotlight, lanceur) : le lecteur vide en
    /// grand, ou la dernière taille s'il y a un document.
    static func reopenPresentation(hasDocument: Bool) -> LaunchPresentation {
        hasDocument ? .lastSize : .emptyReader
    }
}
```

Supprimer les Notes : `git rm macos/Sources/Notes/NotesStore.swift macos/Sources/Notes/NotesView.swift macos/Sources/Notes/NotesWindow.swift macos/Tests/NotesStoreTests.swift`.

Dans `macos/Sources/App/AppServices.swift`, retirer la ligne `var notes: NotesWindowController?`.

Dans `macos/Sources/MenuContent.swift`, retirer les deux lignes :

```swift
        Button("Notes") { app.notes?.show() }
            .keyboardShortcut("n", modifiers: .command)
```

Dans `macos/Sources/AppDelegate.swift` :

1. Retirer la propriété `notes` et son commentaire (« Les pages de notes ; … »), le commentaire de `launchedAtLogin` devient « Lancé par macOS à l'ouverture de session : rien ne s'impose. ».
2. Retirer `notesClosed()` et `hidesPanelWhenNotesClose(playing:)` en entier.
3. Remplacer `applicationShouldHandleReopen` par :

```swift
    /// sillon déjà lancé, rouvert depuis le Dock, le Finder, Spotlight ou un
    /// lanceur : le lecteur.
    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        guard let model else { return false }
        model.refresh()
        present(LaunchKind.reopenPresentation(hasDocument: model.snapshot.hasDocument))
        return false
    }

    /// Montre ce que décide `LaunchKind`. Le lecteur vide s'ouvre au mode
    /// moyen ; la dernière taille, par « Afficher ».
    private func present(_ presentation: LaunchPresentation) {
        guard let panel else { return }
        switch presentation {
        case .nothing: break
        case .emptyReader: panel.apply(.choose(.medium))
        case .lastSize: panel.show()
        }
    }
```

4. Retirer `applicationWillTerminate(_:)` en entier (seules les notes s'y écrivaient).
5. Dans `applicationDidFinishLaunching`, retirer le bloc `let notesDirectory = …` jusqu'à `}, closed: { [weak self] in self?.notesClosed() })` et la ligne `services.notes = notes`.
6. Remplacer le bloc `if isUITesting { … }` par :

```swift
        if isUITesting {
            let arguments = ProcessInfo.processInfo.arguments
            // `--ui-testing-vide` : rien d'ouvert, comme un premier lancement.
            if !arguments.contains("--ui-testing-vide") { model.open(text: Self.sample, source: "exemple") }
            if arguments.contains("--show-panel") { panel.apply(.choose(.medium)) }
            if arguments.contains("--show-settings") {
                // L'icône de la barre des menus donne l'action dès qu'elle paraît.
                settings.open()
            }
            // Le chemin d'un lancement fait à la main, sous les tests.
            if arguments.contains("--ui-testing-lancement-manuel") {
                model.refresh()
                present(LaunchKind.presentation(atLogin: false, withFiles: false,
                                                hasDocument: model.snapshot.hasDocument))
            }
        }
```

7. Remplacer la fin de `applicationDidFinishLaunching` :

```swift
        // Lancé avec un fichier : le lecteur sur lui. Lancé à la main : le
        // lecteur. À la connexion : rien.
        if let file = pendingFiles.last {
            pendingFiles = []
            openFile(file)
        } else {
            present(LaunchKind.presentation(atLogin: launchedAtLogin, withFiles: launchedWithFiles,
                                            hasDocument: model.snapshot.hasDocument))
        }
```

`README.md`, ligne 77 : remplacer « `sillon` seul ouvre la page de notes. » par « `sillon` seul ouvre le lecteur. ».

`scripts/sillon-app.sh`, ligne 4 : remplacer `#   sillon                 # la page de notes` par `#   sillon                 # le lecteur`.

- [ ] **Step 4 : vérifier** — `grep -rn "Notes\|notes\|setActivationPolicy" macos/Sources` : attendu, aucune ligne (hors commentaires sans rapport ; il ne doit rester ni `NotesWindowController`, ni `NotesStore`, ni `setActivationPolicy`). Puis **`TEST`** `/LaunchKindTests` : PASS ; puis **`TEST`** (suite entière) : PASS ; puis `shellcheck scripts/sillon-app.sh` : rien.

- [ ] **Step 5 : commit**

```bash
git add macos/Sources/App/LaunchKind.swift macos/Tests/LaunchKindTests.swift macos/Sources/AppDelegate.swift \
        macos/Sources/App/AppServices.swift macos/Sources/MenuContent.swift README.md scripts/sillon-app.sh
git commit -m "$(cat <<'EOF'
feat(macos): Notes retirées — le lancement manuel et la réouverture ouvrent le lecteur ; LaunchKind déplacé dans App/, avec la décision de ce qui s'affiche ; plus aucune icône de Dock

Les fichiers de ~/Library/Application Support/sillon/Notes/ restent sur le disque.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Tâche 2 : deux formes — le lecteur et le mini

`DisplayMode` passe de `small / medium / large / background` à `mini / reader / hidden`. ⌘M bascule (animé), fermer met en pause et ne montre jamais le mini, toute ouverture rend la dernière taille, le lecteur retient sa taille et sa place. Le contenu du lecteur reste celui de l'ancien grand mode (`ExpandedReader`) jusqu'à la Tâche 3.

**Files:**
- Modify : `macos/Sources/Panel/DisplayMode.swift` (réécrit), `macos/Sources/Panel/DisplayModeStore.swift` (réécrit), `macos/Sources/Panel/PanelPlacement.swift` (réécrit), `macos/Sources/Panel/PanelController.swift` (réécrit), `macos/Sources/Panel/ReadingPanel.swift:3-25`, `macos/Sources/Panel/PanelContent.swift:49-123`, `macos/Sources/Input/KeyMap.swift:44-46,55-71`, `macos/Sources/MenuContent.swift:21-32`, `macos/Sources/AppDelegate.swift`, `macos/Sources/Panel/PanelAnnouncements.swift` (paramètre `background`)
- Create : `macos/Sources/Panel/WindowPlacements.swift`, `macos/Tests/WindowPlacementsTests.swift`, `macos/UITests/ReaderModesUITests.swift`
- Delete : `macos/Tests/PanelGeometryTests.swift`, `macos/UITests/DisplayModeUITests.swift`, `macos/UITests/PanelUITests.swift`
- Test : `macos/Tests/DisplayModeTests.swift` (réécrit), `macos/Tests/DisplayModeStoreTests.swift` (réécrit), `macos/Tests/PanelControllerTests.swift`, `macos/Tests/KeyMapTests.swift`, `macos/Tests/HelpOverlayTests.swift`, `macos/Tests/MiniPlayerTests.swift`, `macos/UITests/ExpandedUITests.swift`, `macos/UITests/AccessibilityAuditUITests.swift`

**Interfaces:**
- Consumes : `LaunchPresentation` (Tâche 1).
- Produces :
  - `enum DisplayMode: String { case mini, reader, hidden; var isVisible: Bool; static func stored(_ raw: String) -> DisplayMode? }`
  - `enum DisplayEvent: Equatable { case choose(DisplayMode), show, toggleVisibility, toggleSize, close }`
  - `struct DisplayState { current, lastVisible; static let initial; func applying(_:) -> DisplayState; func hides(_ next: DisplayState) -> Bool; var relaunched: DisplayState }`
  - `DisplayModeStore.state`, `.miniOrigin: CGPoint?` (clé inchangée `affichage.petit.origine`), `.readerFrame: CGRect?` (clé `affichage.lecteur.cadre`)
  - `ScreenFit`, `MiniPlacement.size = 340 × 88`, `MiniPlacement.frame(savedOrigin:screens:main:)`, `ReaderPlacement.minimumSize`, `ReaderPlacement.defaultFrame(on:)`, `ReaderPlacement.frame(saved:screens:main:)`
  - `PanelController.apply(_:)`, `show()`, `toggleVisibility()`, `toggleSize()`, `close()`, `finishEditing(resume: Bool = true)`, `mode: DisplayMode`
  - `ReaderAction` : `.expand` et `.display(_)` retirés ; `.toggleSize` (⌘M), `.close` (⌘W) ajoutés
  - identifiants d'accessibilité de la fenêtre : `"lecteur"` (« Lecteur »), `"lecteur-mini"` (« Lecteur réduit »)

- [ ] **Step 1 : écrire les tests purs qui échouent.**

`macos/Tests/DisplayModeTests.swift` (remplace le fichier) :

```swift
import XCTest
@testable import Sillon

final class DisplayModeTests: XCTestCase {
    private let start = DisplayState.initial

    func testAtFirstNothingIsShownAndTheReaderComesBack() {
        XCTAssertEqual(start.current, .hidden)
        XCTAssertEqual(start.lastVisible, .reader)
    }

    /// ⌃⌥Espace, ⌘V, un fichier, « Afficher le lecteur » : la dernière
    /// taille ; une forme visible ne change pas.
    func testShowGivesTheLastSizeAndKeepsAVisibleForm() {
        XCTAssertEqual(start.applying(.show).current, .reader)
        let mini = start.applying(.choose(.mini))
        XCTAssertEqual(mini.applying(.show), mini)
        XCTAssertEqual(mini.applying(.close).applying(.show).current, .mini)
    }

    /// ⌘M : du lecteur au mini et retour ; masqué, rien.
    func testToggleSizeSwitchesBetweenReaderAndMini() {
        let reader = start.applying(.show)
        XCTAssertEqual(reader.applying(.toggleSize).current, .mini)
        XCTAssertEqual(reader.applying(.toggleSize).lastVisible, .mini)
        XCTAssertEqual(reader.applying(.toggleSize).applying(.toggleSize).current, .reader)
        XCTAssertEqual(start.applying(.toggleSize), start)
    }

    /// Fermer : masqué, jamais le mini ; la taille fermée revient ensuite.
    func testClosingNeverLeavesTheMini() {
        let closed = start.applying(.show).applying(.close)
        XCTAssertEqual(closed.current, .hidden)
        XCTAssertEqual(closed.lastVisible, .reader)
        XCTAssertEqual(closed.applying(.close), closed)
    }

    /// ⌃⌥S : affiche la dernière taille, ou masque.
    func testToggleVisibilityShowsOrHides() {
        let mini = start.applying(.choose(.mini))
        let hidden = mini.applying(.toggleVisibility)
        XCTAssertEqual(hidden.current, .hidden)
        XCTAssertEqual(hidden.applying(.toggleVisibility).current, .mini)
    }

    /// Fermer et ⌃⌥S qui masque arrêtent la lecture ; réduire et agrandir non.
    func testOnlyHidingStopsTheReading() {
        let reader = start.applying(.show)
        XCTAssertTrue(reader.hides(reader.applying(.close)))
        XCTAssertTrue(reader.hides(reader.applying(.toggleVisibility)))
        XCTAssertFalse(reader.hides(reader.applying(.toggleSize)))
        XCTAssertFalse(start.hides(start.applying(.close)), "déjà masqué")
    }

    /// Le dernier mode visible n'est jamais « masqué ».
    func testLastVisibleIsNeverHidden() {
        let s = DisplayState(current: .hidden, lastVisible: .hidden)
        XCTAssertEqual(s.lastVisible, .reader)
        XCTAssertEqual(start.applying(.choose(.hidden)).lastVisible, .reader)
    }

    /// Une relance ne montre rien et garde la dernière taille.
    func testARelaunchShowsNothingButKeepsTheLastSize() {
        let mini = start.applying(.choose(.mini)).relaunched
        XCTAssertEqual(mini.current, .hidden)
        XCTAssertEqual(mini.lastVisible, .mini)
    }

    /// Les valeurs retenues par la version précédente se traduisent.
    func testModesStoredBeforeTheUpdateAreTranslated() {
        XCTAssertEqual(DisplayMode.stored("small"), .mini)
        XCTAssertEqual(DisplayMode.stored("medium"), .reader)
        XCTAssertEqual(DisplayMode.stored("large"), .reader)
        XCTAssertEqual(DisplayMode.stored("background"), .hidden)
        XCTAssertEqual(DisplayMode.stored("mini"), .mini)
        XCTAssertNil(DisplayMode.stored("géant"))
    }
}
```

`macos/Tests/DisplayModeStoreTests.swift` (remplace le fichier ; les tests de `SmallPlacement` partent dans `WindowPlacementsTests`) :

```swift
import XCTest
@testable import Sillon

final class DisplayModeStoreTests: XCTestCase {
    /// Des préférences à part, vidées : jamais celles de l'utilisateur.
    private func freshDefaults() -> UserDefaults {
        let name = "sillon.tests.affichage.magasin"
        let defaults = UserDefaults(suiteName: name)!
        defaults.removePersistentDomain(forName: name)
        return defaults
    }

    func testAFreshStoreStartsHidden() {
        let store = DisplayModeStore(defaults: freshDefaults())
        XCTAssertEqual(store.state, .initial)
        XCTAssertNil(store.miniOrigin)
        XCTAssertNil(store.readerFrame)
    }

    func testTheLastSizeSurvivesARelaunch() {
        let defaults = freshDefaults()
        let state = DisplayState.initial.applying(.choose(.mini)).applying(.close)
        DisplayModeStore(defaults: defaults).state = state
        XCTAssertEqual(DisplayModeStore(defaults: defaults).state, state)
        XCTAssertEqual(DisplayModeStore(defaults: defaults).state.lastVisible, .mini)
    }

    /// Préférences de la version précédente : traduites, pas perdues.
    func testModesStoredBeforeTheUpdateAreTranslated() {
        let defaults = freshDefaults()
        defaults.set("background", forKey: DisplayModeStore.currentKey)
        defaults.set("small", forKey: DisplayModeStore.lastVisibleKey)
        XCTAssertEqual(DisplayModeStore(defaults: defaults).state,
                       DisplayState(current: .hidden, lastVisible: .mini))
        defaults.set("large", forKey: DisplayModeStore.currentKey)
        XCTAssertEqual(DisplayModeStore(defaults: defaults).state.current, .reader)
    }

    func testUnreadableValuesFallBackToTheDefaults() {
        let defaults = freshDefaults()
        defaults.set("géant", forKey: DisplayModeStore.currentKey)
        defaults.set("hidden", forKey: DisplayModeStore.lastVisibleKey)
        XCTAssertEqual(DisplayModeStore(defaults: defaults).state, .initial)
    }

    func testTheMiniPlaceSurvivesARelaunch() {
        let defaults = freshDefaults()
        DisplayModeStore(defaults: defaults).miniOrigin = CGPoint(x: 120, y: 640)
        XCTAssertEqual(DisplayModeStore(defaults: defaults).miniOrigin, CGPoint(x: 120, y: 640))
        defaults.set(["a"], forKey: DisplayModeStore.miniOriginKey)
        XCTAssertNil(DisplayModeStore(defaults: defaults).miniOrigin, "valeur illisible : pas de place")
    }

    func testTheReaderFrameSurvivesARelaunch() {
        let defaults = freshDefaults()
        let frame = CGRect(x: 100, y: 200, width: 700, height: 480)
        DisplayModeStore(defaults: defaults).readerFrame = frame
        XCTAssertEqual(DisplayModeStore(defaults: defaults).readerFrame, frame)
        defaults.set([1, 2, Double.nan, 4], forKey: DisplayModeStore.readerFrameKey)
        XCTAssertNil(DisplayModeStore(defaults: defaults).readerFrame, "valeur illisible : pas de cadre")
        let store = DisplayModeStore(defaults: defaults)
        store.readerFrame = nil
        XCTAssertNil(DisplayModeStore(defaults: defaults).readerFrame)
    }

    /// Sous les tests, la forme ne va jamais dans les préférences réelles.
    func testUnderTestsTheModeIsNotStoredInTheUserDefaults() {
        let defaults = DisplayModeStore.processDefaults()
        XCTAssertFalse(defaults === UserDefaults.standard)
        let before = UserDefaults.standard.string(forKey: DisplayModeStore.currentKey)
        DisplayModeStore(defaults: defaults).state = DisplayState(current: .reader, lastVisible: .reader)
        XCTAssertEqual(UserDefaults.standard.string(forKey: DisplayModeStore.currentKey), before)
    }
}
```

`macos/Tests/WindowPlacementsTests.swift` (nouveau ; la classe `SmallPlacementTests` de l'ancien `DisplayModeStoreTests.swift` y passe **telle quelle** en remplaçant `SmallPlacement` par `MiniPlacement` et le nom de classe par `MiniPlacementTests` ; puis, à la suite, la classe du lecteur) :

```swift
final class ReaderPlacementTests: XCTestCase {
    private let main = CGRect(x: 0, y: 0, width: 1440, height: 875)
    private let side = CGRect(x: 1440, y: 0, width: 1920, height: 1055)

    func testWithoutASavedFrameTheReaderSitsOnTheMainScreen() {
        let frame = ReaderPlacement.frame(saved: nil, screens: [main, side], main: main)
        XCTAssertEqual(frame, ReaderPlacement.defaultFrame(on: main))
        XCTAssertTrue(main.contains(frame))
        XCTAssertEqual(frame.midX, main.midX, accuracy: 1)
        XCTAssertLessThanOrEqual(frame.width, PanelGeometry.maximumWidth)
    }

    func testASavedFrameOnAScreenIsKept() {
        let saved = CGRect(x: 1600, y: 300, width: 900, height: 600)
        XCTAssertEqual(ReaderPlacement.frame(saved: saved, screens: [main, side], main: main), saved)
    }

    func testAReaderFrameOffEveryScreenComesBackOnTheMainScreen() {
        let saved = CGRect(x: 1600, y: 300, width: 900, height: 600)
        XCTAssertEqual(ReaderPlacement.frame(saved: saved, screens: [main], main: main),
                       ReaderPlacement.defaultFrame(on: main))
    }

    func testAReaderFrameLargerThanTheScreenIsShrunk() {
        let saved = CGRect(x: -100, y: -100, width: 5000, height: 3000)
        let frame = ReaderPlacement.frame(saved: saved, screens: [main], main: main)
        XCTAssertEqual(frame, main)
    }

    func testATinyFrameGrowsToTheMinimum() {
        let saved = CGRect(x: 100, y: 100, width: 50, height: 40)
        let frame = ReaderPlacement.frame(saved: saved, screens: [main], main: main)
        XCTAssertEqual(frame.size, ReaderPlacement.minimumSize)
        XCTAssertTrue(main.contains(frame))
    }
}
```

(Le fichier commence par `import XCTest` et `@testable import Sillon`.)

- [ ] **Step 2 : vérifier l'échec** — **`TEST`** `/DisplayModeTests`. Attendu : échec de compilation (« type 'DisplayMode' has no member 'mini' »).

- [ ] **Step 3 : implémenter les types purs.**

`macos/Sources/Panel/DisplayMode.swift` (remplace le fichier) :

```swift
/// Les deux formes de sillon, et « masqué ». Une seule fenêtre : le
/// lecteur (texte, en-tête, barre) ou le mini (une barre flottante).
enum DisplayMode: String, Equatable, CaseIterable {
    case mini, reader, hidden

    var isVisible: Bool { self != .hidden }

    /// Une valeur retenue, y compris celles de la version précédente
    /// (petit, moyen, grand, arrière-plan) ; nil si elle est illisible.
    static func stored(_ raw: String) -> DisplayMode? {
        if let mode = DisplayMode(rawValue: raw) { return mode }
        switch raw {
        case "small": return .mini
        case "medium", "large": return .reader
        case "background": return .hidden
        default: return nil
        }
    }
}

/// Ce qui fait changer de forme.
enum DisplayEvent: Equatable {
    /// Une forme précise : le lecteur vide au lancement manuel, l'édition,
    /// la recherche ou l'aide demandées depuis le mini.
    case choose(DisplayMode)
    /// ⌃⌥Espace, ⌘V, un fichier, « Afficher le lecteur » : la dernière
    /// taille ; une forme visible ne change pas.
    case show
    /// ⌃⌥S : la dernière taille, ou masqué.
    case toggleVisibility
    /// ⌘M, « Réduire », « Agrandir » : du lecteur au mini et retour.
    case toggleSize
    /// ⌘W, Échap, le bouton fermer : masqué (jamais le mini).
    case close
}

/// La forme courante et la dernière forme visible — jamais « masqué », pour
/// que toute ouverture montre quelque chose. Sans état caché : testé seul.
struct DisplayState: Equatable {
    private(set) var current: DisplayMode
    private(set) var lastVisible: DisplayMode

    static let initial = DisplayState(current: .hidden, lastVisible: .reader)

    init(current: DisplayMode, lastVisible: DisplayMode) {
        self.current = current
        if current.isVisible {
            self.lastVisible = current
        } else {
            // Une dernière forme « masqué » (préférence abîmée) : le lecteur.
            self.lastVisible = lastVisible.isVisible ? lastVisible : .reader
        }
    }

    func applying(_ event: DisplayEvent) -> DisplayState {
        switch event {
        case .choose(let mode):
            return DisplayState(current: mode, lastVisible: lastVisible)
        case .show:
            return current.isVisible ? self : DisplayState(current: lastVisible, lastVisible: lastVisible)
        case .toggleVisibility:
            return current.isVisible ? DisplayState(current: .hidden, lastVisible: current)
                                     : DisplayState(current: lastVisible, lastVisible: lastVisible)
        case .toggleSize:
            switch current {
            case .mini: return DisplayState(current: .reader, lastVisible: .reader)
            case .reader: return DisplayState(current: .mini, lastVisible: .mini)
            case .hidden: return self
            }
        case .close:
            return DisplayState(current: .hidden, lastVisible: current.isVisible ? current : lastVisible)
        }
    }

    /// La fenêtre disparaît : la lecture s'arrête.
    func hides(_ next: DisplayState) -> Bool {
        current.isVisible && !next.current.isVisible
    }

    /// Après un lancement, rien n'est montré ; la dernière taille reste.
    var relaunched: DisplayState {
        DisplayState(current: .hidden, lastVisible: lastVisible)
    }
}
```

`macos/Sources/Panel/DisplayModeStore.swift` (remplace le fichier ; `SmallPlacement` part dans `WindowPlacements.swift`) :

```swift
import Foundation

/// Retient la forme courante, la dernière forme visible, la place du mini
/// et le cadre du lecteur, d'un lancement à l'autre.
final class DisplayModeStore {
    static let currentKey = "affichage.mode"
    static let lastVisibleKey = "affichage.dernierVisible"
    static let miniOriginKey = "affichage.petit.origine"
    static let readerFrameKey = "affichage.lecteur.cadre"

    private let defaults: UserDefaults

    init(defaults: UserDefaults) {
        self.defaults = defaults
    }

    /// Une valeur illisible retombe sur l'état initial (masqué, lecteur) ;
    /// celles de la version précédente se traduisent (`DisplayMode.stored`).
    var state: DisplayState {
        get {
            let current = defaults.string(forKey: Self.currentKey).flatMap(DisplayMode.stored)
                ?? DisplayState.initial.current
            let last = defaults.string(forKey: Self.lastVisibleKey).flatMap(DisplayMode.stored)
                ?? DisplayState.initial.lastVisible
            return DisplayState(current: current, lastVisible: last)
        }
        set {
            defaults.set(newValue.current.rawValue, forKey: Self.currentKey)
            defaults.set(newValue.lastVisible.rawValue, forKey: Self.lastVisibleKey)
        }
    }

    /// Le coin bas gauche du mini, là où l'utilisateur l'a laissé ; nil
    /// tant qu'il ne l'a pas déplacé.
    var miniOrigin: CGPoint? {
        get {
            guard let xy = numbers(Self.miniOriginKey, count: 2) else { return nil }
            return CGPoint(x: xy[0], y: xy[1])
        }
        set {
            if let p = newValue {
                defaults.set([Double(p.x), Double(p.y)], forKey: Self.miniOriginKey)
            } else {
                defaults.removeObject(forKey: Self.miniOriginKey)
            }
        }
    }

    /// Le cadre du lecteur tel que l'utilisateur l'a laissé ; nil tant
    /// qu'il ne l'a ni déplacé ni redimensionné.
    var readerFrame: CGRect? {
        get {
            guard let f = numbers(Self.readerFrameKey, count: 4) else { return nil }
            return CGRect(x: f[0], y: f[1], width: f[2], height: f[3])
        }
        set {
            if let r = newValue {
                defaults.set([Double(r.minX), Double(r.minY), Double(r.width), Double(r.height)],
                             forKey: Self.readerFrameKey)
            } else {
                defaults.removeObject(forKey: Self.readerFrameKey)
            }
        }
    }

    private func numbers(_ key: String, count: Int) -> [Double]? {
        guard let values = defaults.array(forKey: key) as? [Double], values.count == count,
              values.allSatisfy(\.isFinite) else { return nil }
        return values
    }

    /// Les préférences de l'utilisateur, sauf sous les tests (unitaires ou
    /// d'interface) : une suite à part, vidée à chaque appel.
    static func processDefaults() -> UserDefaults {
        let info = ProcessInfo.processInfo
        let testing = info.arguments.contains("--ui-testing")
            || info.environment["XCTestConfigurationFilePath"] != nil
        guard testing else { return .standard }
        let name = "fr.sillon.tests.affichage"
        guard let suite = UserDefaults(suiteName: name) else { return .standard }
        suite.removePersistentDomain(forName: name)
        return suite
    }
}
```

`macos/Sources/Panel/WindowPlacements.swift` (nouveau) :

```swift
import CoreGraphics

/// Un cadre ramené sur un écran visible.
enum ScreenFit {
    /// L'écran qui montre la plus grande part de `wanted`, s'il en montre un bout.
    static func bestScreen(for wanted: CGRect, among screens: [CGRect]) -> CGRect? {
        let best = screens.max { area($0.intersection(wanted)) < area($1.intersection(wanted)) }
        guard let best, area(best.intersection(wanted)) > 0 else { return nil }
        return best
    }

    /// `r` déplacé (pas redimensionné) pour tenir dans `s`.
    static func clamp(_ r: CGRect, into s: CGRect) -> CGRect {
        CGRect(x: min(max(r.minX, s.minX), s.maxX - r.width),
               y: min(max(r.minY, s.minY), s.maxY - r.height),
               width: r.width, height: r.height)
    }

    static func area(_ r: CGRect) -> CGFloat {
        r.isNull ? 0 : r.width * r.height
    }
}

/// Le mini : taille fixe, place retenue, toujours sur un écran visible.
enum MiniPlacement {
    static let size = CGSize(width: 340, height: 88)

    /// La place retenue si un écran en montre au moins un bout (ramenée
    /// entière sur l'écran qui en montre le plus) ; sinon la place par
    /// défaut sur l'écran principal.
    static func frame(savedOrigin: CGPoint?, screens: [CGRect], main: CGRect) -> CGRect {
        if let origin = savedOrigin {
            let wanted = CGRect(origin: origin, size: size)
            if let screen = ScreenFit.bestScreen(for: wanted, among: screens) {
                return ScreenFit.clamp(wanted, into: screen)
            }
        }
        return defaultFrame(on: main)
    }

    /// Centré, le haut au quart de l'écran.
    static func defaultFrame(on screen: CGRect) -> CGRect {
        let top = screen.maxY - screen.height * 0.25
        return CGRect(x: screen.midX - size.width / 2, y: top - size.height,
                      width: size.width, height: size.height)
    }
}

/// Le lecteur : redimensionnable, cadre retenu, toujours entier sur un écran.
enum ReaderPlacement {
    static let minimumSize = CGSize(width: 420, height: 260)

    /// Centré, le haut au cinquième de l'écran, 760 pt de large au plus.
    static func defaultFrame(on screen: CGRect) -> CGRect {
        let width = min(PanelGeometry.maximumWidth, screen.width * 0.6)
        let height = min(560, screen.height * 0.6)
        let top = screen.maxY - screen.height * 0.2
        return CGRect(x: screen.midX - width / 2, y: top - height, width: width, height: height)
    }

    /// Le cadre retenu, ramené sur l'écran qui en montre le plus, à une
    /// taille comprise entre le minimum et cet écran ; sinon le défaut.
    static func frame(saved: CGRect?, screens: [CGRect], main: CGRect) -> CGRect {
        guard let saved, saved.width > 0, saved.height > 0,
              let screen = ScreenFit.bestScreen(for: saved, among: screens) else {
            return defaultFrame(on: main)
        }
        let size = CGSize(width: min(max(saved.width, minimumSize.width), screen.width),
                          height: min(max(saved.height, minimumSize.height), screen.height))
        return ScreenFit.clamp(CGRect(origin: saved.origin, size: size), into: screen)
    }
}
```

Dans `macos/Sources/Panel/ReadingPanel.swift`, remplacer `enum PanelGeometry { … }` (lignes 3-25) par :

```swift
/// Les mesures du lecteur.
enum PanelGeometry {
    static let maximumWidth: CGFloat = 760
    /// La marge de chaque côté du contenu.
    static let horizontalPadding: CGFloat = 22
}
```

`git rm macos/Tests/PanelGeometryTests.swift`.

- [ ] **Step 4 : vérifier** — **`TEST`** `/DisplayModeTests` ne compile pas encore (le contrôleur utilise les anciens cas) : c'est attendu, passer à l'étape 5 avant de lancer quoi que ce soit.

- [ ] **Step 5 : écrire les tests du contrôleur qui échouent.**

Dans `macos/Tests/PanelControllerTests.swift` :

1. `tearDown` : remplacer les deux `controller?.collapseOrHide()` par un seul `controller?.close()`.
2. Supprimer `maxHeight`, `testShortTextGivesAPanelThatFitsTheText`, `testLongTextGivesTheMaximumHeight`, `testCollapsingFromExpandedMeasuresAgainAtPanelWidth`, `testClosingTheWindowGoesToTheBackgroundWithoutPausing`, `testCommandDigitsGiveEachModeItsSize`, `testEscapeGoesFromLargeToMediumThenToTheMiniPlayerWithoutPausing`, `testEscapeFromTheSmallBarGoesToTheBackground`, `testShowBringsBackTheLastVisibleMode`, `testReadingASelectionKeepsTheModeOrOpensTheMediumOne`, `testTheSmallBarKeepsItsPlaceFromOneTimeToTheNext`, `testASmallBarLeftOnAnUnpluggedScreenComesBackToTheMainScreen`, `testTheModeComesBackAfterARelaunchButNotTheBackground`. `testShiftArrowUsesTheSeekStepOfTheSettings` reste jusqu'à la Tâche 5.
3. Dans `testNoAnnouncementWhenHiddenNorOnShowAfterHide`, remplacer le bloc depuis `// Pendant la lecture, la fermeture laisse la barre ;` jusqu'à la deuxième assertion `XCTAssertNil(c.announcementText(for: c.model.snapshot), "rien en arrière-plan")` par :

```swift
        // Fermer met en pause et ne dit rien de ce qui suit.
        c.close()
        XCTAssertEqual(c.mode, .hidden)
        XCTAssertTrue(waitUntil { c.model.refresh(); return !c.model.snapshot.playing })
        XCTAssertNil(c.announcementText(for: c.model.snapshot), "rien fenêtre masquée")
```

et le message suivant `"rien dit en arrière-plan ni à la réouverture"` par `"rien dit fenêtre masquée ni à la réouverture"`.
4. Remplacements mécaniques dans le reste du fichier : `c.perform(.display(.small))` → `c.perform(.toggleSize)` ; `c.collapseOrHide()` → `c.close()` ; `.background` → `.hidden` ; `.small` (mode) → `.mini` ; `c.expand()` → `c.perform(.toggleSize)` ; dans `testModesKeepThePanelUnderTheOpenSettings`, la boucle devient `for _ in 0..<3 { c.perform(.toggleSize); settle(c); XCTAssertEqual(c.window.level, .normal) }` ; dans `testThePlayingCadenceFollowsTheMode`, remplacer les quatre lignes qui suivent `c.collapseOrHide()` (cadence `.second`) par :

```swift
        c.close()
        XCTAssertEqual(c.model.cadence, .none, "fermer met en pause : plus rien ne tourne")
        XCTAssertFalse(frames.isRunning)
        // Les touches média relancent la voix fenêtre masquée : la seconde.
        c.model.send(.togglePlay)
        XCTAssertTrue(waitUntil { c.model.refresh(); return c.model.snapshot.playing })
        XCTAssertEqual(c.model.cadence, .second)
        XCTAssertFalse(frames.isRunning)
```

(et supprimer le `RunLoop.main.run(… gracePeriod …)` et les trois assertions qui le suivaient) ; dans `testGoingToTheBackgroundCancelsAnActiveSearch` (renommer `testClosingCancelsAnActiveSearch`), un seul `c.close()` ; dans `testTheBackgroundKeepsNothingDisplayed` (renommer `testAHiddenWindowKeepsNothingDisplayed`), `c.collapseOrHide()` → `c.close()`.
5. Ajouter, avant `testThePanelStepsBackWhileTheSettingsAreOpen`, les tests des deux formes :

```swift
    private var mainScreen: CGRect { NSScreen.screens[0].visibleFrame }

    /// Le lecteur à sa place ; ⌘M : le mini, à sa taille fixe, puis retour.
    func testCommandMSwitchesBetweenReaderAndMiniWithoutPausing() {
        let c = shownPanel("Un. Deux.")
        XCTAssertEqual(c.mode, .reader)
        XCTAssertEqual(c.window.frame, ReaderPlacement.defaultFrame(on: mainScreen))
        XCTAssertEqual(c.window.contentView?.accessibilityIdentifier(), "lecteur")
        XCTAssertTrue(c.window.styleMask.contains(.resizable))
        c.perform(.command(.togglePlay))
        XCTAssertTrue(waitUntil { c.model.refresh(); return c.model.snapshot.playing })
        c.perform(.toggleSize)
        settle(c)
        XCTAssertEqual(c.mode, .mini)
        XCTAssertEqual(c.window.frame.size, MiniPlacement.size)
        XCTAssertEqual(c.window.contentView?.accessibilityIdentifier(), "lecteur-mini")
        XCTAssertFalse(c.window.styleMask.contains(.resizable), "le mini a une taille fixe")
        c.perform(.toggleSize)
        settle(c)
        XCTAssertEqual(c.mode, .reader)
        XCTAssertTrue(c.model.snapshot.playing, "réduire et agrandir ne touchent pas à la lecture")
        c.model.send(.pause)
    }

    /// ⌘W, Échap ou la fermeture de la fenêtre : masqué et en pause, jamais le mini.
    func testClosingPausesAndNeverLeavesTheMini() {
        for close in [{ (c: PanelController) in c.perform(.close) },
                      { (c: PanelController) in c.perform(.collapseOrHide) },
                      { (c: PanelController) in c.window.performClose(nil) }] {
            let c = shownPanel("Un. Deux.")
            c.sidebarVisible = false
            c.perform(.command(.togglePlay))
            XCTAssertTrue(waitUntil { c.model.refresh(); return c.model.snapshot.playing })
            close(c)
            XCTAssertEqual(c.mode, .hidden)
            XCTAssertFalse(c.window.isVisible)
            XCTAssertEqual(c.model.presence, .hidden)
            XCTAssertTrue(waitUntil { c.model.refresh(); return !c.model.snapshot.playing }, "la lecture continue")
            XCTAssertEqual(c.store.state.current, .hidden)
        }
    }

    /// Le document et la position restent : la réouverture reprend là.
    func testReopeningAfterClosingKeepsThePlace() {
        let c = shownPanel("Un. Deux. Trois.")
        c.perform(.command(.sentences(delta: 2)))
        XCTAssertTrue(waitUntil { c.model.refresh(); return c.model.snapshot.sentence == 2 })
        c.close()
        c.show()
        settle(c)
        XCTAssertEqual(c.mode, .reader)
        XCTAssertEqual(c.model.snapshot.sentence, 2)
    }

    /// Toute ouverture rend la dernière taille : le mini fermé revient mini.
    func testShowGivesTheLastSize() {
        let c = shownPanel("Un. Deux.")
        c.perform(.toggleSize)
        settle(c)
        c.close()
        c.show()
        settle(c)
        XCTAssertEqual(c.mode, .mini)
        XCTAssertTrue(c.window.isVisible)
        // Déjà visible : « show » ne change rien.
        c.show()
        XCTAssertEqual(c.mode, .mini)
    }

    /// ⌃⌥S : affiche la dernière taille, ou masque (en pause).
    func testToggleVisibilityShowsOrHides() {
        let c = shownPanel("Un. Deux.")
        c.perform(.command(.togglePlay))
        XCTAssertTrue(waitUntil { c.model.refresh(); return c.model.snapshot.playing })
        c.toggleVisibility()
        XCTAssertEqual(c.mode, .hidden)
        XCTAssertTrue(waitUntil { c.model.refresh(); return !c.model.snapshot.playing })
        c.toggleVisibility()
        settle(c)
        XCTAssertEqual(c.mode, .reader)
        XCTAssertTrue(c.window.isVisible)
    }

    /// Une modification en cours au moment de fermer : le texte est gardé,
    /// la voix reste en pause (elle lisait avant l'édition).
    func testClosingWhileEditingKeepsTheTextAndStaysPaused() {
        let c = shownPanel("Un. Deux.")
        c.perform(.command(.togglePlay))
        XCTAssertTrue(waitUntil { c.model.refresh(); return c.model.snapshot.playing })
        c.beginEditing()
        XCTAssertTrue(c.editing)
        c.draft = "Trois. Quatre."
        c.close()
        XCTAssertFalse(c.editing)
        XCTAssertTrue(waitUntil { c.model.refresh(); return c.model.editableText == "Trois. Quatre." })
        RunLoop.main.run(until: Date().addingTimeInterval(0.5))
        c.model.refresh()
        XCTAssertFalse(c.model.snapshot.playing, "la reprise différée ne relance pas la voix")
    }

    func testTheMiniKeepsItsPlaceFromOneTimeToTheNext() {
        let defaults = freshDefaults()
        let c = shownPanel("Un. Deux.", defaults: defaults)
        c.perform(.toggleSize)
        settle(c)
        // Placé par le code (animation comprise) : rien n'est retenu.
        XCTAssertNil(c.store.miniOrigin)
        let spot = CGPoint(x: mainScreen.minX + 40, y: mainScreen.minY + 40)
        c.window.setFrameOrigin(spot)
        XCTAssertTrue(waitUntil { c.store.miniOrigin == spot }, "le déplacement n'est pas retenu")
        c.close()
        // Relance : un nouveau contrôleur, mêmes préférences ; rien ne s'affiche
        // avant qu'on le demande, puis le mini, à sa place.
        let again = PanelController(model: c.model, store: DisplayModeStore(defaults: defaults))
        controller = again
        XCTAssertEqual(again.store.state.current, .hidden)
        again.show()
        settle(again)
        XCTAssertEqual(again.mode, .mini)
        XCTAssertEqual(again.window.frame.origin.x, spot.x, accuracy: 1)
        XCTAssertEqual(again.window.frame.origin.y, spot.y, accuracy: 1)
    }

    func testAMiniLeftOnAnUnpluggedScreenComesBackToTheMainScreen() {
        let defaults = freshDefaults()
        DisplayModeStore(defaults: defaults).miniOrigin = CGPoint(x: -50_000, y: -50_000)
        let c = shownPanel("Un.", defaults: defaults)
        c.perform(.toggleSize)
        settle(c)
        XCTAssertTrue(mainScreen.contains(c.window.frame), "mini hors écran : \(c.window.frame)")
    }

    /// Le lecteur redimensionné et déplacé par l'utilisateur garde ce cadre.
    func testTheReaderKeepsItsFrameFromOneTimeToTheNext() {
        let defaults = freshDefaults()
        let c = shownPanel("Un. Deux.", defaults: defaults)
        let wanted = CGRect(x: mainScreen.minX + 30, y: mainScreen.minY + 30, width: 600, height: 400)
        c.window.setFrame(wanted, display: true)
        XCTAssertTrue(waitUntil { c.store.readerFrame == wanted }, "le cadre n'est pas retenu")
        c.perform(.toggleSize)
        settle(c)
        XCTAssertEqual(c.store.readerFrame, wanted, "le passage au mini ne compte pas")
        c.perform(.toggleSize)
        settle(c)
        XCTAssertEqual(c.window.frame, wanted)
    }
```

6. Remplacer `testLinesFollowTheTextEvenInTheSmallBar` : seul changement, `c.perform(.display(.small))` → `c.perform(.toggleSize)` et le nom → `testLinesFollowTheTextEvenInTheMini`.

Dans `macos/Tests/KeyMapTests.swift`, remplacer `testCommandDigitsChooseTheDisplayMode` par :

```swift
    func testCommandMAndCommandWChangeTheForm() {
        XCTAssertEqual(act("m", command: true), .toggleSize)
        XCTAssertEqual(act("w", command: true), .close)
        XCTAssertNil(act("1", command: true), "⌘1 ⌘2 ⌘3 retirés")
        XCTAssertNil(act("", KeyCode.enter), "⏎ n'agrandit plus")
    }
```

et retirer, dans le reste du fichier, toute assertion sur `.expand` ou `.display(`.

Dans `macos/Tests/HelpOverlayTests.swift`, supprimer `HelpOverlayTests` et `HelpOverlayKeysTests` (l'aide est réécrite à la Tâche 5) ; garder `MenuOpeningTests`.

Dans `macos/Tests/MiniPlayerTests.swift`, remplacer partout `SmallPlacement` par `MiniPlacement` ; `testTheBarIs340By88` devient `XCTAssertEqual(MiniPlacement.size, CGSize(width: 340, height: 88))`.

- [ ] **Step 6 : vérifier l'échec** — **`TEST`** `/PanelControllerTests`. Attendu : échec de compilation (`toggleSize`, `close`, `MiniPlacement`, `readerFrame` inconnus du contrôleur).

- [ ] **Step 7 : implémenter le placement et le contrôleur.**

`macos/Sources/Panel/PanelPlacement.swift` (remplace le fichier) :

```swift
import AppKit

/// Où se pose la fenêtre, forme par forme, et ce qui en est retenu : la
/// place du mini quand l'utilisateur le déplace, le cadre du lecteur quand
/// il le déplace ou le redimensionne.
@MainActor
final class PanelPlacement {
    private let window: NSWindow
    private let store: DisplayModeStore
    /// La forme affichée, lue à chaque changement.
    private let mode: () -> DisplayMode
    /// Vrai pendant que le code place la fenêtre : seul un geste de
    /// l'utilisateur est retenu.
    private var placing = false
    private var observers: [NSObjectProtocol] = []

    init(window: NSWindow, store: DisplayModeStore, mode: @escaping () -> DisplayMode) {
        self.window = window
        self.store = store
        self.mode = mode
        observeChanges()
    }

    private var screens: [CGRect] { NSScreen.screens.map(\.visibleFrame) }

    /// L'écran principal (celui de la barre des menus).
    private var primaryScreenFrame: CGRect {
        NSScreen.screens.first?.visibleFrame ?? NSScreen.main?.visibleFrame ?? .zero
    }

    func frame(for mode: DisplayMode) -> CGRect {
        switch mode {
        case .mini:
            return MiniPlacement.frame(savedOrigin: store.miniOrigin, screens: screens, main: primaryScreenFrame)
        case .reader, .hidden:
            return ReaderPlacement.frame(saved: store.readerFrame, screens: screens, main: primaryScreenFrame)
        }
    }

    /// Le mini ne se redimensionne pas ; le lecteur, si.
    func applySizeLimits(for mode: DisplayMode) {
        if mode == .mini {
            window.styleMask.remove(.resizable)
        } else {
            window.styleMask.insert(.resizable)
        }
    }

    /// Animé, le cadre glisse d'une forme à l'autre (sauf « Réduire les
    /// animations »).
    func place(_ frame: CGRect, animated: Bool) {
        placing = true
        defer { placing = false }
        if animated, !NSWorkspace.shared.accessibilityDisplayShouldReduceMotion {
            window.setFrame(frame, display: true, animate: true)
        } else {
            window.setFrame(frame, display: animated)
        }
    }

    private func observeChanges() {
        for name in [NSWindow.didMoveNotification, NSWindow.didResizeNotification] {
            // Livré tout de suite (`queue: nil`), pendant que `placing` est
            // vrai : une file différée verrait chaque placement du code comme
            // un geste de l'utilisateur.
            observers.append(NotificationCenter.default.addObserver(
                forName: name, object: window, queue: nil
            ) { [weak self] _ in
                MainActor.assumeIsolated { self?.windowChanged() }
            })
        }
    }

    /// Un geste de l'utilisateur : la place du mini (à sa taille seulement,
    /// un redimensionnement différé de la vue hôte ne compte pas), ou le
    /// cadre du lecteur.
    func windowChanged() {
        guard !placing else { return }
        switch mode() {
        case .mini:
            if window.frame.size == MiniPlacement.size { store.miniOrigin = window.frame.origin }
        case .reader:
            store.readerFrame = window.frame
        case .hidden:
            break
        }
    }
}
```

`macos/Sources/Panel/PanelController.swift` (remplace le fichier ; seul ce qui touche aux formes change, le reste est repris tel quel) :

```swift
import AppKit
import SwiftUI

/// Montre la fenêtre sous l'une de ses deux formes — le lecteur, le mini —
/// ou la retire, et exécute les actions du clavier. Une seule fenêtre :
/// chaque forme change son contenu et son cadre. Fermer met la lecture en
/// pause. Le placement (`PanelPlacement`), le clavier (`PanelKeyRouter`) et
/// VoiceOver (`PanelAnnouncements`) vivent à part.
@MainActor
@Observable
final class PanelController {
    @ObservationIgnored let model: ReaderModel
    @ObservationIgnored let window = ReadingPanel()
    /// Forme courante, dernière taille, place du mini et cadre du lecteur,
    /// retenus d'un lancement à l'autre.
    @ObservationIgnored let store: DisplayModeStore
    /// Durée d'affichage d'un message passager (« Phrase copiée »).
    @ObservationIgnored var messageDuration: TimeInterval = 2
    @ObservationIgnored private var closeGuard: PanelCloseGuard?
    @ObservationIgnored private var placement: PanelPlacement!
    @ObservationIgnored private var keys: PanelKeyRouter!
    @ObservationIgnored private var announcements: PanelAnnouncements!
    /// ↑ ↓ : la vue de lecture affichée (ou sa copie cachée du mini) dit
    /// où commence la ligne voisine.
    @ObservationIgnored let lines = LineNavigator()

    private(set) var mode: DisplayMode = .hidden
    /// Réglages de lecture du moteur, relus à chaque ouverture et à chaque
    /// réglage écrit.
    var reading = ReadingDisplay()
    var sidebarVisible = true
    var bookmarksVisible = false
    var searchActive = false
    /// Compte les « / » : chacun rend le focus au champ, même déjà ouvert.
    private(set) var openSearchRequests = 0
    var helpVisible = false
    /// Le mode édition : le texte du lecteur devient modifiable.
    private(set) var editing = false
    /// Le texte en cours de modification.
    var draft = ""
    /// Le texte au début de la modification : inchangé, rien ne se recharge.
    @ObservationIgnored private var draftOrigin = ""
    @ObservationIgnored private var resumeAfterEditing = false

    /// Ouvre la fenêtre des réglages.
    @ObservationIgnored let openSettings: @MainActor (SettingsTab?) -> Void
    /// La même, pour l'environnement des vues : une seule valeur, posée une
    /// fois, pour que SwiftUI n'y voie jamais de changement.
    @ObservationIgnored let openSettingsAction: OpenSettingsTabAction

    init(model: ReaderModel, store: DisplayModeStore? = nil,
         openSettings: @escaping @MainActor (SettingsTab?) -> Void = { _ in }) {
        self.model = model
        self.openSettings = openSettings
        openSettingsAction = OpenSettingsTabAction(open: openSettings)
        let store = store ?? DisplayModeStore(defaults: Self.processDefaults)
        // Un lancement ne montre rien ; la dernière taille reste pour la suite.
        store.state = store.state.relaunched
        self.store = store
        // Avant la vue : son corps lit le placement (largeur du texte).
        placement = PanelPlacement(window: window, store: store, mode: { [unowned self] in self.mode })
        keys = PanelKeyRouter(window: window,
                              isEditing: { [unowned self] in self.editing },
                              action: { [unowned self] in self.action(for: $0) },
                              finishEditing: { [unowned self] in self.finishEditing() },
                              perform: { [unowned self] in self.perform($0) })
        announcements = PanelAnnouncements(window: window)
        let host = NSHostingView(rootView: PanelContent(controller: self))
        window.contentView = host
        host.setAccessibilityIdentifier("lecteur")
        host.setAccessibilityLabel("Lecteur")
        // Toute fermeture (⌘W, close(), performClose:) passe par `close()`.
        let guardian = PanelCloseGuard { [weak self] in self?.close() }
        closeGuard = guardian
        window.delegate = guardian
        observeSettingsWindow()
    }

    /// Les préférences du processus, lues une fois : sous les tests, chaque
    /// appel à `processDefaults()` vide la suite.
    private static let processDefaults = DisplayModeStore.processDefaults()

    /// La largeur du texte du lecteur : la copie cachée du mini la garde,
    /// pour que ↑ ↓ visent les mêmes lignes.
    var shadowTextWidth: CGFloat {
        placement.frame(for: .reader).width - 2 * PanelGeometry.horizontalPadding
    }

    // MARK: - Formes

    /// Une transition, retenue puis appliquée. Si la fenêtre disparaît, la
    /// lecture s'arrête.
    func apply(_ event: DisplayEvent) {
        let before = store.state
        let next = before.applying(event)
        store.state = next
        if before.hides(next) { stopForClosing() }
        present(next.current)
    }

    /// ⌃⌥Espace, ⌘V, un fichier, « Afficher le lecteur » : la dernière taille.
    func show() { apply(.show) }
    /// ⌃⌥S.
    func toggleVisibility() { apply(.toggleVisibility) }
    /// ⌘M, « Réduire », « Agrandir ».
    func toggleSize() { apply(.toggleSize) }
    /// ⌘W, Échap, le bouton fermer.
    func close() { apply(.close) }

    /// Fermer arrête la lecture : une modification s'applique sans relancer
    /// la voix, puis la pause. Le document et la position restent.
    private func stopForClosing() {
        if editing { finishEditing(resume: false) }
        model.refresh()
        if model.snapshot.playing { model.send(.pause) }
    }

    private func present(_ target: DisplayMode) {
        guard target.isVisible else { hideWindow(); return }
        // Le mini n'a pas de place pour écrire : la modification s'applique avant.
        if editing, target == .mini { finishEditing() }
        // Lu hors du fil principal : s'applique dès qu'il revient.
        reloadReading()
        let wasHidden = mode == .hidden || !window.isVisible
        if wasHidden || target != mode {
            mode = target
            placement.applySizeLimits(for: target)
            // D'une forme à l'autre, le cadre glisse vers sa nouvelle place.
            placement.place(placement.frame(for: target), animated: !wasHidden)
            updateIdentifier()
        }
        window.makeKeyAndOrderFront(nil)
        // La cadence de relecture suit ce qui est montré : le texte (à
        // chaque image en lecture), ou le mini (4 Hz en lecture, rien en pause).
        model.presence = target == .mini ? .bar : .full
        // L'annonceur ne voit rien fenêtre masquée : on repart de l'état
        // courant pour ne rien annoncer de ce qui s'est passé entre-temps.
        model.refresh()
        announcements.reseed(with: model.snapshot)
        keys.install()
    }

    /// Masqué : la fenêtre se retire.
    private func hideWindow() {
        guard mode != .hidden else { return }
        if editing { finishEditing(resume: false) }
        // Une recherche en cours ne survit pas : sinon le lecteur reviendrait
        // avec un champ vide et un compteur « n / m » périmé.
        if searchActive { searchActive = false; model.send(.searchCancel) }
        helpVisible = false
        mode = .hidden
        if window.isVisible { window.orderOut(nil) }
        model.presence = .hidden
        keys.remove()
    }
```

La suite du fichier reprend l'ancien contenu depuis `// MARK: - Réglages` jusqu'à la fin, avec ces changements seulement :

- `contentHeightChanged(_:)`, `contentHeight`, `resizePending` : supprimés (le lecteur a la taille que l'utilisateur lui donne).
- `announcementText(for:)` : `background: mode == .hidden`.
- `perform(_:)` devient :

```swift
    func perform(_ action: ReaderAction) {
        switch action {
        case .command(let command): model.send(command)
        case .line(let delta): moveLine(delta)
        case .toggleSize: toggleSize()
        case .close: close()
        case .collapseOrHide:
            // Échap : la saisie se valide, puis l'aide, la recherche et le
            // sommaire se ferment, puis le lecteur.
            if editing { finishEditing() }
            else if helpVisible { helpVisible = false }
            else if searchActive { searchActive = false; model.send(.searchCancel) }
            else if sidebarVisible, mode == .reader { sidebarVisible = false }
            else { close() }
        case .copySentence:
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(model.currentSentenceText, forType: .string)
            flash("Phrase copiée")
        case .openSearch:
            if mode != .reader { apply(.choose(.reader)) }
            searchActive = true
            openSearchRequests &+= 1
        case .toggleSidebar:
            if mode != .reader {
                apply(.choose(.reader))
                sidebarVisible = true
            } else {
                sidebarVisible.toggle()
            }
        case .toggleBookmarks:
            if mode != .reader { apply(.choose(.reader)) }
            sidebarVisible = true
            bookmarksVisible.toggle()
        case .showHelp:
            // Le mini n'a pas la place : l'aide s'ouvre dans le lecteur.
            if mode == .mini {
                apply(.choose(.reader))
                helpVisible = true
            } else {
                helpVisible.toggle()
            }
        case .openSettings:
            openSettings(nil)
        case .openVoiceSettings:
            openSettings(.voice)
        case .edit:
            beginEditing()
        }
    }
```

- `beginEditing()` : remplacer `if mode == .small || mode == .background { apply(.choose(.medium)) }` par `if mode != .reader { apply(.choose(.reader)) }` et supprimer les deux dernières lignes (`// Le panneau moyen prend la place d'écrire.` et `if mode == .medium { contentHeightChanged(…) }`).
- `finishEditing()` devient :

```swift
    /// Échap, ⌘↩ ou « Terminé » : le texte modifié remplace le document, la
    /// voix reprend à la même phrase. `resume` faux (fermeture) : la voix
    /// reste en pause.
    func finishEditing(resume: Bool = true) {
        guard editing else { return }
        editing = false
        let play = resume && resumeAfterEditing
        if draft != draftOrigin, !draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            model.reopen(edited: draft, play: play)
            flash("Texte modifié")
        } else if play {
            model.send(.togglePlay)
        }
        window.makeFirstResponder(nil)
    }
```

- `updateIdentifier()` devient :

```swift
    /// L'identifiant pour les tests, le nom pour VoiceOver.
    private func updateIdentifier() {
        let mini = mode == .mini
        window.contentView?.setAccessibilityIdentifier(mini ? "lecteur-mini" : "lecteur")
        window.contentView?.setAccessibilityLabel(mini ? "Lecteur réduit" : "Lecteur")
    }
```

- Supprimer `show()` ancien, `restore()`, `expand()`, `collapseOrHide()` (remplacés plus haut).

Dans `macos/Sources/Panel/PanelAnnouncements.swift`, renommer le paramètre `background:` de `text(for:doc:background:)` en `hidden:` (et son usage interne) ; dans le contrôleur, `announcements.text(for: s, doc: model.document, hidden: mode == .hidden)`.

Dans `macos/Sources/Input/KeyMap.swift` :

```swift
enum ReaderAction: Equatable {
    case command(ReaderCommand)
    /// Une ligne affichée plus haut ou plus bas : seule la vue sait où elle
    /// commence.
    case line(delta: Int)
    case collapseOrHide, copySentence, openSearch
    /// ⌘M : du lecteur au mini et retour.
    case toggleSize
    /// ⌘W : fermer (la lecture s'arrête).
    case close
    case toggleSidebar, toggleBookmarks, showHelp, openSettings, openVoiceSettings
    /// Le texte devient modifiable (double clic, ou « e »).
    case edit
}
```

et, dans `action(for:)`, le bloc `if key.command { … }` devient :

```swift
        if key.command {
            switch key.characters {
            case "c": return .copySentence
            case ",": return .openSettings
            case "m": return .toggleSize
            case "w": return .close
            default: return nil
            }
        }
```

et la ligne `case KeyCode.enter: return .expand` est supprimée.

Dans `macos/Sources/Panel/PanelContent.swift`, remplacer `content(_:)` par :

```swift
    @ViewBuilder
    private func content(_ model: ReaderModel) -> some View {
        switch controller.mode {
        case .mini:
            MiniPlayerView(model: model, theme: theme, perform: controller.perform)
                // La fenêtre fixe la taille (MiniPlacement.size) ; le contenu,
                // centré, n'impose rien.
                .frame(minHeight: 0, maxHeight: .infinity)
                // ↑ ↓ suivent le texte même invisible : une copie cachée de
                // la vue de lecture, à la largeur du lecteur. Un fond ne
                // compte pas dans la taille du mini.
                .background(alignment: .topLeading) {
                    ReadingView(model: model, style: style, lines: controller.lines, shadow: true)
                        .frame(width: controller.shadowTextWidth, height: 40)
                }
        case .reader:
            ExpandedReader(controller: controller, style: style)
                .padding(.horizontal, PanelGeometry.horizontalPadding)
                .padding(.vertical, 18)
        case .hidden:
            // Fenêtre retirée : rien à tenir à jour (une vue de lecture gardée
            // là serait remise à jour à chaque instantané publié).
            Color.clear
        }
    }
```

et supprimer `hint(_:_:)` ; dans `HelpOverlay.rows`, retirer `("⏎", "Agrandir"), ("⌘1 ⌘2 ⌘3", "Petit · moyen · grand"),` et remplacer `("Échap", "Revenir · arrière-plan")` par `("⌘M", "Réduire / agrandir"), ("⌘W · Échap", "Fermer (la lecture s'arrête)")` (l'aide complète arrive à la Tâche 5).

Dans `macos/Sources/MenuContent.swift`, remplacer `Button("Afficher sillon") { app.panel?.show() }` par `Button("Afficher le lecteur") { app.panel?.show() }` et supprimer le `Menu("Taille") { … }` entier.

Dans `macos/Sources/AppDelegate.swift` :
- `present(_:)` : `case .emptyReader: panel.apply(.choose(.reader))` et le commentaire « Le lecteur vide s'ouvre au mode moyen » devient « Le lecteur vide, toujours en grand ».
- `--show-panel` : `panel.apply(.choose(.reader))`.
- Supprimer `panel.restore()` et ses deux lignes de commentaire.
- `openFile(_:)` et `read(capture:)` : `panel.apply(.readSelection)` → `panel.show()` ; le commentaire `// Le mode en cours ; depuis l'arrière-plan, le moyen.` devient `// La forme en cours ; masqué, la dernière taille.`
- `installShortcuts()` : `KeyboardShortcuts.onKeyUp(for: .showPanel) { [unowned self] in self.panel.toggleVisibility() }`.

- [ ] **Step 8 : vérifier** — **`TEST`** `/DisplayModeTests`, puis `/DisplayModeStoreTests`, `/MiniPlacementTests`, `/ReaderPlacementTests`, `/PanelControllerTests`, `/KeyMapTests` : PASS. Puis la suite entière : PASS.

- [ ] **Step 9 : tests d'interface des deux formes.**

`git rm macos/UITests/DisplayModeUITests.swift macos/UITests/PanelUITests.swift`. Déplacer l'extension `waitForLabel` de `PanelUITests.swift` dans `macos/UITests/AuditSupport.swift` (à la fin, telle quelle).

Créer `macos/UITests/ReaderModesUITests.swift` :

```swift
import XCTest

/// Le lecteur et le mini, fermer, rouvrir. Lancé à la main, avec l'accord
/// de l'utilisateur : `-only-testing:SillonUITests/ReaderModesUITests`.
@MainActor
final class ReaderModesUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    private func launch() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--show-panel"]
        app.launch()
        XCTAssertTrue(app.groups["lecteur"].waitForExistence(timeout: 5))
        return app
    }

    /// Rouvre par le menu de la barre des menus.
    private func reopen(_ app: XCUIApplication) {
        app.statusItems.firstMatch.click()
        let item = app.menuItems["Afficher le lecteur"]
        XCTAssertTrue(item.waitForExistence(timeout: 2))
        item.click()
    }

    /// ⌘M : le mini, à sa taille ; ⌘M : le lecteur ; la lecture continue.
    func testCommandMSwitchesBetweenReaderAndMini() {
        let app = launch()
        app.typeKey(" ", modifierFlags: [])
        XCTAssertTrue(app.buttons["lecture"].waitForLabel("Pause"))
        app.typeKey("m", modifierFlags: .command)
        let mini = app.groups["lecteur-mini"]
        XCTAssertTrue(mini.waitForExistence(timeout: 2), "⌘M : pas de mini")
        let settled = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
            abs(mini.frame.width - 340) <= 2 && abs(mini.frame.height - 88) <= 2
        }, object: nil)
        XCTAssertEqual(XCTWaiter.wait(for: [settled], timeout: 2), .completed, "taille du mini : \(mini.frame.size)")
        XCTAssertEqual(app.buttons["lecture"].label, "Pause", "réduire ne met pas en pause")
        app.typeKey("m", modifierFlags: .command)
        XCTAssertTrue(app.groups["lecteur"].waitForExistence(timeout: 2), "⌘M : pas de lecteur")
        XCTAssertEqual(app.buttons["lecture"].label, "Pause")
        app.typeKey(" ", modifierFlags: [])
    }

    /// ⌘W : plus rien à l'écran, ni lecteur ni mini ; rouvert, en pause.
    func testCommandWClosesAndPausesWithoutMini() {
        let app = launch()
        app.typeKey(" ", modifierFlags: [])
        XCTAssertTrue(app.buttons["lecture"].waitForLabel("Pause"))
        app.typeKey("w", modifierFlags: .command)
        XCTAssertTrue(app.groups["lecteur"].waitForNonExistence(timeout: 2))
        XCTAssertFalse(app.groups["lecteur-mini"].exists, "fermer ne laisse jamais le mini")
        reopen(app)
        XCTAssertTrue(app.groups["lecteur"].waitForExistence(timeout: 2))
        XCTAssertTrue(app.buttons["lecture"].waitForLabel("Lecture"), "la lecture s'est arrêtée")
    }

    /// Fermé en mini, rouvert en mini.
    func testReopeningGivesTheLastSize() {
        let app = launch()
        app.typeKey("m", modifierFlags: .command)
        XCTAssertTrue(app.groups["lecteur-mini"].waitForExistence(timeout: 2))
        app.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(app.groups["lecteur-mini"].waitForNonExistence(timeout: 2))
        reopen(app)
        XCTAssertTrue(app.groups["lecteur-mini"].waitForExistence(timeout: 2), "pas rouvert en mini")
    }
}
```

Dans `macos/UITests/AccessibilityAuditUITests.swift`, le test devient :

```swift
    func testReaderAndMiniPassTheAudit() throws {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--show-panel"]
        app.launch()
        XCTAssertTrue(app.groups["lecteur"].waitForExistence(timeout: 5))
        try app.performAccessibilityAuditOutsideTouchBar()
        app.typeKey("m", modifierFlags: .command)
        let mini = app.groups["lecteur-mini"]
        XCTAssertTrue(mini.waitForExistence(timeout: 2))
        try app.performAccessibilityAuditOutsideTouchBar(visibleIn: mini)
    }
```

(commentaire de classe : « Audit d'accessibilité du lecteur et du mini »). Dans `macos/UITests/ExpandedUITests.swift`, dans chaque test : `app.groups["panneau-lecture"]` → `app.groups["lecteur"]` ; supprimer les lignes `app.typeKey(.return, modifierFlags: [])` et les attentes `app.groups["lecteur-agrandi"]…` qui les suivent ; `app.groups["lecteur-agrandi"]` restant → `app.groups["lecteur"]` ; dans `testSidebarSearchAndBack`, la fin devient `app.typeKey(.escape, modifierFlags: [])` (ferme la recherche), puis `XCTAssertTrue(app.groups["lecteur"].exists)` ; dans `testSearchAgainAndNextResult`, supprimer l'assertion « « / » n'a pas agrandi ».

**`UIBUILD`** : attendu, `** TEST BUILD SUCCEEDED **`.

- [ ] **Step 10 : commit**

```bash
git rm macos/Tests/PanelGeometryTests.swift macos/UITests/DisplayModeUITests.swift macos/UITests/PanelUITests.swift
git add macos/Sources/Panel/DisplayMode.swift macos/Sources/Panel/DisplayModeStore.swift \
        macos/Sources/Panel/WindowPlacements.swift macos/Sources/Panel/PanelPlacement.swift \
        macos/Sources/Panel/PanelController.swift macos/Sources/Panel/ReadingPanel.swift \
        macos/Sources/Panel/PanelContent.swift macos/Sources/Panel/PanelAnnouncements.swift \
        macos/Sources/Input/KeyMap.swift macos/Sources/MenuContent.swift macos/Sources/AppDelegate.swift \
        macos/Tests/DisplayModeTests.swift macos/Tests/DisplayModeStoreTests.swift macos/Tests/WindowPlacementsTests.swift \
        macos/Tests/PanelControllerTests.swift macos/Tests/KeyMapTests.swift macos/Tests/HelpOverlayTests.swift \
        macos/Tests/MiniPlayerTests.swift macos/UITests/ReaderModesUITests.swift macos/UITests/AuditSupport.swift \
        macos/UITests/AccessibilityAuditUITests.swift macos/UITests/ExpandedUITests.swift
git commit -m "$(cat <<'EOF'
feat(macos): deux formes, le lecteur et le mini — ⌘M bascule avec le cadre animé, fermer (⌘W, Échap, ⌃⌥S) met en pause et ne laisse jamais le mini, toute ouverture rend la dernière taille, le lecteur retient son cadre ; les préférences de la version précédente se traduisent

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Tâche 3 : le lecteur — en-tête, barre par phrase, progression et vitesse cliquables

Le lecteur prend sa forme définitive : en-tête avec le titre courant et les boutons **Sommaire**, **Rechercher**, **Modifier** (et fermer) ; le texte ; la barre du bas ⏮ ▶ ⏭ par phrase, la progression cliquable, le temps, la vitesse cliquable, « Réduire ». Le sommaire est fermé à l'ouverture. Vide, le lecteur invite à coller ou déposer.

**Files:**
- Create : `macos/Sources/Panel/ReaderView.swift`, `macos/Sources/Panel/BarControls.swift`, `macos/Tests/BarControlsTests.swift`
- Delete : `macos/Sources/Panel/ExpandedReader.swift`
- Modify : `macos/Sources/Panel/PlayerBar.swift:1-84`, `macos/Sources/Panel/PanelContent.swift`, `macos/Sources/Panel/PanelController.swift` (`sidebarVisible`), `macos/Tests/PanelControllerTests.swift` (`PlayerBarFormatTests`, `PanelInvitationTests`), `macos/UITests/LaunchUITests.swift`, `macos/UITests/ExpandedUITests.swift`

**Interfaces:**
- Consumes : `PanelController.perform(_:)`, `.toggleSize`, `.close` (Tâche 2) ; `ReaderModel.document.sentenceCount`.
- Produces :
  - `enum SpeedFormat { static func text(_ speed: Float) -> String; static func spoken(_ speed: Float) -> String }`
  - `enum ProgressSeek { static func sentence(fraction: Double, sentenceCount: UInt32) -> UInt32? }` — la conversion convenue avec le plan 1 : `min(Int(f × count), count − 1)`, envoyée en `.seekTo(sentence:)`
  - `struct BarTransportItem`, `enum SentenceTransport { static let previous, next }` (identifiants `phrase-precedente`, `phrase-suivante`), `struct TransportButton(item:send:)`, `struct BarIconButton(symbol:label:help:identifier:action:)`, `struct HeaderButton`, `struct SeekableTimeline`, `struct SpeedMenu(speed:choose:)` avec `static let choices: [Float] = [0.75, 1, 1.25, 1.5, 2]`
  - `struct ReaderView(controller:style:)`, `struct ReaderHeader`, `struct ReaderBar`, `struct ReaderInvitation`
  - `enum PanelInvitation { static let text: String; static func keys(_:) -> String }`
  - `enum PlayerBar { static func time(_:) -> String }` (la vue `PlayerBar` disparaît)
  - identifiants : `fil-ariane`, `bouton-sommaire`, `bouton-rechercher`, `bouton-modifier`, `fermer`, `progression`, `vitesse-lecteur`, `reduire`, `invitation`, `etat-lecteur`

- [ ] **Step 1 : écrire les tests qui échouent** — `macos/Tests/BarControlsTests.swift` :

```swift
import SwiftUI
import XCTest
@testable import Sillon

@MainActor
final class BarControlsTests: XCTestCase {
    func testSpeedsAreWrittenTheFrenchWayWithoutUselessZeros() {
        XCTAssertEqual(SpeedMenu.choices.map(SpeedFormat.text), ["0,75×", "1×", "1,25×", "1,5×", "2×"])
        XCTAssertEqual(SpeedFormat.text(1.1), "1,1×", "une vitesse hors du menu, après +")
        XCTAssertEqual(SpeedFormat.text(1 + 0.1 + 0.1 + 0.1), "1,3×", "les flottants n'ajoutent pas de chiffres")
        XCTAssertEqual(SpeedFormat.spoken(1.25), "1,25 fois")
    }

    /// Un clic sur la barre : la phrase sous le pointeur, bornée au document.
    func testAClickOnTheTimelineGivesTheSentenceUnderThePointer() {
        XCTAssertEqual(SeekableTimeline.fraction(x: 50, width: 200), 0.25)
        XCTAssertEqual(SeekableTimeline.fraction(x: -10, width: 200), 0)
        XCTAssertEqual(SeekableTimeline.fraction(x: 400, width: 200), 1)
        XCTAssertEqual(SeekableTimeline.fraction(x: 10, width: 0), 0)
        XCTAssertEqual(ProgressSeek.sentence(fraction: 0.25, sentenceCount: 8), 2)
        XCTAssertEqual(ProgressSeek.sentence(fraction: 1, sentenceCount: 8), 7, "la fin : la dernière phrase")
        XCTAssertEqual(ProgressSeek.sentence(fraction: 0, sentenceCount: 8), 0)
        XCTAssertNil(ProgressSeek.sentence(fraction: 0.5, sentenceCount: 0), "document vide : rien")
        XCTAssertNil(ProgressSeek.sentence(fraction: .nan, sentenceCount: 8))
    }

    /// ⏮ et ⏭ font ce que font ← et → : une phrase.
    func testTheSentenceButtonsSendTheArrowActions() {
        XCTAssertEqual(SentenceTransport.previous.action,
                       KeyMap.action(for: KeyPress(characters: "", keyCode: KeyCode.left)))
        XCTAssertEqual(SentenceTransport.next.action,
                       KeyMap.action(for: KeyPress(characters: "", keyCode: KeyCode.right)))
        var sent: [ReaderAction] = []
        TransportButton(item: SentenceTransport.next) { sent.append($0) }.fire()
        XCTAssertEqual(sent, [.command(.sentences(delta: 1))])
    }

    func testEveryNewBarButtonHasAComfortableTarget() {
        for view in [AnyView(TransportButton(item: SentenceTransport.previous) { _ in }),
                     AnyView(BarIconButton(symbol: "xmark", label: "Fermer", help: "Fermer (⌘W)",
                                           identifier: "fermer") {})] {
            let size = NSHostingView(rootView: view).fittingSize
            XCTAssertGreaterThanOrEqual(size.width, 32)
            XCTAssertGreaterThanOrEqual(size.height, 32)
        }
    }

    func testTheEmptyReaderInvitesToPasteOrDrop() {
        XCTAssertEqual(PanelInvitation.text, "Collez un texte (⌘V) ou déposez un fichier")
    }
}
```

Dans `macos/Tests/PanelControllerTests.swift` : dans `PlayerBarFormatTests`, supprimer `testSpeedUsesADecimalCommaWhenShownAndSpoken`, `testVolumeShowsOnlyWhenItDiffersFromFull`, `testSkippedSentencesAreCounted` ; remplacer `PanelInvitationTests` par :

```swift
@MainActor
final class PanelInvitationTests: XCTestCase {
    /// La barre d'espace se dit en français dans « Rien à lire — ⌃⌥Espace ».
    func testTheSpaceBarIsNamedInFrench() {
        let space = KeyboardShortcuts.Shortcut(.space, modifiers: [.control, .option])
        XCTAssertEqual(PanelInvitation.keys(space), "⌃⌥Espace")
        XCTAssertEqual(PlaybackTime.emptyText(shortcut: space), "Rien à lire — ⌃⌥Espace")
        XCTAssertEqual(PlaybackTime.emptyText(shortcut: nil), "Rien à lire — raccourci : ⌘,")
    }
}
```

ajouter dans `PanelControllerTests` :

```swift
    /// Le sommaire s'ouvre à la demande (bouton ou « t ») ; Échap le referme.
    func testTheSidebarOpensOnDemand() {
        let c = shownPanel("# Titre\n\nUn. Deux.")
        XCTAssertFalse(c.sidebarVisible)
        c.perform(.toggleSidebar)
        XCTAssertTrue(c.sidebarVisible)
        c.perform(.collapseOrHide)
        XCTAssertFalse(c.sidebarVisible)
        XCTAssertEqual(c.mode, .reader, "Échap a d'abord fermé le sommaire")
    }
```

et, dans `testClosingPausesAndNeverLeavesTheMini`, supprimer la ligne `c.sidebarVisible = false` (le sommaire est désormais fermé à l'ouverture).

- [ ] **Step 2 : vérifier l'échec** — **`TEST`** `/BarControlsTests`. Attendu : échec de compilation (`SpeedFormat`, `SeekableTimeline`, `ProgressSeek`, `SentenceTransport` inconnus).

- [ ] **Step 3 : implémenter.**

`macos/Sources/Panel/BarControls.swift` :

```swift
import SwiftUI

/// La vitesse à la française : virgule, deux décimales au plus, sans zéro
/// inutile (« 1× », « 1,25× », « 1,1× »).
enum SpeedFormat {
    static func number(_ speed: Float) -> String {
        var s = String(format: "%.2f", (Double(speed) * 100).rounded() / 100)
        while s.hasSuffix("0") { s.removeLast() }
        if s.hasSuffix(".") { s.removeLast() }
        return s.replacingOccurrences(of: ".", with: ",")
    }

    static func text(_ speed: Float) -> String { number(speed) + "×" }
    static func spoken(_ speed: Float) -> String { number(speed) + " fois" }
}

/// Une fraction de la barre de progression devient une phrase (la
/// progression du moteur vaut phrase / nombre de phrases).
enum ProgressSeek {
    static func sentence(fraction: Double, sentenceCount: UInt32) -> UInt32? {
        let count = Int(sentenceCount)
        guard count > 0, fraction.isFinite else { return nil }
        let f = min(max(fraction, 0), 1)
        return UInt32(min(Int(f * Double(count)), count - 1))
    }
}

/// Un bouton de transport : ce qu'il montre, et l'action de sa touche.
struct BarTransportItem: Identifiable, Equatable {
    let identifier: String
    let label: String
    let help: String
    let symbol: String
    let action: ReaderAction
    var id: String { identifier }
}

/// ⏮ et ⏭ : une phrase en arrière, une en avant, comme ← et →.
enum SentenceTransport {
    static let previous = BarTransportItem(identifier: "phrase-precedente", label: "Phrase précédente",
                                           help: "Phrase précédente (←)", symbol: "backward.end.fill",
                                           action: .command(.sentences(delta: -1)))
    static let next = BarTransportItem(identifier: "phrase-suivante", label: "Phrase suivante",
                                       help: "Phrase suivante (→)", symbol: "forward.end.fill",
                                       action: .command(.sentences(delta: 1)))
}

struct TransportButton: View {
    let item: BarTransportItem
    let send: (ReaderAction) -> Void

    func fire() { send(item.action) }

    var body: some View {
        Button(action: fire) {
            Image(systemName: item.symbol)
                .font(.system(size: BarButtonMetrics.glyph))
        }
        .buttonStyle(BarButtonStyle())
        // Les touches restent à la fenêtre après un clic.
        .focusable(false)
        .help(item.help)
        .accessibilityLabel(item.label)
        .accessibilityIdentifier(item.identifier)
    }
}

/// Un bouton d'icône de barre (fermer, réduire, agrandir).
struct BarIconButton: View {
    let symbol: String
    let label: String
    let help: String
    let identifier: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: BarButtonMetrics.glyph))
        }
        .buttonStyle(BarButtonStyle())
        .focusable(false)
        .help(help)
        .accessibilityLabel(label)
        .accessibilityIdentifier(identifier)
    }
}

/// Un bouton titré de l'en-tête (Sommaire, Rechercher, Modifier).
struct HeaderButton: View {
    let title: String
    let symbol: String
    let help: String
    let identifier: String
    var selected = false
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Label(title, systemImage: symbol)
                .labelStyle(.titleAndIcon)
                .padding(.horizontal, 8)
        }
        .buttonStyle(BarButtonStyle())
        .focusable(false)
        .help(help)
        .accessibilityIdentifier(identifier)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}

/// La ligne de temps, cliquable : un clic mène à la phrase sous le pointeur.
struct SeekableTimeline: View {
    let progress: Double
    var markers: [Double] = []
    let theme: Theme
    var enabled = true
    let seek: (Double) -> Void

    /// La part de la barre sous le pointeur, bornée à 0…1.
    static func fraction(x: CGFloat, width: CGFloat) -> Double {
        guard width > 0, x.isFinite else { return 0 }
        return Double(min(max(x / width, 0), 1))
    }

    var body: some View {
        GeometryReader { geo in
            Timeline(progress: progress, markers: markers, theme: theme)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .contentShape(Rectangle())
                .onTapGesture(coordinateSpace: .local) { location in
                    guard enabled else { return }
                    seek(Self.fraction(x: location.x, width: geo.size.width))
                }
        }
        // Une cible plus haute que le trait : facile à viser.
        .frame(height: 20)
        .accessibilityRepresentation {
            ProgressView(value: Timeline.clamped(progress))
                .accessibilityLabel("Progression")
                .accessibilityValue("\(Int(Timeline.clamped(progress) * 100)) %")
        }
        .accessibilityIdentifier("progression")
    }
}

/// La vitesse, cliquable : 0,75 · 1 · 1,25 · 1,5 · 2×.
struct SpeedMenu: View {
    static let choices: [Float] = [0.75, 1, 1.25, 1.5, 2]
    let speed: Float
    let choose: (Float) -> Void

    var body: some View {
        Menu {
            ForEach(Self.choices, id: \.self) { value in
                Button {
                    choose(value)
                } label: {
                    if abs(value - speed) < 0.001 {
                        Label(SpeedFormat.text(value), systemImage: "checkmark")
                    } else {
                        Text(SpeedFormat.text(value))
                    }
                }
            }
        } label: {
            Text(SpeedFormat.text(speed))
                .monospacedDigit()
                .fontWeight(.medium)
                .frame(minWidth: BarButtonMetrics.target, minHeight: BarButtonMetrics.target)
                .contentShape(Rectangle())
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
        .focusable(false)
        .help("Vitesse (+ −)")
        .accessibilityLabel("Vitesse")
        .accessibilityValue(SpeedFormat.spoken(speed))
        .accessibilityIdentifier("vitesse-lecteur")
    }
}
```

`macos/Sources/Panel/ReaderView.swift` :

```swift
import SwiftUI

/// Le lecteur : l'en-tête, le texte (le sommaire à côté s'il est ouvert),
/// la barre, et une ligne d'état seulement quand il y a quelque chose à dire.
struct ReaderView: View {
    let controller: PanelController
    let style: ReadingStyle

    var body: some View {
        let model = controller.model
        VStack(alignment: .leading, spacing: 12) {
            ReaderHeader(controller: controller, theme: style.theme)
            HStack(alignment: .top, spacing: 18) {
                if controller.sidebarVisible {
                    Sidebar(controller: controller, theme: style.theme)
                    Divider()
                }
                if controller.editing {
                    TextEditingView(controller: controller, style: style)
                } else {
                    ReadingView(model: model, style: style, lines: controller.lines,
                                onDoubleClick: { controller.beginEditing() })
                        .overlay {
                            if !model.snapshot.hasDocument { ReaderInvitation(theme: style.theme) }
                        }
                }
            }
            if !controller.editing {
                ReaderBar(controller: controller, theme: style.theme, markers: model.sectionPositions)
            }
            if let line = PanelStatus.line(message: model.message, snapshot: model.snapshot,
                                           preparing: model.voicePreparation.isRunning) {
                Text(line)
                    .font(.system(size: 13))
                    .lineLimit(2)
                    .foregroundStyle(Color(nsColor: style.theme.chrome))
                    .accessibilityIdentifier("etat-lecteur")
            }
        }
    }
}

/// Sans document : l'invitation, au centre de la zone du texte.
struct ReaderInvitation: View {
    let theme: Theme

    var body: some View {
        VStack(spacing: 10) {
            LogoMark()
                .frame(width: 28, height: 28)
                .foregroundStyle(Color(nsColor: theme.past))
            Text(PanelInvitation.text)
                .font(.system(size: 13))
                .multilineTextAlignment(.center)
                .foregroundStyle(Color(nsColor: theme.chrome))
                .accessibilityIdentifier("invitation")
        }
    }
}

/// Le titre courant, la recherche quand elle est ouverte, les trois
/// boutons et la fermeture.
struct ReaderHeader: View {
    let controller: PanelController
    let theme: Theme
    @State private var query = ""
    @FocusState private var searchFocused: Bool

    var body: some View {
        let model = controller.model
        let title = model.breadcrumb.isEmpty ? "sillon" : model.breadcrumb
        HStack(spacing: 6) {
            Text(title)
                .lineLimit(1)
                .truncationMode(.middle)
                .accessibilityLabel(title)
                .accessibilityAddTraits(.isHeader)
                .accessibilityIdentifier("fil-ariane")
            Spacer(minLength: 8)
            if controller.searchActive {
                TextField("Rechercher", text: $query)
                    .textFieldStyle(.notion)
                    .frame(width: 200)
                    .focused($searchFocused)
                    .accessibilityIdentifier("recherche")
                    .onSubmit {
                        model.send(.search(query: query))
                        searchFocused = false
                    }
                    // Le champ n'accepte le focus qu'une fois posé dans la fenêtre.
                    .onAppear { DispatchQueue.main.async { searchFocused = true } }
                    // « / » de nouveau, champ déjà ouvert : le focus y revient.
                    .onChange(of: controller.openSearchRequests) { searchFocused = true }
            }
            if let search = model.snapshot.search {
                Text(search.count == 0 ? "Aucun résultat" : "\(search.position + 1) / \(search.count)")
                    .monospacedDigit()
                    .accessibilityIdentifier("resultats-recherche")
            }
            HeaderButton(title: "Sommaire", symbol: "list.bullet", help: "Sommaire (t)",
                         identifier: "bouton-sommaire", selected: controller.sidebarVisible) {
                controller.perform(.toggleSidebar)
            }
            HeaderButton(title: "Rechercher", symbol: "magnifyingglass", help: "Rechercher (/ ou ⌘F)",
                         identifier: "bouton-rechercher", selected: controller.searchActive) {
                controller.perform(.openSearch)
            }
            HeaderButton(title: "Modifier", symbol: "pencil", help: "Modifier le texte (e)",
                         identifier: "bouton-modifier", selected: controller.editing) {
                controller.perform(.edit)
            }
            .disabled(!model.snapshot.hasDocument || controller.editing)
            BarIconButton(symbol: "xmark", label: "Fermer", help: "Fermer, la lecture s'arrête (⌘W)",
                          identifier: "fermer") {
                controller.perform(.close)
            }
        }
        .font(.system(size: 13))
        .foregroundStyle(Color(nsColor: theme.chrome))
        // Recherche fermée (Échap) : le champ rouvert repart vide.
        .onChange(of: controller.searchActive) { _, active in
            if !active { query = "" }
        }
    }
}

/// ⏮ ▶ ⏭, la progression cliquable, le temps, la vitesse, « Réduire ».
struct ReaderBar: View {
    let controller: PanelController
    let theme: Theme
    var markers: [Double] = []

    var body: some View {
        let model = controller.model
        let s = model.snapshot
        HStack(spacing: 10) {
            HStack(spacing: 2) {
                TransportButton(item: SentenceTransport.previous, send: controller.perform)
                PlayButton(model: model)
                TransportButton(item: SentenceTransport.next, send: controller.perform)
            }
            .disabled(!s.hasDocument)
            SeekableTimeline(progress: Double(s.progress), markers: markers, theme: theme,
                             enabled: s.hasDocument) { fraction in
                guard let sentence = ProgressSeek.sentence(fraction: fraction,
                                                           sentenceCount: model.document.sentenceCount) else { return }
                controller.perform(.command(.seekTo(sentence: sentence)))
            }
            Text(PlayerBar.time(s.elapsed) + " / ~" + PlayerBar.time(s.total))
                .monospacedDigit()
                // Graisse moyenne : les traits fins de « –:–– » restent
                // assez contrastés à 13 pt.
                .fontWeight(.medium)
                .fixedSize()
            SpeedMenu(speed: s.speed) { controller.perform(.command(.setSpeed(speed: $0))) }
            BarIconButton(symbol: "arrow.down.right.and.arrow.up.left", label: "Réduire",
                          help: "Réduire en mini (⌘M)", identifier: "reduire") {
                controller.perform(.toggleSize)
            }
        }
        .font(.system(size: 13))
        .foregroundStyle(Color(nsColor: theme.chrome))
    }
}
```

`git rm macos/Sources/Panel/ExpandedReader.swift`.

Dans `macos/Sources/Panel/PlayerBar.swift`, remplacer la vue `struct PlayerBar: View { … }` (lignes 3-84) par :

```swift
/// Le temps des barres, à la seconde.
enum PlayerBar {
    nonisolated static func time(_ seconds: Double?) -> String {
        guard let seconds, seconds.isFinite else { return "–:––" }
        let t = Int(max(0, seconds).rounded())
        return String(format: "%d:%02d", t / 60, t % 60)
    }
}
```

et dans `SpeedLabel` (encore utilisé par le mini jusqu'à la Tâche 4), `PlayerBar.speedText(speed)` → `SpeedFormat.text(speed)`, `PlayerBar.speedSpoken(speed)` → `SpeedFormat.spoken(speed)`. `Timeline`, `PlayButton`, `BarButtonMetrics`, `BarButtonStyle`, `SpeedLabel`, `VoiceSettingsButton`, `OutputPicker`, `OutputMenu` restent pour l'instant.

Dans `macos/Sources/Panel/PanelContent.swift` : `case .reader:` affiche `ReaderView(controller: controller, style: style)` (mêmes marges) ; remplacer `enum PanelInvitation { … }` par :

```swift
/// L'invitation du lecteur vide, et un raccourci tel qu'il s'écrit.
enum PanelInvitation {
    static let text = "Collez un texte (⌘V) ou déposez un fichier"

    /// Le raccourci tel qu'il s'écrit (« ⌃⌥Espace »). La bibliothèque nomme
    /// la barre d'espace dans la langue de l'app, qui peut retomber sur
    /// l'anglais.
    @MainActor
    static func keys(_ shortcut: KeyboardShortcuts.Shortcut) -> String {
        var keys = shortcut.description
        if shortcut.key == .space, keys.hasSuffix("Space") {
            keys = String(keys.dropLast("Space".count)) + "Espace"
        }
        return keys
    }
}
```

Dans `macos/Sources/Panel/PanelController.swift` : `var sidebarVisible = true` → `var sidebarVisible = false`, avec le commentaire « Le sommaire, fermé à l'ouverture : le bouton ou « t » l'ouvre. ».

- [ ] **Step 4 : vérifier** — **`TEST`** `/BarControlsTests`, `/PanelControllerTests`, `/PlayerBarFormatTests`, `/PanelInvitationTests`, `/MiniPlayerTests` : PASS ; puis la suite entière : PASS.

- [ ] **Step 5 : tests d'interface du lecteur.**

Dans `macos/UITests/LaunchUITests.swift`, ajouter :

```swift
    /// Lancé à la main sans rien à lire : le lecteur, vide, qui invite à
    /// coller ou déposer.
    func testAManualLaunchOpensTheEmptyReader() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-testing-vide", "--ui-testing-lancement-manuel"]
        app.launch()
        XCTAssertTrue(app.groups["lecteur"].waitForExistence(timeout: 5), "pas de lecteur")
        let invitation = app.staticTexts["invitation"]
        XCTAssertTrue(invitation.waitForExistence(timeout: 2))
        XCTAssertEqual(invitation.value as? String ?? invitation.label, "Collez un texte (⌘V) ou déposez un fichier")
        XCTAssertFalse(app.groups["lecteur-mini"].exists)
        XCTAssertFalse(app.buttons["bouton-modifier"].isEnabled, "rien à modifier")
    }
```

Dans `macos/UITests/ExpandedUITests.swift` :
- `testSidebarSearchAndBack` devient :

```swift
    func testHeaderButtonsOpenTheSidebarAndTheSearch() {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--show-panel"]
        app.launch()
        XCTAssertTrue(app.groups["lecteur"].waitForExistence(timeout: 5))
        let sommaire = app.outlines["sommaire"]
        XCTAssertFalse(sommaire.exists, "le sommaire est fermé à l'ouverture")
        app.buttons["bouton-sommaire"].click()
        XCTAssertTrue(sommaire.waitForExistence(timeout: 2))
        app.typeKey("t", modifierFlags: [])
        XCTAssertTrue(sommaire.waitForNonExistence(timeout: 2))

        app.buttons["bouton-rechercher"].click()
        let field = app.textFields["recherche"]
        XCTAssertTrue(field.waitForExistence(timeout: 2))
        field.typeText("promesses\r")
        XCTAssertTrue(app.staticTexts["resultats-recherche"].waitForExistence(timeout: 2))
        app.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(field.waitForNonExistence(timeout: 2))
        XCTAssertTrue(app.groups["lecteur"].exists, "Échap a fermé la recherche, pas le lecteur")
    }
```

- dans `testSidebarKeyboardNavigation` et `testSidebarFollowsTheCurrentSection`, juste après l'attente de `app.groups["lecteur"]`, ajouter `app.typeKey("t", modifierFlags: [])` (le sommaire s'ouvre à la demande).

**`UIBUILD`** : `** TEST BUILD SUCCEEDED **`.

- [ ] **Step 6 : commit**

```bash
git rm macos/Sources/Panel/ExpandedReader.swift
git add macos/Sources/Panel/ReaderView.swift macos/Sources/Panel/BarControls.swift macos/Sources/Panel/PlayerBar.swift \
        macos/Sources/Panel/PanelContent.swift macos/Sources/Panel/PanelController.swift \
        macos/Tests/BarControlsTests.swift macos/Tests/PanelControllerTests.swift \
        macos/UITests/LaunchUITests.swift macos/UITests/ExpandedUITests.swift
git commit -m "$(cat <<'EOF'
feat(macos): le lecteur — en-tête avec Sommaire, Rechercher, Modifier et fermer ; barre ⏮ ▶ ⏭ par phrase, progression cliquable (phrase sous le pointeur), vitesse cliquable (0,75 à 2×), Réduire ; sommaire fermé à l'ouverture ; lecteur vide : « Collez un texte (⌘V) ou déposez un fichier »

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Tâche 4 : le mini, par phrase

Le mini garde ses deux rangées (temps et progression ; transport) mais avance **par phrase** (⏮ ▶ ⏭, comme le lecteur) et gagne « Agrandir ». La vitesse, la sortie audio et l'écrou en partent.

**Files:**
- Modify : `macos/Sources/Panel/MiniPlayerView.swift:65-66,123-202`, `macos/Sources/Panel/PlayerBar.swift` (`SpeedLabel`, `VoiceSettingsButton`, `OutputMenu` retirés)
- Test : `macos/Tests/MiniPlayerTests.swift`, `macos/UITests/ReaderModesUITests.swift`

**Interfaces:**
- Consumes : `SentenceTransport`, `TransportButton`, `BarIconButton` (Tâche 3) ; `ReaderAction.toggleSize` (Tâche 2).
- Produces : `MiniControlsRow(model:transportEnabled:send:)`, `MiniControlsRow.transport: [BarTransportItem]`, `MiniControlsRow.growAction: ReaderAction` ; bouton `agrandir`. `MiniTransport`, `MiniTransportItem`, `MiniTransportButton`, `SpeedLabel`, `VoiceSettingsButton`, `OutputMenu` n'existent plus. `OutputPicker` reste (menu de la barre des menus, jusqu'à la Tâche 7).

- [ ] **Step 1 : écrire les tests qui échouent** — dans `macos/Tests/MiniPlayerTests.swift` :
- supprimer `testTheTransportButtonsSendTheRightActions`, `testTheTransportButtonsHaveLabelsAndHelp`, `testTheButtonsMatchTheKeys`, `testAButtonSendsItsActionOnClick`, `testTheTransportIsDisabledWithoutADocument` ;
- dans le test qui mesure la place de la rangée du haut (il se termine par `XCTAssertLessThanOrEqual(speed.fittingSize.width, MiniBarMetrics.side)`), supprimer la ligne `let speed = NSHostingView(rootView: SpeedLabel(…))` et cette dernière assertion ;
- dans `testEveryBarButtonHasAComfortableTarget`, la liste devient `[AnyView(PlayButton(model: model)), AnyView(TransportButton(item: SentenceTransport.next) { _ in })]` ;
- ajouter :

```swift
    /// Le mini avance par phrase, comme le lecteur, et s'agrandit.
    func testTheMiniMovesBySentenceAndGrows() {
        XCTAssertEqual(MiniControlsRow.transport.map(\.identifier), ["phrase-precedente", "phrase-suivante"])
        XCTAssertEqual(MiniControlsRow.transport.map(\.action),
                       [.command(.sentences(delta: -1)), .command(.sentences(delta: 1))])
        XCTAssertEqual(MiniControlsRow.growAction, .toggleSize)
    }
```

- [ ] **Step 2 : vérifier l'échec** — **`TEST`** `/MiniPlayerTests`. Attendu : échec de compilation (`MiniControlsRow.transport` inconnu).

- [ ] **Step 3 : implémenter** — dans `macos/Sources/Panel/MiniPlayerView.swift` :
- dans `MiniPlayerBar.body`, l'appel devient `MiniControlsRow(model: model, transportEnabled: s.hasDocument, send: perform)` ; le commentaire d'en-tête du fichier dit : « la timeline entre le temps écoulé et le temps restant ; puis ⏮ ▶ ⏭ par phrase, et « Agrandir » » ;
- remplacer `struct MiniControlsRow` et tout ce qui suit (`MiniTransportItem`, `MiniTransport`, `MiniTransportButton`) par :

```swift
/// Rangée 2 : ⏮ ▶ ⏭ par phrase au centre, « Agrandir » à droite.
struct MiniControlsRow: View {
    let model: ReaderModel
    var transportEnabled = true
    let send: (ReaderAction) -> Void

    static let transport = [SentenceTransport.previous, SentenceTransport.next]
    static let growAction = ReaderAction.toggleSize

    var body: some View {
        HStack(spacing: 0) {
            // La même largeur des deux côtés : le transport reste centré.
            Color.clear.frame(width: MiniBarMetrics.side, height: 1)
            Spacer(minLength: 0)
            HStack(spacing: 4) {
                TransportButton(item: SentenceTransport.previous, send: send)
                PlayButton(model: model, prominent: true)
                TransportButton(item: SentenceTransport.next, send: send)
            }
            .disabled(!transportEnabled)
            Spacer(minLength: 0)
            BarIconButton(symbol: "arrow.up.left.and.arrow.down.right", label: "Agrandir",
                          help: "Agrandir le lecteur (⌘M)", identifier: "agrandir") {
                send(Self.growAction)
            }
            .frame(width: MiniBarMetrics.side, alignment: .trailing)
        }
        .frame(height: BarButtonMetrics.disc + 4)
    }
}
```

- `MiniBarMetrics.side` : commentaire « La largeur réservée de chaque côté : le transport reste centré. ».

Dans `macos/Sources/Panel/PlayerBar.swift`, supprimer `SpeedLabel`, `VoiceSettingsButton` et `OutputMenu` (garder `OutputPicker`).

`grep -rn "MiniTransport\|SpeedLabel\|VoiceSettingsButton\|OutputMenu" macos/Sources macos/Tests` : attendu, rien.

- [ ] **Step 4 : vérifier** — **`TEST`** `/MiniPlayerTests` : PASS ; suite entière : PASS.

- [ ] **Step 5 : test d'interface** — dans `macos/UITests/ReaderModesUITests.swift`, ajouter :

```swift
    /// Le mini : ⏮ ▶ ⏭ par phrase et « Agrandir », rien d'autre.
    func testTheMiniMovesBySentenceAndGrows() {
        let app = launch()
        app.typeKey("m", modifierFlags: .command)
        let mini = app.groups["lecteur-mini"]
        XCTAssertTrue(mini.waitForExistence(timeout: 2))
        for id in ["phrase-precedente", "lecture", "phrase-suivante", "agrandir"] {
            XCTAssertTrue(mini.buttons[id].exists, "\(id) absent du mini")
        }
        for id in ["mot-precedent", "mot-suivant", "ligne-precedente", "ligne-suivante", "reglages-voix", "sortie-audio"] {
            XCTAssertFalse(mini.descendants(matching: .any)[id].exists, "\(id) est revenu")
        }
        mini.buttons["agrandir"].click()
        XCTAssertTrue(app.groups["lecteur"].waitForExistence(timeout: 2))
    }
```

**`UIBUILD`** : réussit.

- [ ] **Step 6 : commit**

```bash
git add macos/Sources/Panel/MiniPlayerView.swift macos/Sources/Panel/PlayerBar.swift \
        macos/Tests/MiniPlayerTests.swift macos/UITests/ReaderModesUITests.swift
git commit -m "$(cat <<'EOF'
feat(macos): le mini avance par phrase (⏮ ▶ ⏭, comme le lecteur) et s'agrandit d'un bouton ; vitesse, sortie audio et écrou retirés du mini

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Tâche 5 : le clavier réduit, l'aide complète, ⌘V et ⌘O

La table de la spec 1.5, rien de plus : signets, volume, préréglages, mot, saut de N phrases, bloc, `r`, `g`, Début/Fin, `v`, ⌘1/2/3 et les alias disparaissent. L'aide liste **toutes** les touches. ⌘V lit le presse-papiers, ⌘O ouvre le panneau d'ouverture. Le sommaire perd les signets.

**Files:**
- Modify : `macos/Sources/Input/KeyMap.swift` (réécrit à partir de la ligne 39), `macos/Sources/Panel/PanelController.swift`, `macos/Sources/Panel/PanelContent.swift` (`HelpOverlay.rows`), `macos/Sources/Panel/Sidebar.swift` (réécrit), `macos/Sources/Panel/ReadingDisplay.swift`, `macos/Sources/Model/ReaderModel+Engine.swift:21-24`, `macos/Sources/AppDelegate.swift`
- Create : `macos/Sources/App/OpenableFiles.swift`, `macos/UITests/PasteUITests.swift`
- Test : `macos/Tests/KeyMapTests.swift` (réécrit), `macos/Tests/HelpOverlayTests.swift`, `macos/Tests/PanelControllerTests.swift`, `macos/Tests/ReadingDisplayTests.swift`

**Interfaces:**
- Consumes : `PanelController` (Tâche 2), `SentenceTransport` (Tâche 3).
- Produces :
  - `enum ReaderAction: Equatable { case command(ReaderCommand), line(delta: Int), escape, close, toggleSize, copy, paste, openFile, openSearch, toggleSidebar, showHelp, openSettings, edit }` (plus de `collapseOrHide`, `copySentence`, `toggleBookmarks`, `openVoiceSettings`)
  - `KeyMap.action(for: KeyPress) -> ReaderAction?` (plus de `seekStep`)
  - `PanelController.init(model:store:openSettings:readClipboard:chooseFile:)` — `readClipboard: @escaping @MainActor () -> Void = {}`, `chooseFile: @escaping @MainActor () -> Void = {}` ; `PanelController.pasteboard: NSPasteboard` (le général ; les tests en donnent un nommé) ; `PanelController.copy()`
  - `enum OpenableFiles { static let types: [UTType] }` ; `AppDelegate.chooseFile()`
  - `HelpOverlay.rows: [(String, String)]` (17 lignes)

- [ ] **Step 1 : écrire les tests qui échouent.**

`macos/Tests/KeyMapTests.swift` (remplace le fichier) :

```swift
import XCTest
@testable import Sillon

/// La table de la spec 1.5, et rien d'autre.
final class KeyMapTests: XCTestCase {
    private func act(_ c: String, _ code: UInt16 = 0, option: Bool = false,
                     shift: Bool = false, command: Bool = false, control: Bool = false) -> ReaderAction? {
        KeyMap.action(for: KeyPress(characters: c, keyCode: code, option: option,
                                    shift: shift, command: command, control: control))
    }

    func testTheReadingKeys() {
        XCTAssertEqual(act(" ", KeyCode.space), .command(.togglePlay))
        XCTAssertEqual(act("", KeyCode.left), .command(.sentences(delta: -1)))
        XCTAssertEqual(act("", KeyCode.right), .command(.sentences(delta: 1)))
        XCTAssertEqual(act("", KeyCode.up), .line(delta: -1))
        XCTAssertEqual(act("", KeyCode.down), .line(delta: 1))
        XCTAssertEqual(act("n"), .command(.sections(delta: 1)))
        XCTAssertEqual(act("p"), .command(.sections(delta: -1)))
        XCTAssertEqual(act("+", shift: true), .command(.changeSpeed(delta: 0.1)))
        XCTAssertEqual(act("-"), .command(.changeSpeed(delta: -0.1)))
    }

    func testSearchSidebarEditAndHelp() {
        XCTAssertEqual(act("/"), .openSearch)
        XCTAssertEqual(act("f", command: true), .openSearch)
        XCTAssertEqual(act("N", shift: true), .command(.searchAdvance(delta: 1)))
        XCTAssertEqual(act("P", shift: true), .command(.searchAdvance(delta: -1)))
        XCTAssertEqual(act("t"), .toggleSidebar)
        XCTAssertEqual(act("e"), .edit)
        XCTAssertEqual(act("?", shift: true), .showHelp)
    }

    func testTheCommandKeys() {
        XCTAssertEqual(act("c", command: true), .copy)
        XCTAssertEqual(act("v", command: true), .paste)
        XCTAssertEqual(act("o", command: true), .openFile)
        XCTAssertEqual(act("m", command: true), .toggleSize)
        XCTAssertEqual(act("w", command: true), .close)
        XCTAssertEqual(act(",", command: true), .openSettings)
        XCTAssertEqual(act("", KeyCode.escape), .escape)
        // ⌘A et les autres restent au texte (tout sélectionner).
        XCTAssertNil(act("a", command: true))
    }

    /// Tout ce que la spec retire ne fait plus rien.
    func testRemovedKeysDoNothing() {
        for c in ["b", "B", "[", "]", "m", "r", "g", "v", "h", "=", "_", "1", "2", "3", "4", "&", "é", "\"", "'"] {
            XCTAssertNil(act(c), "« \(c) » devait être retiré")
        }
        for digit in ["1", "2", "3"] { XCTAssertNil(act(digit, command: true), "⌘\(digit)") }
        XCTAssertNil(act("", KeyCode.left, option: true), "⌥← (mot)")
        XCTAssertNil(act("", KeyCode.right, shift: true), "⇧→ (saut)")
        XCTAssertNil(act("", KeyCode.up, shift: true), "⇧↑ (bloc)")
        XCTAssertNil(act("", KeyCode.down, option: true), "⌥↓ (volume)")
        XCTAssertNil(act("", 115), "Début")
        XCTAssertNil(act("", 119), "Fin")
        XCTAssertNil(act("", KeyCode.enter), "⏎")
    }

    func testOptionAndControlBlockTheCharacterKeys() {
        XCTAssertNil(act("n", option: true))
        XCTAssertNil(act("t", control: true))
        XCTAssertNil(act("c", option: true, command: true))
    }
}
```

`macos/Tests/HelpOverlayTests.swift` : ajouter avant `MenuOpeningTests` :

```swift
/// L'aide liste toutes les touches du lecteur, et chacune fait quelque chose.
@MainActor
final class HelpOverlayTests: XCTestCase {
    private let presses: [(String, [KeyPress])] = [
        ("Espace", [KeyPress(characters: " ", keyCode: KeyCode.space)]),
        ("← →", [KeyPress(characters: "", keyCode: KeyCode.left), KeyPress(characters: "", keyCode: KeyCode.right)]),
        ("↑ ↓", [KeyPress(characters: "", keyCode: KeyCode.up), KeyPress(characters: "", keyCode: KeyCode.down)]),
        ("n p", [KeyPress(characters: "n"), KeyPress(characters: "p")]),
        ("/ ⌘F", [KeyPress(characters: "/"), KeyPress(characters: "f", command: true)]),
        ("N P", [KeyPress(characters: "N", shift: true), KeyPress(characters: "P", shift: true)]),
        ("t", [KeyPress(characters: "t")]),
        ("e", [KeyPress(characters: "e")]),
        ("+ −", [KeyPress(characters: "+", shift: true), KeyPress(characters: "-")]),
        ("⌘C", [KeyPress(characters: "c", command: true)]),
        ("⌘V", [KeyPress(characters: "v", command: true)]),
        ("⌘O", [KeyPress(characters: "o", command: true)]),
        ("⌘M", [KeyPress(characters: "m", command: true)]),
        ("⌘W", [KeyPress(characters: "w", command: true)]),
        ("Échap", [KeyPress(characters: "", keyCode: KeyCode.escape)]),
        ("?", [KeyPress(characters: "?", shift: true)]),
        ("⌘,", [KeyPress(characters: ",", command: true)]),
    ]

    func testTheHelpListsEveryKeyAndEachOneWorks() {
        XCTAssertEqual(HelpOverlay.rows.map(\.0), presses.map(\.0))
        for (row, keys) in presses {
            for key in keys { XCTAssertNotNil(KeyMap.action(for: key), "\(row) : sans action") }
        }
    }

    func testTheHelpSaysThatClosingStopsTheReading() {
        XCTAssertTrue(HelpOverlay.rows.contains { $0 == ("⌘W", "Fermer (la lecture s'arrête)") })
        XCTAssertTrue(HelpOverlay.rows.contains { $0 == ("⌘C", "Copier la sélection, sinon la phrase") })
    }
}
```

`macos/Tests/ReadingDisplayTests.swift`, `testTakesWhatThePanelNeedsFromTheSettings` devient :

```swift
    func testTakesWhatThePanelNeedsFromTheSettings() {
        var s = SettingsView.defaults
        s.lineSpacing = 3; s.readingRuler = false
        let d = ReadingDisplay(s)
        XCTAssertEqual(d.lineSpacing, 3)
        XCTAssertFalse(d.readingRuler)
        XCTAssertEqual(ReadingDisplay(), ReadingDisplay(SettingsView.defaults))
    }
```

Dans `macos/Tests/PanelControllerTests.swift` :
- supprimer `testShiftArrowUsesTheSeekStepOfTheSettings` ;
- remplacements : `.collapseOrHide` → `.escape`, `.copySentence` → `.copy` ;
- `shownPanel` : après `let c = PanelController(…)`, ajouter `c.pasteboard = NSPasteboard(name: NSPasteboard.Name("fr.sillon.tests.copie"))` ;
- ajouter :

```swift
    /// ⌘V et ⌘O passent à l'app : le presse-papiers lu, le panneau d'ouverture.
    func testCommandVAndCommandOGoToTheApp() {
        var pasted = 0
        var chosen = 0
        let model = ReaderModel(reader: SillonReader.silent())
        let c = PanelController(model: model, store: DisplayModeStore(defaults: freshDefaults()),
                                readClipboard: { pasted += 1 }, chooseFile: { chosen += 1 })
        controller = c
        c.perform(.paste)
        c.perform(.openFile)
        XCTAssertEqual(pasted, 1)
        XCTAssertEqual(chosen, 1)
    }

    /// ⌘C sans sélection : la phrase en cours, dans le presse-papiers donné.
    func testCommandCWithoutSelectionCopiesTheSentence() {
        let c = shownPanel("Un deux. Trois.")
        c.pasteboard.clearContents()
        c.perform(.copy)
        XCTAssertEqual(c.pasteboard.string(forType: .string), "Un deux.")
        XCTAssertEqual(c.model.message, "Phrase copiée")
    }
```

- [ ] **Step 2 : vérifier l'échec** — **`TEST`** `/KeyMapTests`. Attendu : échec de compilation (`.copy`, `.paste`, `.openFile`, `.escape` inconnus).

- [ ] **Step 3 : implémenter.**

`macos/Sources/Input/KeyMap.swift`, de `enum ReaderAction` à la fin (les `KeyCode` gardent `left`, `right`, `down`, `up`, `enter`, `escape`, `space` ; `home` et `end` sont supprimés) :

```swift
enum ReaderAction: Equatable {
    case command(ReaderCommand)
    /// Une ligne affichée plus haut ou plus bas : seule la vue sait où elle
    /// commence.
    case line(delta: Int)
    /// Échap : la saisie se valide, l'aide, la recherche ou le sommaire se
    /// ferment, sinon le lecteur (comme ⌘W).
    case escape
    /// ⌘W : fermer (la lecture s'arrête).
    case close
    /// ⌘M : du lecteur au mini et retour.
    case toggleSize
    /// ⌘C : la sélection, sinon la phrase en cours.
    case copy
    /// ⌘V : lire le presse-papiers.
    case paste
    /// ⌘O : le panneau d'ouverture.
    case openFile
    case openSearch, toggleSidebar, showHelp, openSettings
    /// Le texte devient modifiable (bouton Modifier, ou « e »).
    case edit
}

/// Les touches du lecteur : la table de la spec (1.5), rien d'autre.
enum KeyMap {
    static func action(for key: KeyPress) -> ReaderAction? {
        if key.command {
            if key.option || key.control { return nil }
            switch key.characters {
            case "c": return .copy
            case "v": return .paste
            case "o": return .openFile
            case "f": return .openSearch
            case "m": return .toggleSize
            case "w": return .close
            case ",": return .openSettings
            default: return nil
            }
        }
        if key.option || key.control { return nil }

        // Les touches sans caractère.
        switch key.keyCode {
        case KeyCode.space: return .command(.togglePlay)
        case KeyCode.escape: return .escape
        case KeyCode.left, KeyCode.right:
            guard !key.shift else { return nil }
            return .command(.sentences(delta: key.keyCode == KeyCode.left ? -1 : 1))
        case KeyCode.up, KeyCode.down:
            guard !key.shift else { return nil }
            return .line(delta: key.keyCode == KeyCode.up ? -1 : 1)
        default: break
        }

        switch key.characters {
        case "n": return .command(.sections(delta: 1))
        case "p": return .command(.sections(delta: -1))
        case "N": return .command(.searchAdvance(delta: 1))
        case "P": return .command(.searchAdvance(delta: -1))
        case "+": return .command(.changeSpeed(delta: 0.1))
        case "-": return .command(.changeSpeed(delta: -0.1))
        case "/": return .openSearch
        case "t": return .toggleSidebar
        case "e": return .edit
        case "?": return .showHelp
        default: return nil
        }
    }
}
```

Dans `macos/Sources/Panel/PanelContent.swift`, `HelpOverlay.rows` devient :

```swift
    static let rows: [(String, String)] = [
        ("Espace", "Lecture / pause"),
        ("← →", "Phrase précédente / suivante"),
        ("↑ ↓", "Ligne précédente / suivante"),
        ("n p", "Titre suivant / précédent"),
        ("/ ⌘F", "Rechercher"),
        ("N P", "Résultat suivant / précédent"),
        ("t", "Sommaire"),
        ("e", "Modifier le texte"),
        ("+ −", "Vitesse"),
        ("⌘C", "Copier la sélection, sinon la phrase"),
        ("⌘V", "Lire le presse-papiers"),
        ("⌘O", "Ouvrir un fichier"),
        ("⌘M", "Réduire / agrandir"),
        ("⌘W", "Fermer (la lecture s'arrête)"),
        ("Échap", "Fermer la recherche ou le sommaire, sinon fermer"),
        ("?", "Cette aide"),
        ("⌘,", "Réglages"),
    ]
```

`macos/Sources/Panel/ReadingDisplay.swift` : supprimer `seekStep` (propriété, commentaire et ligne de `init(_:)`).

`macos/Sources/Panel/Sidebar.swift` (remplace le fichier ; le sommaire seul) :

```swift
import SwiftUI

/// Le sommaire. Tab pour y entrer, flèches pour parcourir, Entrée pour y aller.
struct Sidebar: View {
    let controller: PanelController
    let theme: Theme
    @State private var selection: UInt32?
    /// Vrai quand la liste a le focus clavier : on la parcourt.
    @FocusState private var listFocused: Bool

    var body: some View {
        let model = controller.model
        let items = model.tableOfContents
        VStack(alignment: .leading, spacing: 6) {
            Label("Sommaire", systemImage: "list.bullet")
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(Color(nsColor: theme.chrome))
                .padding(.leading, 8)
                .accessibilityAddTraits(.isHeader)
            List(items, id: \.id, selection: $selection) { item in
                Text(item.title)
                    .font(.system(size: 13))
                    .padding(.leading, CGFloat(max(Int(item.level) - 1, 0)) * 12)
                    .lineLimit(1)
                    .tag(item.id)
            }
            .listStyle(.sidebar)
            .scrollContentBackground(.hidden)
            .focused($listFocused)
            .accessibilityIdentifier("sommaire")
            // Entrée (ou double clic) sur la ligne choisie : on y va.
            .contextMenu(forSelectionType: UInt32.self, menu: { _ in }, primaryAction: { ids in
                guard let id = ids.first, let item = items.first(where: { $0.id == id }),
                      let sentence = item.firstSentence else { return }
                model.send(.seekTo(sentence: sentence))
            })
            .overlay {
                if items.isEmpty {
                    Text("Aucun titre")
                        .font(.system(size: 13))
                        .foregroundStyle(Color(nsColor: theme.chrome))
                }
            }
        }
        .foregroundStyle(Color(nsColor: theme.text))
        .frame(width: 220)
        .onAppear { selection = model.snapshot.section }
        // La ligne choisie suit la section lue, sauf pendant qu'on parcourt
        // la liste au clavier ; en la quittant, elle se recale.
        .onChange(of: model.snapshot.section) { _, section in
            if !listFocused { selection = section }
        }
        .onChange(of: listFocused) { _, focused in
            if !focused { selection = model.snapshot.section }
        }
    }
}
```

(`SectionView.firstSentence` est un `UInt32?` : un titre sans phrase ne mène nulle part.)

`macos/Sources/Model/ReaderModel+Engine.swift` : supprimer `loadBookmarks()` et son commentaire ; le titre de section `// MARK: - Signets` garde `loadAllBookmarks` et `removeBookmarkAsync` jusqu'à la Tâche 9.

`macos/Sources/App/OpenableFiles.swift` :

```swift
import UniformTypeIdentifiers

/// Ce que sillon ouvre : Markdown, texte, PDF, images.
enum OpenableFiles {
    static let types: [UTType] = [UTType(filenameExtension: "md"), .plainText, .text, .pdf, .image]
        .compactMap { $0 }
}
```

`macos/Sources/Panel/PanelController.swift` :
- propriétés et `init` :

```swift
    /// Où va ⌘C : le presse-papiers général ; les tests en donnent un à eux.
    @ObservationIgnored var pasteboard = NSPasteboard.general
    /// ⌘V : l'app lit le presse-papiers (texte ou image).
    @ObservationIgnored private let readClipboard: @MainActor () -> Void
    /// ⌘O : l'app montre le panneau d'ouverture.
    @ObservationIgnored private let chooseFile: @MainActor () -> Void

    init(model: ReaderModel, store: DisplayModeStore? = nil,
         openSettings: @escaping @MainActor (SettingsTab?) -> Void = { _ in },
         readClipboard: @escaping @MainActor () -> Void = {},
         chooseFile: @escaping @MainActor () -> Void = {}) {
        self.model = model
        self.openSettings = openSettings
        self.readClipboard = readClipboard
        self.chooseFile = chooseFile
        … (la suite de l'init inchangée)
```

- supprimer `bookmarksVisible` ;
- `action(for:)` devient `func action(for press: KeyPress) -> ReaderAction? { KeyMap.action(for: press) }` ;
- dans `perform(_:)` : `case .collapseOrHide:` → `case .escape:` ; `case .copySentence:` → `case .copy: copy()` ; supprimer `case .toggleBookmarks` et `case .openVoiceSettings` ; ajouter `case .paste: readClipboard()` et `case .openFile: chooseFile()` ;
- ajouter, après `perform(_:)` :

```swift
    /// ⌘C : la phrase en cours (la sélection arrive à la tâche suivante).
    func copy() {
        let sentence = model.currentSentenceText
        guard !sentence.isEmpty else { return }
        pasteboard.clearContents()
        pasteboard.setString(sentence, forType: .string)
        flash("Phrase copiée")
    }
```

`macos/Sources/AppDelegate.swift` :
- `import UniformTypeIdentifiers` en tête ;
- la création du contrôleur devient :

```swift
        panel = PanelController(model: model, openSettings: { settings.open(tab: $0) },
                                readClipboard: { [weak self] in Task { @MainActor in await self?.read() } },
                                chooseFile: { [weak self] in self?.chooseFile() })
```

- ajouter, après `openFile(_:)` :

```swift
    /// ⌘O, « Ouvrir un fichier… » : le panneau d'ouverture, au-dessus du
    /// lecteur (qui flotte). Une app de barre des menus passe d'abord au
    /// premier plan, sinon le panneau s'ouvrirait derrière l'application active.
    func chooseFile() {
        let chooser = NSOpenPanel()
        chooser.allowedContentTypes = OpenableFiles.types
        chooser.allowsMultipleSelection = false
        chooser.canChooseDirectories = false
        chooser.message = "Choisissez un texte, un PDF ou une image à lire"
        chooser.level = NSWindow.Level(rawValue: NSWindow.Level.floating.rawValue + 1)
        NSApp.activate()
        chooser.begin { [weak self] response in
            guard response == .OK, let url = chooser.url else { return }
            MainActor.assumeIsolated { self?.openFile(url) }
        }
    }
```

`grep -rn "collapseOrHide\|copySentence\|toggleBookmarks\|openVoiceSettings\|seekStep\|bookmarksVisible" macos/Sources macos/Tests` : attendu, rien.

- [ ] **Step 4 : vérifier** — **`TEST`** `/KeyMapTests`, `/HelpOverlayTests`, `/ReadingDisplayTests`, `/PanelControllerTests`, `/BarControlsTests` : PASS ; suite entière : PASS.

- [ ] **Step 5 : test d'interface ⌘V** — `macos/UITests/PasteUITests.swift` :

```swift
import AppKit
import XCTest

/// ⌘V dans le lecteur lit le presse-papiers. Lancé à la main, avec l'accord
/// de l'utilisateur : `-only-testing:SillonUITests/PasteUITests`. Le
/// presse-papiers de l'utilisateur est sauvé, puis rendu.
@MainActor
final class PasteUITests: XCTestCase {
    private var saved: String?

    override func setUp() {
        continueAfterFailure = false
        saved = NSPasteboard.general.string(forType: .string)
    }

    override func tearDown() {
        NSPasteboard.general.clearContents()
        if let saved { NSPasteboard.general.setString(saved, forType: .string) }
    }

    func testCommandVReadsTheClipboard() {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString("Bonjour du presse-papiers. Deuxième phrase.", forType: .string)
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-testing-vide", "--show-panel"]
        app.launch()
        XCTAssertTrue(app.groups["lecteur"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["invitation"].waitForExistence(timeout: 2))
        app.typeKey("v", modifierFlags: .command)
        XCTAssertTrue(app.staticTexts["invitation"].waitForNonExistence(timeout: 3), "rien n'a été lu")
        let text = app.textViews.firstMatch
        XCTAssertTrue(text.waitForText(timeout: 3) { $0.contains("Bonjour du presse-papiers") },
                      "le texte copié n'est pas dans le lecteur")
    }
}
```

**`UIBUILD`** : réussit.

- [ ] **Step 6 : commit**

```bash
git add macos/Sources/Input/KeyMap.swift macos/Sources/Panel/PanelController.swift macos/Sources/Panel/PanelContent.swift \
        macos/Sources/Panel/Sidebar.swift macos/Sources/Panel/ReadingDisplay.swift macos/Sources/Model/ReaderModel+Engine.swift \
        macos/Sources/App/OpenableFiles.swift macos/Sources/AppDelegate.swift \
        macos/Tests/KeyMapTests.swift macos/Tests/HelpOverlayTests.swift macos/Tests/ReadingDisplayTests.swift \
        macos/Tests/PanelControllerTests.swift macos/UITests/PasteUITests.swift
git commit -m "$(cat <<'EOF'
feat(macos): clavier réduit à la table de la spec — signets, volume, préréglages, mot, saut, bloc, r, g, Début/Fin, v, ⌘1/2/3 et alias retirés ; ⌘V lit le presse-papiers, ⌘O ouvre un fichier, ⌘F cherche ; l'aide liste toutes les touches ; le sommaire sans signets

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Tâche 6 : sélectionner et copier, distinct de l'édition

En lecture, le texte se sélectionne (glisser, double clic : un mot, triple clic : le paragraphe) et ne se modifie pas. ⌘C copie la sélection, sinon la phrase. Clic droit : « Copier », « Lire à partir d'ici ». Le suivi de lecture n'efface jamais la sélection. Le double clic ne mène plus à l'édition : seuls le bouton **Modifier** et `e` y mènent ; en édition, le fond est teinté.

**Files:**
- Create : `macos/Sources/Reading/ReadingCopy.swift`, `macos/UITests/SelectionUITests.swift`
- Modify : `macos/Sources/Reading/ReadingTextView.swift:11-12,76-99`, `macos/Sources/Reading/ReadingView.swift`, `macos/Sources/Reading/LineNavigator.swift`, `macos/Sources/Panel/PanelController.swift` (`copy()`, `readFrom(location:)`), `macos/Sources/Panel/ReaderView.swift`, `macos/Sources/Panel/TextEditingView.swift`
- Test : `macos/Tests/ReadingTextViewTests.swift`, `macos/Tests/LineNavigationTests.swift`, `macos/Tests/PanelControllerTests.swift`

**Interfaces:**
- Consumes : `ReaderAction.copy` (Tâche 5), `PanelController.pasteboard` (Tâche 5).
- Produces :
  - `ReadingTextView.isSelectable == true`, `.isEditable == false` ; `var onCopy: (() -> Void)?` ; `var onReadFrom: ((Int) -> Void)?` ; `var selectedText: String?` ; `override func menu(for:) -> NSMenu?` (« Copier », « Lire à partir d'ici ») ; `onDoubleClick` et `mouseDown(with:)` n'existent plus
  - `ReadingView(model:style:lines:shadow:onCopy:onReadFrom:onContentHeight:)` (plus de `onDoubleClick`)
  - `LineNavigator.selectedText: String?` ; `LineNavigator.readCommand(at: Int) -> ReaderCommand?` ; `static LineNavigator.readTarget(at:words:sentences:) -> ReaderCommand?`
  - `enum ReadingCopy { static func choose(selection: String?, sentence: String) -> (text: String, message: String)? }`
  - `PanelController.copy()` (sélection d'abord), `PanelController.readFrom(location: Int)`

- [ ] **Step 1 : écrire les tests qui échouent.**

Dans `macos/Tests/ReadingTextViewTests.swift`, ajouter :

```swift
    /// En lecture, le texte se sélectionne et ne se modifie pas.
    func testTheTextIsSelectableButNotEditable() {
        XCTAssertTrue(view.isSelectable)
        XCTAssertFalse(view.isEditable)
        view.setSelectedRange(NSRange(location: 3, length: 4))
        XCTAssertEqual(view.selectedText, "deux")
        view.setSelectedRange(NSRange(location: 0, length: 0))
        XCTAssertNil(view.selectedText)
    }

    /// La voix avance, la vue suit (trait, couleur, défilement) : la
    /// sélection reste.
    func testFollowingTheVoiceKeepsTheSelection() {
        let selection = NSRange(location: 15, length: 6)
        view.setSelectedRange(selection)
        view.playing = true
        for (i, word) in [NSRange(location: 0, length: 2), NSRange(location: 3, length: 4),
                          NSRange(location: 8, length: 5), NSRange(location: 15, length: 6)].enumerated() {
            view.show(word: word, sentence: NSRange(location: i < 3 ? 0 : 15, length: 14),
                      pastUpTo: word.location, follow: true)
            frames.tick()
        }
        view.setFrameSize(NSSize(width: 420, height: view.frame.height))
        XCTAssertEqual(view.selectedRange(), selection)
        view.playing = false
    }

    /// Clic droit : « Copier », « Lire à partir d'ici » (à l'endroit cliqué).
    func testTheContextMenuOffersCopyAndReadFromHere() throws {
        var readFrom: Int?
        var copies = 0
        view.onReadFrom = { readFrom = $0 }
        view.onCopy = { copies += 1 }
        let word = try XCTUnwrap(view.rectForTesting(NSRange(location: 15, length: 6)))
        let inWindow = view.convert(CGPoint(x: word.midX, y: word.midY), to: nil)
        let event = try XCTUnwrap(NSEvent.mouseEvent(with: .rightMouseDown, location: inWindow, modifierFlags: [],
                                                     timestamp: 0, windowNumber: window.windowNumber, context: nil,
                                                     eventNumber: 0, clickCount: 1, pressure: 1))
        let menu = try XCTUnwrap(view.menu(for: event))
        XCTAssertEqual(menu.items.map(\.title), ["Copier", "Lire à partir d'ici"])
        menu.performActionForItem(at: 1)
        XCTAssertTrue((15...21).contains(readFrom ?? -1), "endroit : \(String(describing: readFrom))")
        menu.performActionForItem(at: 0)
        XCTAssertEqual(copies, 1)
    }
```

Dans `macos/Tests/LineNavigationTests.swift`, ajouter (la classe existante y trouve déjà `WordPlace`) :

```swift
    /// « Lire à partir d'ici » : le mot sous le pointeur, sinon le suivant
    /// de la phrase ; un bloc sans mot, sa phrase.
    func testReadFromHereTargetsTheWordUnderThePointer() {
        // « Un deux. Trois » : phrase 0 = 0…7, phrase 1 = 9…13.
        let words = [WordPlace(range: NSRange(location: 0, length: 2), sentence: 0, word: 0, index: 0),
                     WordPlace(range: NSRange(location: 3, length: 5), sentence: 0, word: 1, index: 1),
                     WordPlace(range: NSRange(location: 9, length: 5), sentence: 1, word: 0, index: 2)]
        let sentences: [UInt32: NSRange] = [0: NSRange(location: 0, length: 8), 1: NSRange(location: 9, length: 5),
                                            2: NSRange(location: 15, length: 20)]
        XCTAssertEqual(LineNavigator.readTarget(at: 4, words: words, sentences: sentences),
                       .seekToWord(sentence: 0, word: 1))
        XCTAssertEqual(LineNavigator.readTarget(at: 3, words: words, sentences: sentences),
                       .seekToWord(sentence: 0, word: 1), "au début du mot")
        XCTAssertEqual(LineNavigator.readTarget(at: 8, words: words, sentences: sentences),
                       .seekToWord(sentence: 1, word: 0), "entre deux phrases : la suivante")
        XCTAssertEqual(LineNavigator.readTarget(at: 20, words: words, sentences: sentences),
                       .seekTo(sentence: 2), "un bloc de code : sa phrase")
        XCTAssertNil(LineNavigator.readTarget(at: 99, words: words, sentences: sentences))
    }
```

Dans `macos/Tests/PanelControllerTests.swift`, ajouter :

```swift
    /// ⌘C : la sélection si elle existe, sinon la phrase en cours.
    func testCommandCCopiesTheSelectionThenTheSentence() throws {
        let c = shownPanel("Un deux. Trois quatre.")
        let text = try XCTUnwrap(textView(c))
        text.setSelectedRange(NSRange(location: 9, length: 5))
        c.perform(.copy)
        XCTAssertEqual(c.pasteboard.string(forType: .string), "Trois")
        XCTAssertEqual(c.model.message, "Sélection copiée")
        XCTAssertFalse(c.editing, "copier n'entre pas en édition")
        text.setSelectedRange(NSRange(location: 0, length: 0))
        c.perform(.copy)
        XCTAssertEqual(c.pasteboard.string(forType: .string), "Un deux.")
    }

    /// « Lire à partir d'ici » : la voix part de ce mot.
    func testReadFromHereStartsTheVoiceThere() {
        let c = shownPanel("Un deux. Trois quatre.")
        c.readFrom(location: 9)
        XCTAssertTrue(waitUntil { c.model.refresh(); return c.model.snapshot.sentence == 1 && c.model.snapshot.playing })
        c.model.send(.pause)
    }

    func testCopyChoosesANonBlankSelection() {
        XCTAssertEqual(ReadingCopy.choose(selection: "mot", sentence: "Phrase.")?.text, "mot")
        XCTAssertEqual(ReadingCopy.choose(selection: "  \n", sentence: "Phrase.")?.text, "Phrase.")
        XCTAssertEqual(ReadingCopy.choose(selection: nil, sentence: "Phrase.")?.message, "Phrase copiée")
        XCTAssertNil(ReadingCopy.choose(selection: nil, sentence: ""))
    }
```

- [ ] **Step 2 : vérifier l'échec** — **`TEST`** `/ReadingTextViewTests`. Attendu : échec de compilation (`selectedText`, `onReadFrom`, `onCopy` inconnus).

- [ ] **Step 3 : implémenter.**

`macos/Sources/Reading/ReadingCopy.swift` :

```swift
/// Ce que copie ⌘C : la sélection (si elle n'est pas vide), sinon la phrase
/// en cours ; et ce qu'on en dit.
enum ReadingCopy {
    static func choose(selection: String?, sentence: String) -> (text: String, message: String)? {
        if let selection, !selection.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            return (selection, "Sélection copiée")
        }
        guard !sentence.isEmpty else { return nil }
        return (sentence, "Phrase copiée")
    }
}
```

`macos/Sources/Reading/ReadingTextView.swift` :
- remplacer les lignes 11-12 (`/// Un double clic : le mode édition …` et `var onDoubleClick`) par :

```swift
    /// « Copier » du menu contextuel : l'app décide (sélection, sinon phrase).
    var onCopy: (() -> Void)?
    /// « Lire à partir d'ici » : l'endroit cliqué, en caractère du texte.
    var onReadFrom: ((Int) -> Void)?
```

- dans `init()`, `isSelectable = false` → `isSelectable = true` (avec le commentaire « Le texte se sélectionne (glisser, double clic : un mot, triple clic : le paragraphe) ; il ne se modifie qu'en édition, ailleurs. ») ;
- supprimer `override func mouseDown(with:)` en entier ;
- ajouter, après `contentHeight` :

```swift
    /// Le texte sélectionné, nil sans sélection.
    var selectedText: String? {
        let range = selectedRange()
        guard range.length > 0, NSMaxRange(range) <= (string as NSString).length else { return nil }
        return (string as NSString).substring(with: range)
    }

    // MARK: - Menu contextuel

    override func menu(for event: NSEvent) -> NSMenu? {
        let point = convert(event.locationInWindow, from: nil)
        let index = characterIndexForInsertion(at: point)
        let menu = NSMenu()
        let copy = NSMenuItem(title: "Copier", action: #selector(copyFromMenu(_:)), keyEquivalent: "")
        copy.target = self
        let read = NSMenuItem(title: "Lire à partir d'ici", action: #selector(readFromMenu(_:)), keyEquivalent: "")
        read.target = self
        read.representedObject = index
        menu.items = [copy, read]
        return menu
    }

    @objc private func copyFromMenu(_ sender: Any?) {
        onCopy?()
    }

    @objc private func readFromMenu(_ sender: NSMenuItem) {
        guard let index = sender.representedObject as? Int else { return }
        onReadFrom?(index)
    }
```

(Aucune autre méthode ne touche à `selectedRange` : `show`, `place`, `applyPast`, `applyWordColor` et `keepLineInPlace` passent par les attributs de rendu et le défilement, qui laissent la sélection en place. `setDocument` la remet à zéro, mais seulement à un nouveau document ou un nouveau style.)

`macos/Sources/Reading/ReadingView.swift` :
- remplacer `var onDoubleClick: (() -> Void)?` (et son commentaire) par :

```swift
    /// « Copier » du menu contextuel.
    var onCopy: (() -> Void)?
    /// « Lire à partir d'ici » du menu contextuel.
    var onReadFrom: ((Int) -> Void)?
```

- dans `updateNSView`, `text.onDoubleClick = onDoubleClick` → `text.onCopy = onCopy` et `text.onReadFrom = onReadFrom`.

`macos/Sources/Reading/LineNavigator.swift`, ajouter à la classe :

```swift
    /// Le texte sélectionné dans la vue affichée.
    var selectedText: String? { view?.selectedText }

    /// « Lire à partir d'ici » : la commande qui mène la voix à `location`.
    func readCommand(at location: Int) -> ReaderCommand? {
        guard let layout else { return nil }
        return Self.readTarget(at: location, words: layout.words, sentences: layout.sentenceRanges)
    }

    /// Le mot sous `location` (ou le suivant, s'il est dans la même phrase,
    /// ou si l'endroit n'est dans aucune phrase) ; sinon la phrase qui
    /// contient `location` (un bloc de code annoncé) ; sinon rien.
    static func readTarget(at location: Int, words: [WordPlace], sentences: [UInt32: NSRange]) -> ReaderCommand? {
        var low = 0, high = words.count
        while low < high {
            let mid = (low + high) / 2
            if NSMaxRange(words[mid].range) <= location { low = mid + 1 } else { high = mid }
        }
        let sentenceHere = sentences.first { NSLocationInRange(location, $0.value) }?.key
        if low < words.count {
            let w = words[low]
            if sentenceHere == nil || sentenceHere == w.sentence {
                return .seekToWord(sentence: w.sentence, word: w.word)
            }
        }
        if let sentence = sentenceHere { return .seekTo(sentence: sentence) }
        return nil
    }
```

`macos/Sources/Panel/PanelController.swift`, `copy()` devient, et `readFrom(location:)` s'ajoute :

```swift
    /// ⌘C, « Copier » : la sélection, sinon la phrase en cours. N'entre
    /// jamais en édition.
    func copy() {
        guard let choice = ReadingCopy.choose(selection: lines.selectedText,
                                              sentence: model.currentSentenceText) else { return }
        pasteboard.clearContents()
        pasteboard.setString(choice.text, forType: .string)
        flash(choice.message)
    }

    /// « Lire à partir d'ici » : la voix rejoint ce mot et lit.
    func readFrom(location: Int) {
        guard let command = lines.readCommand(at: location) else { return }
        model.send(command)
        model.refresh()
        if !model.snapshot.playing { model.send(.togglePlay) }
    }
```

et le commentaire de `beginEditing()` devient « Bouton Modifier ou « e » : … ».

`macos/Sources/Panel/ReaderView.swift`, l'appel de `ReadingView` devient :

```swift
                    ReadingView(model: model, style: style, lines: controller.lines,
                                onCopy: { controller.copy() },
                                onReadFrom: { controller.readFrom(location: $0) })
```

`macos/Sources/Panel/TextEditingView.swift` : le fond teinté — après `.scrollIndicators(.automatic)`, ajouter :

```swift
                .padding(8)
                // En édition, le fond se teinte : on voit qu'on écrit.
                .background(RoundedRectangle(cornerRadius: 8)
                    .fill(Color(nsColor: style.theme.text).opacity(0.05)))
```

`grep -rn "onDoubleClick\|mouseDown" macos/Sources` : attendu, rien.

- [ ] **Step 4 : vérifier** — **`TEST`** `/ReadingTextViewTests`, `/LineNavigationTests`, `/PanelControllerTests`, `/EditingTests` : PASS ; suite entière : PASS.

- [ ] **Step 5 : tests d'interface** — `macos/UITests/SelectionUITests.swift` :

```swift
import AppKit
import XCTest

/// Sélectionner et copier sans entrer en édition ; l'édition seulement par
/// le bouton Modifier. Lancé à la main, avec l'accord de l'utilisateur :
/// `-only-testing:SillonUITests/SelectionUITests`. Le presse-papiers de
/// l'utilisateur est sauvé, puis rendu.
@MainActor
final class SelectionUITests: XCTestCase {
    private var saved: String?

    override func setUp() {
        continueAfterFailure = false
        saved = NSPasteboard.general.string(forType: .string)
    }

    override func tearDown() {
        NSPasteboard.general.clearContents()
        if let saved { NSPasteboard.general.setString(saved, forType: .string) }
    }

    private func launch() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--show-panel"]
        app.launch()
        XCTAssertTrue(app.groups["lecteur"].waitForExistence(timeout: 5))
        return app
    }

    /// Tout sélectionner puis ⌘C : tout le texte, pas seulement la phrase ;
    /// aucune édition.
    func testSelectionAndCommandCCopyWithoutEditing() {
        let app = launch()
        let text = app.textViews.firstMatch
        XCTAssertTrue(text.waitForExistence(timeout: 2))
        text.click()
        app.typeKey("a", modifierFlags: .command)
        NSPasteboard.general.clearContents()
        app.typeKey("c", modifierFlags: .command)
        let copied = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
            (NSPasteboard.general.string(forType: .string) ?? "").contains("promesses")
        }, object: nil)
        XCTAssertEqual(XCTWaiter.wait(for: [copied], timeout: 2), .completed,
                       "copié : \(NSPasteboard.general.string(forType: .string) ?? "rien")")
        XCTAssertFalse(app.descendants(matching: .any)["barre-edition"].exists, "⌘C est entré en édition")
    }

    /// Le double clic sélectionne un mot ; il n'entre plus en édition.
    func testDoubleClickDoesNotEdit() {
        let app = launch()
        let text = app.textViews.firstMatch
        XCTAssertTrue(text.waitForExistence(timeout: 2))
        text.doubleClick()
        XCTAssertFalse(app.descendants(matching: .any)["barre-edition"].waitForExistence(timeout: 1))
    }

    /// Le bouton Modifier ouvre l'édition ; Terminé la referme.
    func testTheEditButtonEdits() {
        let app = launch()
        app.buttons["bouton-modifier"].click()
        XCTAssertTrue(app.descendants(matching: .any)["barre-edition"].waitForExistence(timeout: 2))
        XCTAssertTrue(app.textViews["edition-texte"].exists)
        app.buttons["edition-terminer"].click()
        XCTAssertTrue(app.descendants(matching: .any)["barre-edition"].waitForNonExistence(timeout: 2))
    }
}
```

**`UIBUILD`** : réussit.

- [ ] **Step 6 : commit**

```bash
git add macos/Sources/Reading/ReadingCopy.swift macos/Sources/Reading/ReadingTextView.swift \
        macos/Sources/Reading/ReadingView.swift macos/Sources/Reading/LineNavigator.swift \
        macos/Sources/Panel/PanelController.swift macos/Sources/Panel/ReaderView.swift macos/Sources/Panel/TextEditingView.swift \
        macos/Tests/ReadingTextViewTests.swift macos/Tests/LineNavigationTests.swift macos/Tests/PanelControllerTests.swift \
        macos/UITests/SelectionUITests.swift
git commit -m "$(cat <<'EOF'
feat(macos): le texte se sélectionne sans se modifier — ⌘C copie la sélection, sinon la phrase ; clic droit : Copier, Lire à partir d'ici ; le suivi de lecture garde la sélection ; le double clic ne mène plus à l'édition (bouton Modifier ou e), fond teinté en édition

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Tâche 7 : ouvrir (glisser-déposer), menu de cinq lignes, deux raccourcis globaux

Un fichier déposé sur le lecteur s'ouvre. Le menu de la barre des menus tient en cinq lignes (plus « Autoriser la lecture de la sélection… » tant que l'accessibilité manque). Il ne reste que deux raccourcis globaux : lire la sélection, afficher / masquer le lecteur.

**Files:**
- Create : `macos/Tests/OpeningTests.swift`, `macos/UITests/MenuUITests.swift`
- Modify : `macos/Sources/App/OpenableFiles.swift`, `macos/Sources/Panel/PanelController.swift` (`openFile`, `openDropped`), `macos/Sources/Panel/PanelContent.swift` (dépôt), `macos/Sources/MenuContent.swift` (réécrit), `macos/Sources/App/AppServices.swift`, `macos/Sources/App/Shortcuts.swift:5-11`, `macos/Sources/Settings/ShortcutsTab.swift:9-15`, `macos/Sources/AppDelegate.swift`, `macos/Sources/Panel/PlaybackTime.swift` (`menuLine` retiré), `macos/Sources/Panel/PlayerBar.swift` (`OutputPicker` retiré)
- Test : `macos/Tests/MiniPlayerTests.swift` (`testTheMenuLine` retiré)

**Interfaces:**
- Consumes : `AppDelegate.openFile(_:)`, `AppDelegate.chooseFile()` (Tâche 5) ; `PanelController.toggleVisibility()` (Tâche 2).
- Produces :
  - `enum DroppedFiles { static func file(from urls: [URL]) -> URL? }` (dans `OpenableFiles.swift`)
  - `PanelController.init(…, openFile: @escaping @MainActor (URL) -> Void = { _ in })` ; `PanelController.openDropped(_ urls: [URL]) -> Bool`
  - `AppServices { model, panel, readSelection: @MainActor () async -> Void, chooseFile: @MainActor () -> Void }` (plus de `transport` ni de `read`)
  - `KeyboardShortcuts.Name.readSelection`, `.showPanel` seulement
  - argument de test `--ui-testing-raccourcis` : le seul raccourci « afficher / masquer » est inscrit sous les tests d'interface

- [ ] **Step 1 : écrire les tests qui échouent** — `macos/Tests/OpeningTests.swift` :

```swift
import XCTest
@testable import Sillon

/// Glisser-déposer un fichier sur le lecteur.
@MainActor
final class OpeningTests: XCTestCase {
    func testOnlyFilesAreOpenedAndTheLastOneWins() {
        let a = URL(fileURLWithPath: "/tmp/a.md")
        let b = URL(fileURLWithPath: "/tmp/b.pdf")
        let web = URL(string: "https://exemple.fr/page")!
        XCTAssertEqual(DroppedFiles.file(from: [a, b]), b)
        XCTAssertEqual(DroppedFiles.file(from: [a, web]), a)
        XCTAssertNil(DroppedFiles.file(from: [web]))
        XCTAssertNil(DroppedFiles.file(from: []))
    }

    func testADroppedFileGoesToTheApp() {
        var opened: [URL] = []
        let name = "sillon.tests.affichage.ouverture"
        let defaults = UserDefaults(suiteName: name)!
        defaults.removePersistentDomain(forName: name)
        let c = PanelController(model: ReaderModel(reader: SillonReader.silent()),
                                store: DisplayModeStore(defaults: defaults),
                                openFile: { opened.append($0) })
        let file = URL(fileURLWithPath: "/tmp/cours.md")
        XCTAssertTrue(c.openDropped([file]))
        XCTAssertFalse(c.openDropped([URL(string: "https://exemple.fr")!]))
        XCTAssertEqual(opened, [file])
    }

    /// Le panneau d'ouverture propose Markdown, texte, PDF et images.
    func testTheOpenPanelOffersTextPdfAndImages() {
        XCTAssertTrue(OpenableFiles.types.contains(.pdf))
        XCTAssertTrue(OpenableFiles.types.contains(.image))
        XCTAssertTrue(OpenableFiles.types.contains(.plainText))
        XCTAssertTrue(OpenableFiles.types.contains { $0.preferredFilenameExtension == "md" })
    }
}
```

(`import UniformTypeIdentifiers` en tête du fichier, pour `.pdf`, `.image`, `.plainText`.)

Dans `macos/Tests/MiniPlayerTests.swift`, supprimer `testTheMenuLine`.

- [ ] **Step 2 : vérifier l'échec** — **`TEST`** `/OpeningTests`. Attendu : échec de compilation (`DroppedFiles`, `openFile:`, `openDropped` inconnus).

- [ ] **Step 3 : implémenter.**

Dans `macos/Sources/App/OpenableFiles.swift`, ajouter :

```swift
import Foundation

/// Ce qu'on dépose sur le lecteur : le dernier fichier (un seul document à
/// la fois) ; une adresse web n'est pas un fichier.
enum DroppedFiles {
    static func file(from urls: [URL]) -> URL? {
        urls.last(where: \.isFileURL)
    }
}
```

`macos/Sources/Panel/PanelController.swift` :
- propriété et paramètre :

```swift
    /// Un fichier déposé : l'app l'ouvre.
    @ObservationIgnored private let openFile: @MainActor (URL) -> Void
```

`init(…, chooseFile: … = {}, openFile: @escaping @MainActor (URL) -> Void = { _ in })`, avec `self.openFile = openFile` ;
- ajouter :

```swift
    /// Un dépôt sur la fenêtre : le dernier fichier s'ouvre. Faux si rien
    /// n'était un fichier.
    @discardableResult
    func openDropped(_ urls: [URL]) -> Bool {
        guard let file = DroppedFiles.file(from: urls) else { return false }
        openFile(file)
        return true
    }
```

`macos/Sources/Panel/PanelContent.swift`, dans `body`, après `.ignoresSafeArea()` :

```swift
            // Un fichier (Markdown, texte, PDF, image) déposé sur le lecteur ou le mini.
            .dropDestination(for: URL.self) { urls, _ in controller.openDropped(urls) }
```

`macos/Sources/App/Shortcuts.swift`, l'extension devient :

```swift
extension KeyboardShortcuts.Name {
    /// Lire la sélection, sinon le presse-papiers.
    static let readSelection = Self("readSelection", default: .init(.space, modifiers: [.control, .option]))
    /// Afficher le lecteur dans sa dernière taille, ou le masquer.
    static let showPanel = Self("showPanel", default: .init(.s, modifiers: [.control, .option]))
}
```

`macos/Sources/Settings/ShortcutsTab.swift`, le groupe devient :

```swift
            SettingsGroup(title: "Raccourcis globaux") {
                row("Lire la sélection", "Le texte sélectionné, sinon le presse-papiers", .readSelection,
                    id: "raccourci-lecture-selection")
                row("Afficher / masquer le lecteur", "Dans sa dernière taille", .showPanel, id: "raccourci-afficher")
            }
```

`macos/Sources/App/AppServices.swift`, la classe devient :

```swift
@MainActor
@Observable
final class AppServices {
    var model: ReaderModel?
    var panel: PanelController?
    /// « Lire la sélection » : la sélection, sinon le presse-papiers.
    @ObservationIgnored var readSelection: @MainActor () async -> Void = {}
    /// « Ouvrir un fichier… » : le panneau d'ouverture.
    @ObservationIgnored var chooseFile: @MainActor () -> Void = {}
}
```

`macos/Sources/MenuContent.swift` (remplace le fichier) :

```swift
import KeyboardShortcuts
import SwiftUI

/// Le menu de la barre des menus : cinq lignes, plus l'autorisation
/// d'accessibilité tant qu'elle manque.
struct MenuContent: View {
    /// Avant la fin du lancement et en mode mesure, ni modèle ni panneau :
    /// chaque action passe par `?` et ne fait rien.
    @Environment(AppServices.self) private var app

    var body: some View {
        Button("Lire la sélection") {
            Task { @MainActor in await app.readSelection() }
        }
        .globalKeyboardShortcut(.readSelection)
        Button("Ouvrir un fichier…") { app.chooseFile() }
            .keyboardShortcut("o")
        // Toujours là : de quoi revenir au lecteur.
        Button("Afficher le lecteur") { app.panel?.show() }
            .globalKeyboardShortcut(.showPanel)
        if Onboarding.needsReminder {
            Button("Autoriser la lecture de la sélection…") {
                Onboarding.reminded()
                let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")!
                NSWorkspace.shared.open(url)
            }
        }
        Divider()
        SettingsLink { Text("Réglages…") }
            .keyboardShortcut(",")
        Button("Quitter sillon") { NSApp.terminate(nil) }
            .keyboardShortcut("q")
    }
}
```

`macos/Sources/Panel/PlaybackTime.swift` : supprimer `menuLine(elapsed:total:)` ; le commentaire du type devient « Les temps du mini, calculés sans vue. ». `macos/Sources/Panel/PlayerBar.swift` : supprimer `OutputPicker`.

`macos/Sources/AppDelegate.swift` :
- la création du contrôleur reçoit `openFile: { [weak self] in self?.openFile($0) }` ;
- `services.transport = transport` et `services.read = …` deviennent :

```swift
        services.readSelection = { [weak self] in await self?.readSelection() }
        services.chooseFile = { [weak self] in self?.chooseFile() }
```

- ajouter, après `read(capture:)` :

```swift
    /// « Lire la sélection » du menu : comme ⌃⌥Espace ; sans la chaîne de
    /// capture (sous les tests), le presse-papiers seul.
    func readSelection() async {
        if let chain = captureChain {
            await read(capture: { await chain.capture() })
        } else {
            await read()
        }
    }
```

- `installShortcuts()` devient :

```swift
    private func installShortcuts() {
        captureChain = CaptureChain(sources: [
            SelectionSource(),
            CopySource(pasteboard: SystemPasteboard(), keys: SystemKeystrokes()),
        ])
        KeyboardShortcuts.onKeyUp(for: .readSelection) { [unowned self] in
            Task { @MainActor in await self.readSelection() }
        }
        installToggleShortcut()
    }

    /// ⌃⌥S : afficher le lecteur dans sa dernière taille, ou le masquer.
    private func installToggleShortcut() {
        KeyboardShortcuts.onKeyUp(for: .showPanel) { [unowned self] in self.panel.toggleVisibility() }
    }
```

- dans le bloc `if isUITesting { … }`, ajouter :

```swift
            // Le seul ⌃⌥S, pour son test d'interface (quitter d'abord le
            // sillon installé, qui tiendrait le même raccourci).
            if arguments.contains("--ui-testing-raccourcis") { installToggleShortcut() }
```

`grep -rn "playPause\|nextSentence\|previousSentence\|menuLine\|OutputPicker\|services.transport\|app.read()" macos/Sources macos/Tests` : attendu, rien.

- [ ] **Step 4 : vérifier** — **`TEST`** `/OpeningTests`, `/MiniPlayerTests`, `/PanelControllerTests` : PASS ; suite entière : PASS.

- [ ] **Step 5 : tests d'interface** — `macos/UITests/MenuUITests.swift` :

```swift
import XCTest

/// Le menu de la barre des menus et ⌃⌥S. Lancé à la main, avec l'accord
/// de l'utilisateur : `-only-testing:SillonUITests/MenuUITests`. Quitter
/// d'abord le sillon installé : il tiendrait ⌃⌥S et aurait aussi une icône.
@MainActor
final class MenuUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    func testTheMenuHasFiveLines() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing"]
        app.launch()
        app.statusItems.firstMatch.click()
        let titles = app.menuItems.allElementsBoundByIndex.map(\.title).filter { !$0.isEmpty }
            .filter { $0 != "Autoriser la lecture de la sélection…" }
        XCTAssertEqual(titles, ["Lire la sélection", "Ouvrir un fichier…", "Afficher le lecteur",
                                "Réglages…", "Quitter sillon"])
        app.typeKey(.escape, modifierFlags: [])
    }

    /// ⌃⌥S affiche le lecteur, puis le masque.
    func testControlOptionSShowsAndHidesTheReader() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--ui-testing-raccourcis"]
        app.launch()
        XCTAssertFalse(app.groups["lecteur"].waitForExistence(timeout: 1), "rien au lancement")
        app.typeKey("s", modifierFlags: [.control, .option])
        XCTAssertTrue(app.groups["lecteur"].waitForExistence(timeout: 3), "⌃⌥S n'a rien affiché")
        app.typeKey("s", modifierFlags: [.control, .option])
        XCTAssertTrue(app.groups["lecteur"].waitForNonExistence(timeout: 3), "⌃⌥S n'a pas masqué")
    }
}
```

**`UIBUILD`** : réussit.

- [ ] **Step 6 : commit**

```bash
git add macos/Sources/App/OpenableFiles.swift macos/Sources/Panel/PanelController.swift macos/Sources/Panel/PanelContent.swift \
        macos/Sources/MenuContent.swift macos/Sources/App/AppServices.swift macos/Sources/App/Shortcuts.swift \
        macos/Sources/Settings/ShortcutsTab.swift macos/Sources/AppDelegate.swift macos/Sources/Panel/PlaybackTime.swift \
        macos/Sources/Panel/PlayerBar.swift macos/Tests/OpeningTests.swift macos/Tests/MiniPlayerTests.swift \
        macos/UITests/MenuUITests.swift
git commit -m "$(cat <<'EOF'
feat(macos): un fichier déposé sur le lecteur s'ouvre ; menu de cinq lignes (lire la sélection, ouvrir, afficher le lecteur, réglages, quitter) ; deux raccourcis globaux seulement, ⌃⌥Espace et ⌃⌥S qui affiche ou masque le lecteur

Lecture/pause et phrase ± globaux retirés ; les touches média et les AirPods restent.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Tâche 8 : police et taille dans `config.toml`, avec reprise des préférences

Un seul stockage : le lecteur lit police et taille dans les réglages du moteur (`SettingsView.readingFont`, `.readingSize`, plan 1), comme l'interlignage. Au premier lancement, les valeurs encore dans les préférences (`readingFont`, `readingSize`) sont écrites dans `config.toml` puis effacées — sauf si l'écriture n'a pas lieu.

**Files:**
- Create : `macos/Sources/Settings/ReadingPreferencesMigration.swift`, `macos/Tests/ReadingPreferencesMigrationTests.swift`
- Modify : `macos/Sources/Panel/ReadingDisplay.swift`, `macos/Sources/Panel/PanelContent.swift:6-7,17-23`, `macos/Sources/Style/ReadingFont.swift:4-8`, `macos/Sources/Settings/ReadingTab.swift:6-7,13-25`, `macos/Sources/Settings/SettingsDiff.swift`, `macos/Sources/Settings/SettingsDefaults.swift`, `macos/Sources/AppDelegate.swift`
- Test : `macos/Tests/ReadingDisplayTests.swift`, `macos/Tests/SettingsDiffTests.swift`, `macos/Tests/FakeSettingsBackend.swift`

**Interfaces:**
- Consumes : `SettingsView.readingFont: String`, `.readingSize: Float`, `SettingsPatch.readingFont: String?`, `.readingSize: Float?` (plan 1) ; `SettingsBackend` (existant) ; `Snapshot.configUnreadable`.
- Produces :
  - `ReadingDisplay { font: ReadingFont, size: CGFloat, lineSpacing, readingRuler }`
  - `enum ReadingPreferencesMigration { static let fontKey = "readingFont"; static let sizeKey = "readingSize"; static func patch(from: UserDefaults) -> SettingsPatch?; static func run(defaults:backend:configUnreadable:) async -> Bool }`
  - `SettingsDiff` envoie `readingFont` et `readingSize`
  - `DisplayDefaults` n'existe plus

- [ ] **Step 1 : écrire les tests qui échouent.**

`macos/Tests/ReadingPreferencesMigrationTests.swift` :

```swift
import XCTest
@testable import Sillon

/// Police et taille : des préférences vers config.toml, une fois.
@MainActor
final class ReadingPreferencesMigrationTests: XCTestCase {
    private func freshDefaults() -> UserDefaults {
        let name = "sillon.tests.reprise-lecture"
        let defaults = UserDefaults(suiteName: name)!
        defaults.removePersistentDomain(forName: name)
        return defaults
    }

    func testNothingToTakeBack() {
        XCTAssertNil(ReadingPreferencesMigration.patch(from: freshDefaults()))
    }

    func testTheValuesAreTakenAsTheyAre() {
        let defaults = freshDefaults()
        defaults.set("openDyslexic", forKey: ReadingPreferencesMigration.fontKey)
        defaults.set(28.0, forKey: ReadingPreferencesMigration.sizeKey)
        let patch = ReadingPreferencesMigration.patch(from: defaults)
        XCTAssertEqual(patch?.readingFont, "openDyslexic")
        XCTAssertEqual(patch?.readingSize, 28)
        XCTAssertNil(patch?.speed, "rien d'autre n'est touché")
    }

    func testOddValuesAreBoundedOrIgnored() {
        let defaults = freshDefaults()
        defaults.set("comic-sans", forKey: ReadingPreferencesMigration.fontKey)
        defaults.set(400.0, forKey: ReadingPreferencesMigration.sizeKey)
        let patch = ReadingPreferencesMigration.patch(from: defaults)
        XCTAssertNil(patch?.readingFont, "une police inconnue est ignorée")
        XCTAssertEqual(patch?.readingSize, 40, "la taille est bornée")
        defaults.set(Double.nan, forKey: ReadingPreferencesMigration.sizeKey)
        defaults.removeObject(forKey: ReadingPreferencesMigration.fontKey)
        XCTAssertNil(ReadingPreferencesMigration.patch(from: defaults))
    }

    func testTheValuesAreWrittenOnceThenForgotten() async {
        let defaults = freshDefaults()
        defaults.set("atkinson", forKey: ReadingPreferencesMigration.fontKey)
        defaults.set(18.0, forKey: ReadingPreferencesMigration.sizeKey)
        let backend = FakeSettingsBackend()
        let moved = await ReadingPreferencesMigration.run(defaults: defaults, backend: backend, configUnreadable: false)
        XCTAssertTrue(moved)
        XCTAssertEqual(backend.stored.readingFont, "atkinson")
        XCTAssertEqual(backend.stored.readingSize, 18)
        XCTAssertNil(defaults.object(forKey: ReadingPreferencesMigration.fontKey))
        XCTAssertNil(defaults.object(forKey: ReadingPreferencesMigration.sizeKey))
        let again = await ReadingPreferencesMigration.run(defaults: defaults, backend: backend, configUnreadable: false)
        XCTAssertFalse(again)
        XCTAssertEqual(backend.patches.count, 1, "une seule fois")
    }

    /// Écriture refusée, ou config.toml illisible (le moteur n'y écrit pas) :
    /// les préférences restent pour le lancement suivant.
    func testAFailedWriteKeepsThePreferencesForTheNextLaunch() async {
        let defaults = freshDefaults()
        defaults.set("atkinson", forKey: ReadingPreferencesMigration.fontKey)
        let backend = FakeSettingsBackend()
        backend.failNext = "disque plein"
        let refused = await ReadingPreferencesMigration.run(defaults: defaults, backend: backend, configUnreadable: false)
        XCTAssertFalse(refused)
        XCTAssertEqual(defaults.string(forKey: ReadingPreferencesMigration.fontKey), "atkinson")
        let unreadable = await ReadingPreferencesMigration.run(defaults: defaults, backend: backend, configUnreadable: true)
        XCTAssertFalse(unreadable)
        XCTAssertEqual(backend.patches.count, 1, "rien envoyé sur un fichier illisible")
        XCTAssertEqual(defaults.string(forKey: ReadingPreferencesMigration.fontKey), "atkinson")
    }
}
```

`macos/Tests/ReadingDisplayTests.swift`, ajouter :

```swift
    /// Police et taille viennent de config.toml, comme l'interlignage.
    func testFontAndSizeComeFromTheSettings() {
        var s = SettingsView.defaults
        XCTAssertEqual(ReadingDisplay(s).font, .literata)
        XCTAssertEqual(ReadingDisplay(s).size, 22)
        s.readingFont = "atkinson"; s.readingSize = 31
        XCTAssertEqual(ReadingDisplay(s).font, .atkinson)
        XCTAssertEqual(ReadingDisplay(s).size, 31)
        s.readingFont = "inconnue"; s.readingSize = .nan
        XCTAssertEqual(ReadingDisplay(s).font, .literata, "police inconnue : la police par défaut")
        XCTAssertEqual(ReadingDisplay(s).size, 22, "taille illisible : la taille par défaut")
    }
```

`macos/Tests/SettingsDiffTests.swift`, ajouter (dans la classe existante) :

```swift
    func testFontAndSizeAreSentAlone() {
        let old = SettingsView.defaults
        var new = old
        new.readingFont = "openDyslexic"
        new.readingSize = 26
        let patch = SettingsDiff.patch(from: old, to: new)
        XCTAssertEqual(patch.readingFont, "openDyslexic")
        XCTAssertEqual(patch.readingSize, 26)
        XCTAssertNil(patch.lineSpacing)
    }
```

`macos/Tests/FakeSettingsBackend.swift`, dans `updateSettings`, après `if let v = patch.codeBlocks { … }` :

```swift
        if let v = patch.readingFont { stored.readingFont = v }
        if let v = patch.readingSize { stored.readingSize = min(max(v, 14), 40) }
        if let v = patch.readingRuler { stored.readingRuler = v }
```

- [ ] **Step 2 : vérifier l'échec** — **`TEST`** `/ReadingPreferencesMigrationTests`. Attendu : échec de compilation (`ReadingPreferencesMigration` inconnu).

- [ ] **Step 3 : implémenter.**

`macos/Sources/Settings/ReadingPreferencesMigration.swift` :

```swift
import Foundation

/// Police et taille vivaient dans les préférences de l'app ; elles vivent
/// désormais dans config.toml. Au lancement, ce qui reste des préférences y
/// est écrit, puis effacé — seulement si l'écriture a eu lieu.
@MainActor
enum ReadingPreferencesMigration {
    static let fontKey = "readingFont"
    static let sizeKey = "readingSize"

    /// Le patch qui reprend les préférences : une police connue telle
    /// quelle, une taille bornée à 14–40 ; nil s'il n'y a rien à reprendre.
    static func patch(from defaults: UserDefaults) -> SettingsPatch? {
        var patch = SettingsPatch.empty
        if let font = defaults.string(forKey: fontKey), ReadingFont(rawValue: font) != nil {
            patch.readingFont = font
        }
        if defaults.object(forKey: sizeKey) != nil {
            let size = defaults.double(forKey: sizeKey)
            if size.isFinite, size > 0 { patch.readingSize = Float(min(max(size, 14), 40)) }
        }
        return patch.isEmpty ? nil : patch
    }

    /// Vrai si des valeurs ont été écrites. config.toml illisible : le
    /// moteur n'y écrit rien, les préférences attendent le lancement suivant.
    @discardableResult
    static func run(defaults: UserDefaults, backend: SettingsBackend, configUnreadable: Bool) async -> Bool {
        guard let patch = patch(from: defaults) else {
            forget(defaults)
            return false
        }
        guard !configUnreadable else { return false }
        do {
            _ = try await backend.updateSettings(patch)
        } catch {
            return false
        }
        forget(defaults)
        return true
    }

    private static func forget(_ defaults: UserDefaults) {
        defaults.removeObject(forKey: fontKey)
        defaults.removeObject(forKey: sizeKey)
    }
}
```

`macos/Sources/Panel/ReadingDisplay.swift` (remplace le fichier) :

```swift
import CoreGraphics

/// Ce que le lecteur lit des réglages du moteur (config.toml), relu à
/// chaque ouverture et à chaque réglage écrit.
struct ReadingDisplay: Equatable {
    var font: ReadingFont = .literata
    var size: CGFloat = 22
    var lineSpacing: UInt32 = 1
    var readingRuler = true

    init() {}

    init(_ s: SettingsView) {
        font = ReadingFont(rawValue: s.readingFont) ?? .literata
        size = s.readingSize.isFinite ? CGFloat(min(max(s.readingSize, 14), 40)) : 22
        lineSpacing = s.lineSpacing
        readingRuler = s.readingRuler
    }
}
```

`macos/Sources/Panel/PanelContent.swift` : supprimer les deux `@AppStorage` (lignes 6-7) ; `style` devient :

```swift
    private var style: ReadingStyle {
        ReadingStyle(font: controller.reading.font,
                     size: controller.reading.size,
                     lineSpacing: controller.reading.lineSpacing,
                     ruler: controller.reading.readingRuler,
                     theme: theme)
    }
```

`macos/Sources/Style/ReadingFont.swift` : supprimer `enum DisplayDefaults { … }` (lignes 4-8).

`macos/Sources/Settings/ReadingTab.swift` : supprimer les deux `@AppStorage` ; la police et la taille passent par le magasin :

```swift
                SettingRow(title: "Police de lecture", detail: "Le texte lu, dans le lecteur") {
                    PopUpPicker(label: "Police de lecture", identifier: "police-lecture",
                                options: ReadingFont.allCases.map { ($0.label, $0.rawValue) },
                                selection: Binding(get: { ReadingFont(rawValue: store.values.readingFont)?.rawValue
                                                              ?? ReadingFont.literata.rawValue },
                                                   set: { store.set(\.readingFont, $0) }))
                        .fixedSize()
                }
                SettingRow(title: "Taille") {
                    Slider(value: store.binding(\.readingSize), in: 14...40, step: 1) {
                        Text("Taille : \(Int(store.values.readingSize)) pt")
                    }
                    .notionSlider()
                    .accessibilityLabel("Taille du texte")
                    .accessibilityIdentifier("taille-texte")
                    .accessibilityValue("\(Int(store.values.readingSize)) points")
                    ValueText("\(Int(store.values.readingSize)) pt")
                }
```

`macos/Sources/Settings/SettingsDiff.swift`, après la ligne `readingRuler` :

```swift
        if new.readingFont != old.readingFont { p.readingFont = new.readingFont }
        if new.readingSize != old.readingSize { p.readingSize = new.readingSize }
```

`macos/Sources/Settings/SettingsDefaults.swift`, `empty` devient (les deux champs du plan 1 posés explicitement) :

```swift
    static var empty: SettingsPatch {
        SettingsPatch(
            voice: nil, language: nil, speed: nil, pitch: nil, volume: nil, engine: nil,
            threads: nil, sessions: nil, prefetch: nil, codeBlocks: nil, autoplay: nil,
            seekStep: nil, lineWidth: nil, lineSpacing: nil, readingRuler: nil,
            keepVoiceMinutes: nil, outputDevice: nil, voiceForLanguage: nil,
            readingFont: nil, readingSize: nil)
    }
```

`macos/Sources/AppDelegate.swift`, après `guard !isUITesting, !isUnitTesting else { return }` (jamais sous les tests : les préférences réelles ne sont pas touchées), avant `installShortcuts()` :

```swift
        // Police et taille : des préférences vers config.toml, une fois.
        Task { @MainActor [model = model!, panel = panel!] in
            model.refresh()
            if await ReadingPreferencesMigration.run(defaults: .standard, backend: model,
                                                     configUnreadable: model.snapshot.configUnreadable) {
                panel.reloadReading()
            }
        }
```

`grep -rn "DisplayDefaults\|AppStorage" macos/Sources` : attendu, rien.

- [ ] **Step 4 : vérifier** — **`TEST`** `/ReadingPreferencesMigrationTests`, `/ReadingDisplayTests`, `/SettingsDiffTests`, `/SettingsStoreTests` : PASS ; suite entière : PASS.

- [ ] **Step 5 : commit**

```bash
git add macos/Sources/Settings/ReadingPreferencesMigration.swift macos/Sources/Panel/ReadingDisplay.swift \
        macos/Sources/Panel/PanelContent.swift macos/Sources/Style/ReadingFont.swift macos/Sources/Settings/ReadingTab.swift \
        macos/Sources/Settings/SettingsDiff.swift macos/Sources/Settings/SettingsDefaults.swift macos/Sources/AppDelegate.swift \
        macos/Tests/ReadingPreferencesMigrationTests.swift macos/Tests/ReadingDisplayTests.swift \
        macos/Tests/SettingsDiffTests.swift macos/Tests/FakeSettingsBackend.swift
git commit -m "$(cat <<'EOF'
feat(macos): police et taille de lecture dans config.toml, un seul stockage — les préférences existantes y sont reprises au lancement, puis effacées, seulement si l'écriture a eu lieu

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Tâche 9 : les réglages sur une page

Une fenêtre unique, sans barre latérale ni onglets : **Voix** (voix française et anglaise avec « Écouter », vitesse, sortie audio), **Lecture** (police, taille, interlignage, règle), **Raccourcis** (les deux), **Ouvrir sillon à la connexion**. Le reste devient automatique : les onglets Modèles, Avancé, Général, la voix gardée, les blocs de code, « lire dès l'ouverture », le pas d'avance, le volume, la langue du document, le cache, le dossier de configuration et les signets n'ont plus d'interface (leurs clés de `config.toml` gardent leur valeur).

**Files:**
- Create : `macos/Sources/Settings/SettingsSections.swift`
- Modify : `macos/Sources/Settings/ReglagesView.swift` (réécrit), `macos/Sources/Settings/SettingsOpener.swift`, `macos/Sources/App/AppServices.swift` (`OpenSettingsTabAction` retiré), `macos/Sources/Panel/PanelController.swift` (`openSettings: () -> Void`), `macos/Sources/Panel/PanelContent.swift:28`, `macos/Sources/AppDelegate.swift`, `macos/Sources/Model/ReaderModel+Engine.swift`, `macos/Sources/Model/UITestStaging.swift`, `macos/Sources/SillonApp.swift:39-44`
- Delete : `macos/Sources/Settings/SettingsTab.swift`, `VoiceTab.swift`, `ReadingTab.swift`, `ShortcutsTab.swift`, `GeneralTab.swift`, `AdvancedTab.swift`, `ModelsTab.swift`, `ModelsStore.swift`, `KeepVoiceChoice.swift`, `Choices.swift` (tous dans `macos/Sources/Settings/`) ; `macos/Tests/ModelsStoreTests.swift`, `FakeModelsBackend.swift`, `AdvancedTabTests.swift`, `GeneralTabTests.swift`, `KeepVoiceChoiceTests.swift`, `ReadingTabTests.swift` (dans `macos/Tests/`)
- Rename : `macos/Tests/VoiceTabTests.swift` → `macos/Tests/VoiceSectionTests.swift`
- Test : `macos/Tests/SettingsSectionsTests.swift` (nouveau), `macos/Tests/SettingsOpenerTests.swift`, `macos/UITests/SettingsUITests.swift` (réécrit), `macos/UITests/KokoroVoiceUITests.swift` (réécrit)

**Interfaces:**
- Consumes : `SettingsStore` (existant), `ReadingDisplay` et la police / taille dans le magasin (Tâche 8), `.readSelection` / `.showPanel` (Tâche 7).
- Produces :
  - `struct VoiceSection(model:store:)` avec les fonctions statiques de l'ancien `VoiceTab` : `followsLanguage`, `showsKokoroVoices`, `suggestsModels`, `voices`, `voice(for:in:_:)`, `voiceOptions`, `previewLine`
  - `struct ReadingSection(store:)` avec `static let fontOptions: [(title: String, value: String)]` et `static func shownFont(_ stored: String) -> String`
  - `struct ShortcutsSection`, `struct StartupSection`
  - `SettingsOpener.open()` (sans onglet) ; `PanelController.init(model:store:openSettings: @escaping @MainActor () -> Void = {}, …)`
  - identifiants : `page-reglages`, `voix-francaise`, `voix-anglaise`, `ecouter-fr`, `ecouter-en`, `vitesse`, `sortie-audio`, `police-lecture`, `taille-texte`, `interlignage`, `regle-lecture`, `raccourci-lecture-selection`, `raccourci-afficher`, `ouvrir-connexion`
  - `ReaderModel+Engine` ne garde que : `call`, `tryCall`, `loadCapabilities`, `previewVoice`, `stopPreview`, `migrateVoice`, `replaceUnreadableConfig`
  - `UITestStaging` : plus d'offre ni d'avis (`--ui-testing-offre`, `--ui-testing-avis` retirés)

- [ ] **Step 1 : écrire les tests qui échouent.**

`git mv macos/Tests/VoiceTabTests.swift macos/Tests/VoiceSectionTests.swift`, puis dans ce fichier : classe `VoiceSectionTests`, `VoiceTab.` → `VoiceSection.` partout ; supprimer `testTheDocumentLanguageSaysWhatWasDetected`, `testAMixedDocumentIsSaidWithItsMajority`, `testTheFirstPreparationIsSaid`, `testTheOfferSaysItsSizeTheFrenchWay`, `testTheLaunchNoticeOfAnOfferIsTheApps`, `testAFailedAcceptedDownloadIsSaid`, `testTheVoiceStateLine`, `testTheFallbackIsSaidOnceWithItsReason` ; `testUITestArgumentsStageTheOfferNoticeAndUnreadableConfig` devient :

```swift
    /// Les tests d'interface (compilés seulement) atteignent le bandeau et
    /// les voix de Kokoro sous le lecteur muet.
    func testUITestArgumentsStageTheUnreadableConfigAndKokoro() {
        XCTAssertNil(UITestStaging.staging(for: ["--ui-testing"]))
        var s = Snapshot.sample()
        UITestStaging.staging(for: ["--ui-testing", "--ui-testing-config-illisible"])?(&s)
        XCTAssertTrue(s.configUnreadable)
        XCTAssertNil(UITestStaging.capabilities(for: ["--ui-testing"]))
        let kokoro = UITestStaging.capabilities(for: ["--ui-testing", "--ui-testing-kokoro"])
        XCTAssertEqual(kokoro?.family, "kokoro")
        XCTAssertEqual(kokoro?.voices.map(\.id), Capabilities.kokoro.voices.map(\.id))
    }
```

`macos/Tests/SettingsSectionsTests.swift` :

```swift
import XCTest
@testable import Sillon

@MainActor
final class SettingsSectionsTests: XCTestCase {
    /// Les trois polices, sous leur nom ; la valeur est celle de config.toml.
    func testTheFontsAreOfferedByName() {
        XCTAssertEqual(ReadingSection.fontOptions.map(\.title), ["Literata", "Atkinson Hyperlegible", "OpenDyslexic"])
        XCTAssertEqual(ReadingSection.fontOptions.map(\.value), ["literata", "atkinson", "openDyslexic"])
    }

    /// Une police écrite à la main et inconnue se montre comme celle qui
    /// s'affiche vraiment : Literata.
    func testAnUnknownFontShowsAsTheDefault() {
        XCTAssertEqual(ReadingSection.shownFont("atkinson"), "atkinson")
        XCTAssertEqual(ReadingSection.shownFont("comic-sans"), "literata")
    }

    /// La page porte les deux raccourcis globaux, pas plus.
    func testTheShortcutsAreTheTwoGlobalOnes() {
        XCTAssertEqual(ShortcutsSection.rows.map(\.identifier), ["raccourci-lecture-selection", "raccourci-afficher"])
        XCTAssertEqual(ShortcutsSection.rows.map(\.title), ["Lire la sélection", "Afficher / masquer le lecteur"])
    }
}
```

`macos/Tests/SettingsOpenerTests.swift` : supprimer `freshDefaults()`, `testOpeningOnATabSelectsItAndPlainOpeningKeepsTheLast`, `testTheLastTabSurvivesARelaunch`, `testUnderTestsTheTabIsNotStoredInTheUserDefaults` (plus d'onglet) ; les trois premiers tests restent tels quels.

- [ ] **Step 2 : vérifier l'échec** — **`TEST`** `/SettingsSectionsTests`. Attendu : échec de compilation (`ReadingSection`, `ShortcutsSection`, `VoiceSection` inconnus).

- [ ] **Step 3 : implémenter.**

`git rm` des fichiers listés en « Delete ».

`macos/Sources/Settings/SettingsSections.swift` :

```swift
import KeyboardShortcuts
import SwiftUI

/// Voix : une voix et un « Écouter » par langue (quand Kokoro parle), la
/// vitesse, la sortie audio.
struct VoiceSection: View {
    let model: ReaderModel
    @Bindable var store: SettingsStore

    var body: some View {
        SettingsGroup(title: "Voix") {
            if Self.showsKokoroVoices(store.capabilities, voice: model.snapshot.voice) {
                languageVoicePicker("fr", title: "Voix française", identifier: "voix-francaise")
                languageVoicePicker("en", title: "Voix anglaise", identifier: "voix-anglaise")
                if let line = Self.previewLine(model.snapshot.preview, refusal: store.previewNote) {
                    HStack {
                        Text(line)
                            .font(Typo.detail)
                            .foregroundStyle(Palette.muted)
                            .accessibilityIdentifier("etat-ecoute")
                        Spacer()
                        Button("Arrêter") { Task { await store.stopListening() } }
                            .buttonStyle(.notionPlain)
                            .disabled(!(model.snapshot.preview.pending || model.snapshot.preview.playing))
                            .accessibilityLabel("Arrêter l'écoute")
                    }
                    .padding(.vertical, 4)
                }
            }
            SettingRow(title: "Vitesse", detail: "Dans le lecteur : + et −, ou la vitesse de la barre") {
                Slider(value: Binding(get: { store.values.speed },
                                      set: { store.set(\.speed, SettingFormat.rounded($0)) }),
                       in: 0.5...3, step: 0.05) {
                    Text("Vitesse : \(SettingFormat.decimal(store.values.speed))×")
                }
                .notionSlider()
                .accessibilityLabel("Vitesse de lecture")
                .accessibilityIdentifier("vitesse")
                .accessibilityValue("\(SettingFormat.decimal(store.values.speed)) fois")
                ValueText("\(SettingFormat.decimal(store.values.speed))×")
            }
            SettingRow(title: "Sortie audio",
                       detail: "Le son ne sort que par là : débranchée, la lecture se met en pause") {
                PopUpPicker(label: "Sortie audio", identifier: "sortie-audio",
                            options: AudioOutputChoices.choices(model.outputs, chosen: model.outputDevice)
                                .map { ($0.title, $0.value) },
                            selection: Binding(get: { model.outputDevice }, set: { model.chooseOutput($0) }))
                    .frame(width: 240)
            }
        }
        // Un aperçu qui échoue après coup (synthèse refusée) se dit aussi.
        .onChange(of: model.snapshot.preview.error) { _, error in
            if let error { Announcement.post(error) }
        }
    }

    /// Une voix par langue (Kokoro), chacune avec son « Écouter ».
    @ViewBuilder private func languageVoicePicker(_ language: String, title: String, identifier: String) -> some View {
        SettingRow(title: title, detail: language == "fr" ? "Lit les passages en français" : "Lit les passages en anglais") {
            HStack(spacing: 8) {
                PopUpPicker(label: title, identifier: identifier,
                            options: Self.voiceOptions(store.capabilities, language: language),
                            selection: Binding(get: { Self.voice(for: language, in: store.values, store.capabilities) },
                                               set: { store.setVoice($0, language: language) }))
                    .frame(width: 220)
                Button {
                    Task { await store.listen(language: language) }
                } label: {
                    HStack(spacing: 5) {
                        Image(systemName: "play.fill").font(.system(size: 10))
                        Text("Écouter")
                    }
                }
                .buttonStyle(.notion)
                .fixedSize()
                .accessibilityLabel("Écouter la \(title.lowercased())")
                .accessibilityIdentifier("ecouter-\(language)")
            }
        }
    }

    /// Kokoro choisit sa voix par langue.
    static func followsLanguage(_ c: ModelCapabilities) -> Bool { c.family == "kokoro" }

    /// Les voix Kokoro ne se montrent que si Kokoro parle vraiment : sinon
    /// « Écouter la voix française » ferait entendre la voix du système.
    static func showsKokoroVoices(_ c: ModelCapabilities, voice: VoiceState) -> Bool {
        followsLanguage(c) && !suggestsModels(voice)
    }

    /// Voix de repli ou voix muette.
    static func suggestsModels(_ state: VoiceState) -> Bool {
        switch state {
        case .fallback, .unavailable: true
        default: false
        }
    }

    static func voices(_ c: ModelCapabilities, language: String) -> [VoiceChoice] {
        c.voices.filter { $0.language == language }
    }

    /// La voix en place pour une langue ; rien de choisi, ou une voix qui
    /// n'est plus proposée : la recommandée (la première de la langue).
    static func voice(for language: String, in values: SettingsView, _ c: ModelCapabilities) -> String {
        let listed = voices(c, language: language)
        let chosen = values.voicesByLanguage.first(where: { $0.language == language })?.voice
        return listed.first(where: { $0.id == chosen })?.id ?? listed.first?.id ?? ""
    }

    static func voiceOptions(_ c: ModelCapabilities, language: String) -> [(title: String, value: String)] {
        voices(c, language: language).map { (title: $0.label, value: $0.id) }
    }

    static func previewLine(_ preview: PreviewView, refusal: String?) -> String? {
        if let refusal { return refusal }
        if let error = preview.error { return error }
        if preview.pending { return "Synthèse de l'aperçu…" }
        if preview.playing { return "Écoute…" }
        return nil
    }
}

/// Lecture : police, taille, interlignage, règle de lecture.
struct ReadingSection: View {
    @Bindable var store: SettingsStore

    static let fontOptions: [(title: String, value: String)] = ReadingFont.allCases.map { ($0.label, $0.rawValue) }

    /// La police telle qu'elle s'affiche : une valeur inconnue, Literata.
    static func shownFont(_ stored: String) -> String {
        ReadingFont(rawValue: stored)?.rawValue ?? ReadingFont.literata.rawValue
    }

    var body: some View {
        SettingsGroup(title: "Lecture") {
            SettingRow(title: "Police") {
                PopUpPicker(label: "Police de lecture", identifier: "police-lecture", options: Self.fontOptions,
                            selection: Binding(get: { Self.shownFont(store.values.readingFont) },
                                               set: { store.set(\.readingFont, $0) }))
                    .fixedSize()
            }
            SettingRow(title: "Taille") {
                Slider(value: store.binding(\.readingSize), in: 14...40, step: 1) {
                    Text("Taille : \(Int(store.values.readingSize)) pt")
                }
                .notionSlider()
                .accessibilityLabel("Taille du texte")
                .accessibilityIdentifier("taille-texte")
                .accessibilityValue("\(Int(store.values.readingSize)) points")
                ValueText("\(Int(store.values.readingSize)) pt")
            }
            SettingRow(title: "Interlignage", detail: "L'air entre deux lignes") {
                ValueText("\(store.values.lineSpacing)", width: 20)
                Stepper("Interlignage : \(store.values.lineSpacing)",
                        value: Binding(get: { Int(store.values.lineSpacing) },
                                       set: { store.set(\.lineSpacing, UInt32(max(0, $0))) }),
                        in: 0...3)
                    .labelsHidden()
                    .accessibilityLabel("Interlignage")
                    .accessibilityIdentifier("interlignage")
                    .accessibilityValue("\(store.values.lineSpacing)")
            }
            SettingRow(title: "Règle de lecture", detail: "Une bande sous la ligne lue, pour ne pas la perdre") {
                Toggle("Règle de lecture", isOn: store.binding(\.readingRuler))
                    .notionSwitch()
                    .accessibilityIdentifier("regle-lecture")
            }
        }
    }
}

/// Raccourcis : les deux raccourcis globaux. Un raccourci déjà pris est
/// signalé par l'enregistreur lui-même.
struct ShortcutsSection: View {
    struct Row {
        let title: String
        let detail: String
        let name: KeyboardShortcuts.Name
        let identifier: String
    }

    static let rows = [
        Row(title: "Lire la sélection", detail: "Le texte sélectionné, sinon le presse-papiers",
            name: .readSelection, identifier: "raccourci-lecture-selection"),
        Row(title: "Afficher / masquer le lecteur", detail: "Dans sa dernière taille",
            name: .showPanel, identifier: "raccourci-afficher"),
    ]

    static let voiceOverNote = "⌃⌥ est aussi la touche de VoiceOver : avec VoiceOver actif, choisissez d'autres touches."

    var body: some View {
        SettingsGroup(title: "Raccourcis") {
            ForEach(Self.rows, id: \.identifier) { row in
                SettingRow(title: row.title, detail: row.detail) {
                    KeyboardShortcuts.Recorder(for: row.name)
                        .frame(width: 150)
                        .accessibilityLabel(row.title)
                        .accessibilityIdentifier(row.identifier)
                }
            }
            Callout(symbol: "info.circle", text: Self.voiceOverNote, identifier: "raccourcis-note-voiceover")
        }
    }
}

/// Ouvrir sillon à la connexion.
struct StartupSection: View {
    @State private var login = false

    var body: some View {
        SettingsGroup(title: "Démarrage") {
            SettingRow(title: "Ouvrir sillon à la connexion", detail: "Dans la barre des menus, prêt à lire") {
                Toggle("Ouvrir sillon à la connexion", isOn: $login)
                    .notionSwitch()
                    .onAppear { login = LoginItem.isEnabled }
                    .onChange(of: login) { _, on in
                        // L'inscription peut être refusée : l'interrupteur dit l'état réel.
                        if LoginItem.isEnabled != on, !LoginItem.set(on) { login = LoginItem.isEnabled }
                    }
                    .accessibilityLabel("Ouvrir sillon à la connexion")
                    .accessibilityIdentifier("ouvrir-connexion")
            }
        }
    }
}
```

`macos/Sources/Settings/ReglagesView.swift`, la vue `ReglagesView` (le bandeau `ConfigUnreadableBanner` et `HiddenWindowToolbar` restent tels quels) devient :

```swift
/// La fenêtre des réglages : une seule page — Voix, Lecture, Raccourcis,
/// Démarrage. Le nom `SettingsView` est pris par l'enregistrement du moteur.
struct ReglagesView: View {
    let model: ReaderModel
    @State private var store: SettingsStore
    /// Panneau masqué, l'état de la voix (aperçu, téléchargement) n'est plus
    /// relu : la fenêtre le relit à 4 Hz, tant qu'elle est visible seulement.
    @State private var refresh: RefreshLoop

    init(model: ReaderModel) {
        self.model = model
        _store = State(initialValue: SettingsStore(backend: model))
        _refresh = State(initialValue: RefreshLoop { model.refresh() })
    }

    var body: some View {
        VStack(spacing: 0) {
            ConfigUnreadableBanner(model: model, store: store)
            SettingsPage(title: "Réglages") {
                ErrorText(store.error, identifier: "erreur-reglages")
                VoiceSection(model: model, store: store)
                ReadingSection(store: store)
                ShortcutsSection()
                StartupSection()
            }
        }
        .background(Palette.page)
        // La barre de titre est transparente : la page monte jusqu'en haut.
        .ignoresSafeArea()
        .modifier(HiddenWindowToolbar())
        .navigationTitle("Réglages")
        .frame(width: 620, height: 680)
        .font(Typo.body)
        .accessibilityIdentifier("page-reglages")
        .task {
            AppDelegate.shared?.settingsStore = store
            await store.load()
        }
        // La relecture suit la fenêtre réelle ; fermée, ce qui attend sa pause part.
        .background(WindowVisibility(onVisible: { refresh.setVisible($0) },
                                     onClose: { closeWindow() }))
        .onDisappear { refresh.setVisible(false); closeWindow() }
        // La vitesse changée depuis le lecteur, resté visible à côté.
        .onChange(of: model.snapshot.speed) { _, _ in followReader() }
        // Une voix installée (téléchargement fini) : ses voix apparaissent.
        .onChange(of: ReaderModel.isDownloading(model.snapshot.download)) { was, running in
            if was, !running { Task { await store.reload() } }
        }
    }

    private func followReader() {
        store.followReader(speed: model.snapshot.speed, volume: model.snapshot.volume)
    }

    /// Fermer la fenêtre n'abandonne pas un changement qui attend sa pause.
    private func closeWindow() {
        store.endDragging()
        Task { await store.flush() }
    }
}
```

`macos/Sources/Settings/SettingsOpener.swift` : retirer `navigation` (propriété, paramètre d'`init` et usage) ; `open(tab:)` devient :

```swift
    /// Une app de barre des menus n'est pas au premier plan : on l'y met
    /// d'abord, sinon la fenêtre s'ouvrirait derrière l'application active.
    func open() {
        activate()
        if let action { action() } else { pending = true }
    }
```

`macos/Sources/App/AppServices.swift` : supprimer `OpenSettingsTabAction` et l'extension `EnvironmentValues` (plus aucune vue ne s'en sert depuis la Tâche 4).

`macos/Sources/Panel/PanelController.swift` : `openSettings: @MainActor (SettingsTab?) -> Void` → `openSettings: @MainActor () -> Void` (propriété et paramètre, défaut `{}`) ; supprimer `openSettingsAction` ; dans `perform`, `case .openSettings: openSettings()`.

`macos/Sources/Panel/PanelContent.swift` : supprimer `.environment(\.openSettingsTab, controller.openSettingsAction)`.

`macos/Sources/AppDelegate.swift` :
- `panel = PanelController(model: model, openSettings: { settings.open() }, …)` ;
- dans `startKokoro()`, la tâche devient :

```swift
        migration = Task { @MainActor [weak self] in
            guard let self, let model = self.model else { return }
            // L'avis du passage (s'il y en a un) se dit une fois dans le lecteur.
            if let notice = await model.migrateVoice() {
                model.message = notice
                self.panel.post(notice)
            }
            self.kokoroBecameActive()
        }
```

- supprimer `launchNotice(engine:offer:)`.

`macos/Sources/SillonApp.swift`, le commentaire de `SettingsWindowName` : « Le titre de la fenêtre (masqué, lu par Mission Control) est « Réglages » ; … » (plus de page qui change).

`macos/Sources/Model/ReaderModel+Engine.swift` : supprimer `// MARK: - Signets` et ses deux fonctions, `loadVoices`, `installVoiceAsync`, `useVoiceAsync`, `cancelDownloadAsync`, `removeModelAsync`, `freeSpace`, `setDocumentLanguage`, `acceptVoiceOffer`, `dismissVoiceOffer`, `dismissNotice`, `// MARK: - Fichiers` et ses deux membres ; le commentaire d'en-tête devient « Les appels au moteur que les réglages et la voix font par le modèle. … ».

`macos/Sources/Model/UITestStaging.swift` : supprimer `offer`, `notice` et leurs branches ; `staging(for:)` ne pose plus que `configUnreadable` :

```swift
    static func staging(for arguments: [String]) -> (@Sendable (inout Snapshot) -> Void)? {
        guard arguments.contains(unreadableConfig) else { return nil }
        return { s in s.configUnreadable = true }
    }
```

`grep -rn "SettingsTab\|SettingsNavigation\|VoiceTab\|ModelsStore\|KeepVoiceChoice\|openSettingsTab\|launchNotice\|loadAllBookmarks\|StoredBookmarkView" macos/Sources macos/Tests` : attendu, rien.

- [ ] **Step 4 : vérifier** — **`TEST`** `/SettingsSectionsTests`, `/VoiceSectionTests`, `/SettingsOpenerTests`, `/SettingsStoreTests` : PASS ; suite entière : PASS.

- [ ] **Step 5 : tests d'interface** — `macos/UITests/SettingsUITests.swift` (remplace le fichier ; garder les aides `launch`, `focused`, `identity`, `fullKeyboardAccess` de l'ancien fichier, retirer `tabs`, `waitForContent`, `modelButtons`) :

```swift
    private let present = ["vitesse", "sortie-audio", "police-lecture", "taille-texte", "interlignage",
                           "regle-lecture", "raccourci-lecture-selection", "raccourci-afficher", "ouvrir-connexion"]
    private let removed = ["page-voix", "page-modeles", "page-avance", "page-general", "moteur", "garder-voix",
                           "blocs-code", "langue-document", "espace-libre", "offre-voix"]

    /// Une seule page : tout ce que la spec garde, rien de ce qu'elle retire ;
    /// l'audit en haut et en bas.
    func testEverythingFitsOnOnePageAndPassesTheAudit() throws {
        let app = launch(["--show-settings"])
        let window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        let any = window.descendants(matching: .any)
        for id in present { XCTAssertTrue(any[id].waitForExistence(timeout: 3), "\(id) absent") }
        for id in removed { XCTAssertFalse(any[id].exists, "\(id) est revenu") }
        XCTAssertFalse(window.buttons["Avancé"].exists, "plus de barre latérale")
        try app.performAccessibilityAuditOutsideTouchBar(visibleIn: window)
        let page = window.scrollViews.firstMatch
        if page.exists {
            page.scroll(byDeltaX: 0, deltaY: -5000)
            try app.performAccessibilityAuditOutsideTouchBar(visibleIn: window)
        }
    }

    /// La page se parcourt au clavier.
    func testThePageIsKeyboardReachable() throws {
        guard fullKeyboardAccess else {
            throw XCTSkip("Activer Réglages Système › Clavier › Navigation au clavier pour ce test")
        }
        let app = launch(["--show-settings"])
        let window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        var seen = Set<String>()
        for _ in 0..<8 {
            window.typeKey("\t", modifierFlags: [])
            let element = focused(in: window)
            XCTAssertTrue(element.exists, "aucun contrôle n'a le focus")
            seen.insert(identity(element))
        }
        XCTAssertGreaterThan(seen.count, 4, "Tab ne parcourt pas la page")
    }

    /// ⌘, depuis le lecteur ouvre la page.
    func testCommandCommaFromTheReaderOpensTheSettings() throws {
        let app = launch(["--show-panel"])
        let reader = app.groups["lecteur"]
        XCTAssertTrue(reader.waitForExistence(timeout: 5))
        reader.typeKey(",", modifierFlags: .command)
        let window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        XCTAssertTrue(window.descendants(matching: .any)["page-reglages"].waitForExistence(timeout: 3))
    }
```

(commentaire de classe : « La page des réglages, au clavier et sous l'audit. Lancé à la main, avec l'accord de l'utilisateur : `-only-testing:SillonUITests/SettingsUITests`. »)

`macos/UITests/KokoroVoiceUITests.swift` : `voiceTab(_:)` devient `settings(_:)` sans le `window.buttons["Voix"].click()` ; supprimer `testTheOfferNoticeAndBannerAreReachableAndAudited` ; dans `testKokoroShowsOneVoicePerLanguageAndTheDocumentLanguage` (renommé `testKokoroShowsOneVoicePerLanguage`), supprimer le bloc `langue-document` et l'assertion `window.sliders["Volume"]` ; dans `assertNothingRemovedIsBack`, ajouter `"langue-document"` à la liste ; dans `testWithoutKokoroOnlySpeedAndVolumeRemain` (renommé `testWithoutKokoroOnlySpeedAndOutputRemain`), le volume n'est plus attendu, la sortie audio l'est (`window.popUpButtons["sortie-audio"]`). Ajouter :

```swift
    /// config.toml illisible : le bandeau est atteignable et passe l'audit.
    func testTheUnreadableConfigBannerIsAudited() throws {
        let (app, window) = settings(["--ui-testing-config-illisible"])
        XCTAssertTrue(window.descendants(matching: .any)["configuration-illisible"].waitForExistence(timeout: 3))
        XCTAssertTrue(window.buttons["remplacer-configuration"].exists)
        try app.performAccessibilityAuditOutsideTouchBar(visibleIn: window)
    }
```

**`UIBUILD`** : réussit.

- [ ] **Step 6 : commit**

```bash
git rm macos/Sources/Settings/SettingsTab.swift macos/Sources/Settings/VoiceTab.swift macos/Sources/Settings/ReadingTab.swift \
       macos/Sources/Settings/ShortcutsTab.swift macos/Sources/Settings/GeneralTab.swift macos/Sources/Settings/AdvancedTab.swift \
       macos/Sources/Settings/ModelsTab.swift macos/Sources/Settings/ModelsStore.swift macos/Sources/Settings/KeepVoiceChoice.swift \
       macos/Sources/Settings/Choices.swift macos/Tests/ModelsStoreTests.swift macos/Tests/FakeModelsBackend.swift \
       macos/Tests/AdvancedTabTests.swift macos/Tests/GeneralTabTests.swift macos/Tests/KeepVoiceChoiceTests.swift \
       macos/Tests/ReadingTabTests.swift
git add macos/Sources/Settings/SettingsSections.swift macos/Sources/Settings/ReglagesView.swift \
        macos/Sources/Settings/SettingsOpener.swift macos/Sources/App/AppServices.swift macos/Sources/Panel/PanelController.swift \
        macos/Sources/Panel/PanelContent.swift macos/Sources/AppDelegate.swift macos/Sources/Model/ReaderModel+Engine.swift \
        macos/Sources/Model/UITestStaging.swift macos/Sources/SillonApp.swift \
        macos/Tests/VoiceSectionTests.swift macos/Tests/SettingsSectionsTests.swift macos/Tests/SettingsOpenerTests.swift \
        macos/UITests/SettingsUITests.swift macos/UITests/KokoroVoiceUITests.swift
git commit -m "$(cat <<'EOF'
feat(macos): les réglages sur une page — Voix (une voix et Écouter par langue, vitesse, sortie audio), Lecture (police, taille, interlignage, règle), les deux raccourcis, ouverture à la connexion ; onglets, modèles, moteur, voix gardée, blocs de code, cache, signets et langue du document n'ont plus d'interface

Les clés retirées gardent leur valeur dans config.toml.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Tâche 10 : la voix s'installe seule ; une ligne d'état seulement en cas de problème

Au lancement, après `migrateVoice()`, l'app appelle `ensureDefaultVoice()` : Kokoro se télécharge sans rien demander, la voix du système lit en attendant. La ligne d'état du lecteur et le bas des réglages ne disent quelque chose que si la voix n'est pas prête : téléchargement (avec sa progression), échec (avec « Réessayer »), voix indisponible, voix du système en repli, première préparation. Sinon rien.

**Files:**
- Create : `macos/Sources/App/VoiceStatus.swift`, `macos/Tests/VoiceStatusTests.swift`, `macos/UITests/VoiceStatusUITests.swift`
- Modify : `macos/Sources/Model/ReaderModel.swift` (`voiceSetupError`, `installDefaultVoice()`), `macos/Sources/Model/ReaderModel+Engine.swift` (`ensureDefaultVoice()`), `macos/Sources/AppDelegate.swift` (`startKokoro`), `macos/Sources/Panel/PanelContent.swift` (`PanelStatus`), `macos/Sources/Panel/ReaderView.swift`, `macos/Sources/Settings/ReglagesView.swift`, `macos/Sources/Model/UITestStaging.swift`
- Test : `macos/Tests/PanelControllerTests.swift` (`PanelStatusTests`), `macos/Tests/VoiceSectionTests.swift`

**Interfaces:**
- Consumes : `SillonReader.ensureDefaultVoice() throws` (plan 1) ; `Snapshot.download: DownloadView?` (`id`, `received`, `total`, `failed`) ; `ReaderModel.voicePreparation.isRunning` ; `SizeFormat.bytes` ; `SettingsStore.message(of:)`.
- Produces :
  - `enum VoiceStatus: Equatable { case downloading(received: UInt64, total: UInt64), failed(String), unavailable(String), fallback(String), preparing; static func current(snapshot:preparing:setupError:) -> VoiceStatus?; var text: String; var canRetry: Bool }`
  - `struct VoiceStatusLine(model:color:)` — identifiants `etat-voix`, `reessayer-voix`
  - `ReaderModel.voiceSetupError: String?` ; `ReaderModel.installDefaultVoice() async` ; `ReaderModel.ensureDefaultVoice() async throws`
  - `PanelStatus.line(message:snapshot:) -> String?` (les messages passagers seulement ; plus de voix)
  - arguments de test `--ui-testing-telechargement`, `--ui-testing-echec-voix`

- [ ] **Step 1 : écrire les tests qui échouent** — `macos/Tests/VoiceStatusTests.swift` :

```swift
import XCTest
@testable import Sillon

/// La ligne d'état de la voix : rien quand tout va bien.
@MainActor
final class VoiceStatusTests: XCTestCase {
    private func snapshot(voice: VoiceState = .ready, download: DownloadView? = nil) -> Snapshot {
        var s = Snapshot.sample(voice: voice)
        s.download = download
        return s
    }

    func testNothingIsSaidWhenTheVoiceIsFine() {
        for voice in [VoiceState.ready, .resting, .loading] {
            XCTAssertNil(VoiceStatus.current(snapshot: snapshot(voice: voice), preparing: false, setupError: nil),
                         "\(voice)")
        }
    }

    func testTheDownloadShowsItsProgress() {
        let d = DownloadView(id: "kokoro_82m_mlx", received: 100_000_000, total: 342_264_783, failed: nil)
        let status = VoiceStatus.current(snapshot: snapshot(voice: .fallback(reason: "Kokoro absent"), download: d),
                                         preparing: false, setupError: nil)
        XCTAssertEqual(status, .downloading(received: 100_000_000, total: 342_264_783))
        XCTAssertEqual(status?.text, "Installation de la voix : 100 Mo sur 342 Mo — la voix du système lit en attendant")
        XCTAssertEqual(status?.canRetry, false)
    }

    /// Un échec, du téléchargement ou de l'appel : « Réessayer ».
    func testAFailureOffersToRetry() {
        let d = DownloadView(id: "kokoro_82m_mlx", received: 5, total: 10, failed: "réseau indisponible")
        let failed = VoiceStatus.current(snapshot: snapshot(download: d), preparing: false, setupError: nil)
        XCTAssertEqual(failed, .failed("réseau indisponible"))
        XCTAssertEqual(failed?.text, "La voix ne s'est pas installée : réseau indisponible")
        XCTAssertEqual(failed?.canRetry, true)
        XCTAssertEqual(VoiceStatus.current(snapshot: snapshot(), preparing: false, setupError: "disque plein"),
                       .failed("disque plein"))
    }

    func testFallbackUnavailableAndFirstPreparationAreSaid() {
        XCTAssertEqual(VoiceStatus.current(snapshot: snapshot(voice: .fallback(reason: "Metal indisponible")),
                                           preparing: false, setupError: nil)?.text,
                       "Voix du système en repli : Metal indisponible")
        XCTAssertEqual(VoiceStatus.current(snapshot: snapshot(voice: .unavailable(reason: "aucune session")),
                                           preparing: false, setupError: nil)?.text,
                       "Voix indisponible : aucune session")
        XCTAssertEqual(VoiceStatus.current(snapshot: snapshot(), preparing: true, setupError: nil)?.text,
                       "Voix en préparation (première fois)…")
    }

    /// Les tests d'interface atteignent le téléchargement et l'échec.
    func testUITestArgumentsStageTheDownloadAndTheFailure() {
        var s = Snapshot.sample()
        UITestStaging.staging(for: ["--ui-testing-telechargement"])?(&s)
        XCTAssertNotNil(s.download)
        XCTAssertNil(s.download?.failed)
        var f = Snapshot.sample()
        UITestStaging.staging(for: ["--ui-testing-echec-voix"])?(&f)
        XCTAssertNotNil(f.download?.failed)
    }
}
```

Dans `macos/Tests/PanelControllerTests.swift`, `PanelStatusTests` devient (la voix a sa propre ligne) :

```swift
/// La ligne de message du lecteur : ce que l'app vient de dire, sinon ce
/// que le lecteur dit ; rien sur la voix (VoiceStatus s'en charge).
@MainActor
final class PanelStatusTests: XCTestCase {
    private func snapshot(status: String = "", voice: VoiceState = .ready) -> Snapshot {
        var s = Snapshot.sample(voice: voice)
        s.status = status
        return s
    }

    func testAppMessageWins() {
        XCTAssertEqual(PanelStatus.line(message: "Phrase copiée", snapshot: snapshot(status: "Sortie audio débranchée")),
                       "Phrase copiée")
    }

    func testReaderStatusShows() {
        XCTAssertEqual(PanelStatus.line(message: nil, snapshot: snapshot(status: "Sortie audio débranchée")),
                       "Sortie audio débranchée")
    }

    func testTheVoiceIsNotSaidHere() {
        XCTAssertNil(PanelStatus.line(message: nil, snapshot: snapshot(voice: .fallback(reason: "x"))))
        XCTAssertNil(PanelStatus.line(message: nil, snapshot: snapshot(voice: .loading)))
        XCTAssertNil(PanelStatus.line(message: nil, snapshot: snapshot()))
    }
}
```

- [ ] **Step 2 : vérifier l'échec** — **`TEST`** `/VoiceStatusTests`. Attendu : échec de compilation (`VoiceStatus` inconnu).

- [ ] **Step 3 : implémenter.**

`macos/Sources/App/VoiceStatus.swift` :

```swift
import SwiftUI

/// Ce qu'on dit de la voix, seulement quand elle n'est pas prête.
enum VoiceStatus: Equatable {
    case downloading(received: UInt64, total: UInt64)
    case failed(String)
    case unavailable(String)
    case fallback(String)
    case preparing

    /// Un échec d'abord (il attend « Réessayer »), puis le téléchargement
    /// (la voix du système lit pendant ce temps : pas de repli à dire en
    /// plus), puis la voix muette, le repli, la première préparation.
    static func current(snapshot s: Snapshot, preparing: Bool, setupError: String?) -> VoiceStatus? {
        if let failed = s.download?.failed { return .failed(failed) }
        if let setupError { return .failed(setupError) }
        if let d = s.download { return .downloading(received: d.received, total: d.total) }
        if case .unavailable(let reason) = s.voice { return .unavailable(reason) }
        if case .fallback(let reason) = s.voice { return .fallback(reason) }
        if preparing { return .preparing }
        return nil
    }

    var text: String {
        switch self {
        case .downloading(let received, let total):
            "Installation de la voix : \(SizeFormat.bytes(received)) sur \(SizeFormat.bytes(total)) — la voix du système lit en attendant"
        case .failed(let why): "La voix ne s'est pas installée : \(why)"
        case .unavailable(let why): "Voix indisponible : \(why)"
        case .fallback(let why): "Voix du système en repli : \(why)"
        case .preparing: "Voix en préparation (première fois)…"
        }
    }

    var canRetry: Bool {
        if case .failed = self { return true }
        return false
    }
}

/// La ligne, dans le lecteur et en bas des réglages : rien quand la voix va bien.
struct VoiceStatusLine: View {
    let model: ReaderModel
    var color: Color = Palette.muted

    var body: some View {
        if let status = VoiceStatus.current(snapshot: model.snapshot, preparing: model.voicePreparation.isRunning,
                                            setupError: model.voiceSetupError) {
            HStack(spacing: 8) {
                if case .downloading(let received, let total) = status {
                    ProgressView(value: Double(received), total: Double(max(total, 1)))
                        .frame(width: 100)
                        .accessibilityLabel("Installation de la voix")
                        .accessibilityValue(SettingFormat.percent(Float(received) / Float(max(total, 1))))
                }
                Text(status.text)
                    .lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("etat-voix")
                if status.canRetry {
                    Button("Réessayer") { Task { await model.installDefaultVoice() } }
                        .buttonStyle(.notion)
                        .accessibilityLabel("Réessayer l'installation de la voix")
                        .accessibilityIdentifier("reessayer-voix")
                }
            }
            .font(.system(size: 12))
            .foregroundStyle(color)
            .accessibilityElement(children: .contain)
        }
    }
}
```

`macos/Sources/Model/ReaderModel+Engine.swift`, ajouter sous `migrateVoice()` :

```swift
    /// L'installation automatique de la voix (plan 1) : accepte l'offre de
    /// Kokoro, ou télécharge le Kokoro recommandé, sans rien demander ;
    /// sans effet pendant un téléchargement. La progression : `snapshot.download`.
    func ensureDefaultVoice() async throws {
        try await tryCall { try $0.ensureDefaultVoice() }
        wake()
        onVoiceMayHaveChanged?()
    }
```

`macos/Sources/Model/ReaderModel.swift`, après `restVoice()` :

```swift
    /// Le dernier refus de l'installation automatique ; nil après une
    /// réussite. Avec « Réessayer » dans la ligne d'état.
    var voiceSetupError: String?

    /// Au lancement et à chaque « Réessayer ».
    func installDefaultVoice() async {
        do {
            try await ensureDefaultVoice()
            voiceSetupError = nil
        } catch {
            voiceSetupError = SettingsStore.message(of: error)
        }
    }
```

`macos/Sources/AppDelegate.swift`, dans la tâche de `startKokoro()`, entre l'avis de `migrateVoice()` et `self.kokoroBecameActive()` :

```swift
            // Puis la voix s'installe seule s'il le faut (rapide : le
            // téléchargement, lui, court à part).
            await model.installDefaultVoice()
```

et le commentaire de `startKokoro()` devient « Le passage à Kokoro, l'installation automatique de la voix, puis la préparation des noyaux Metal… ».

`macos/Sources/Panel/PanelContent.swift`, `PanelStatus` devient :

```swift
/// La ligne de message du lecteur : ce que l'app vient de dire (« Phrase
/// copiée »), sinon ce que dit le lecteur (« Sortie audio débranchée ») ;
/// la voix a sa ligne à elle (`VoiceStatusLine`).
enum PanelStatus {
    static func line(message: String?, snapshot: Snapshot) -> String? {
        if let message { return message }
        if !snapshot.status.isEmpty { return snapshot.status }
        return nil
    }
}
```

`macos/Sources/Panel/ReaderView.swift`, le bas de la pile devient :

```swift
            if let line = PanelStatus.line(message: model.message, snapshot: model.snapshot) {
                Text(line)
                    .font(.system(size: 13))
                    .lineLimit(2)
                    .foregroundStyle(Color(nsColor: style.theme.chrome))
                    .accessibilityIdentifier("etat-lecteur")
            }
            VoiceStatusLine(model: model, color: Color(nsColor: style.theme.chrome))
```

`macos/Sources/Settings/ReglagesView.swift`, dans la `VStack` de `body`, après `SettingsPage(…) { … }` :

```swift
            // En bas, discret : l'état de la voix si elle n'est pas prête.
            VoiceStatusLine(model: model)
                .padding(.horizontal, 36)
                .padding(.vertical, 10)
```

`macos/Sources/Model/UITestStaging.swift` :

```swift
    static let unreadableConfig = "--ui-testing-config-illisible"
    /// Un téléchargement de la voix en cours, puis en échec.
    static let downloading = "--ui-testing-telechargement"
    static let failedDownload = "--ui-testing-echec-voix"

    static func staging(for arguments: [String]) -> (@Sendable (inout Snapshot) -> Void)? {
        let unreadable = arguments.contains(unreadableConfig)
        let downloading = arguments.contains(Self.downloading)
        let failed = arguments.contains(failedDownload)
        guard unreadable || downloading || failed else { return nil }
        return { s in
            if unreadable { s.configUnreadable = true }
            if downloading || failed {
                s.download = DownloadView(id: "kokoro_82m_mlx", received: 100_000_000, total: 342_264_783,
                                          failed: failed ? "réseau indisponible" : nil)
            }
        }
    }
```

`grep -rn "fallbackHint\|preparingLine\|loadingLine\|preparing: model.voicePreparation" macos/Sources macos/Tests` : attendu, seulement `VoiceStatus.swift`.

- [ ] **Step 4 : vérifier** — **`TEST`** `/VoiceStatusTests`, `/PanelStatusTests`, `/VoiceSectionTests`, `/KokoroWarmUpTests` : PASS ; suite entière : PASS.

- [ ] **Step 5 : test d'interface** — `macos/UITests/VoiceStatusUITests.swift` :

```swift
import XCTest

/// La ligne d'état de la voix : rien quand elle va bien ; l'échec et
/// « Réessayer » dans le lecteur et en bas des réglages. Lancé à la main,
/// avec l'accord de l'utilisateur : `-only-testing:SillonUITests/VoiceStatusUITests`.
@MainActor
final class VoiceStatusUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    func testNothingIsSaidWhenTheVoiceIsFine() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--show-panel"]
        app.launch()
        XCTAssertTrue(app.groups["lecteur"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.descendants(matching: .any)["etat-voix"].exists)
    }

    func testAFailedInstallationOffersToRetryInTheReaderAndTheSettings() throws {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--show-panel", "--show-settings", "--ui-testing-echec-voix"]
        app.launch()
        let reader = app.groups["lecteur"]
        XCTAssertTrue(reader.waitForExistence(timeout: 5))
        XCTAssertTrue(reader.buttons["reessayer-voix"].waitForExistence(timeout: 2), "pas de Réessayer dans le lecteur")
        let window = app.windows["Réglages"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        XCTAssertTrue(window.buttons["reessayer-voix"].waitForExistence(timeout: 2), "pas de Réessayer dans les réglages")
        try app.performAccessibilityAuditOutsideTouchBar(visibleIn: window)
    }

    func testTheDownloadShowsItsProgressAndPassesTheAudit() throws {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--show-panel", "--ui-testing-telechargement"]
        app.launch()
        XCTAssertTrue(app.groups["lecteur"].waitForExistence(timeout: 5))
        let line = app.staticTexts["etat-voix"]
        XCTAssertTrue(line.waitForExistence(timeout: 2))
        XCTAssertTrue((line.value as? String ?? line.label).hasPrefix("Installation de la voix"))
        XCTAssertFalse(app.buttons["reessayer-voix"].exists)
        try app.performAccessibilityAuditOutsideTouchBar()
    }
}
```

**`UIBUILD`** : réussit.

- [ ] **Step 6 : commit**

```bash
git add macos/Sources/App/VoiceStatus.swift macos/Sources/Model/ReaderModel.swift macos/Sources/Model/ReaderModel+Engine.swift \
        macos/Sources/AppDelegate.swift macos/Sources/Panel/PanelContent.swift macos/Sources/Panel/ReaderView.swift \
        macos/Sources/Settings/ReglagesView.swift macos/Sources/Model/UITestStaging.swift \
        macos/Tests/VoiceStatusTests.swift macos/Tests/PanelControllerTests.swift macos/UITests/VoiceStatusUITests.swift
git commit -m "$(cat <<'EOF'
feat(macos): la voix s'installe seule au lancement (ensureDefaultVoice après le passage à Kokoro) ; une ligne d'état dans le lecteur et en bas des réglages, seulement en cas de problème — installation en cours avec sa progression, échec avec Réessayer, repli, voix indisponible, première préparation

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Tâche 11 : le pont FFI sans signets ni commandes inutilisées

Ce que l'app n'utilise plus et que le terminal n'utilise pas (il n'utilise jamais le pont) sort de `ffi/src/lib.rs`. Le module `src/bookmark.rs` et `Command::ToggleBookmark` du cœur **restent** (le plan 3 les retire).

Vérification faite à la rédaction du plan, à refaire à l'étape 1 : appelants Swift de chaque variante à retirer.

| Variante FFI | Appelant Swift après les tâches 1 à 10 | Décision |
|---|---|---|
| `ToggleBookmark` | aucun (`b` retiré, Tâche 5) | retirée |
| `Blocks` | aucun (⇧↑↓ retirés, Tâche 5) | retirée |
| `ChangeVolume` | aucun (`[ ]`, ⌥↑↓ retirés, Tâche 5) | retirée |
| `ToggleMute` | aucun (`m` retiré, Tâche 5) | retirée |
| `RestartSentence` | aucun (`r` retiré, Tâche 5) | retirée |
| `Start` | aucun (`g`, Début retirés, Tâche 5) | retirée |
| `End` | `Tests/ReaderModelTests.swift:70` (`m.send(.end)`) | retirée ; le test passe à `.sections(delta: 1)` |
| `Words` | **`Sources/App/Measure.swift:75`** (mesure `saut_mot`, `--measure`) et `Tests/AnnouncerTests.swift:33` | **gardée** : l'outil de mesure s'en sert |

**Files:**
- Modify : `ffi/src/lib.rs` (enum `ReaderCommand` et son `From`, `Snapshot.bookmarked`, `snapshot_of`, `BookmarkView`, `StoredBookmarkView`, `bookmarks()`, `all_bookmarks()`, `remove_bookmark()`, `stored_bookmark_of`, l'import `data`, test `the_silent_reader_writes_and_erases_nothing`), `macos/Tests/ReaderModelTests.swift:70`
- Régénéré (non suivi) : `macos/Generated/sillon_ffi.swift`, `macos/Libraries/libsillon_ffi.a`

**Interfaces:**
- Produces : `ReaderCommand` FFI = `TogglePlay, Pause, Words, Sentences, Sections, SeekTo, SeekToWord, SetSpeed, ChangeSpeed, Search, SearchAdvance, SearchCancel` (+ ce que le plan 1 a ajouté) ; plus de `Snapshot.bookmarked`, `BookmarkView`, `StoredBookmarkView`, `bookmarks()`, `allBookmarks()`, `removeBookmark()` côté Swift.

- [ ] **Step 1 : vérifier les appelants, puis le contrôle qui doit échouer.**

```bash
grep -rnE "\.(toggleBookmark|blocks|changeVolume|toggleMute|restartSentence|start|end)\b|bookmarked|BookmarkView|allBookmarks|removeBookmark|\.bookmarks\(\)" macos/Sources macos/Tests macos/UITests \
  | grep -v "frames.start\|schedule.start\|KeepVoice"
```

Attendu : seulement `macos/Tests/ReaderModelTests.swift:70: m.send(.end)`. Toute autre ligne : s'arrêter et la signaler.

Contrôle de la liaison (il échoue tant que rien n'est retiré) :

```bash
! grep -nE "case toggleBookmark|case blocks|case changeVolume|case toggleMute|case restartSentence|case start$|case end$|bookmarked|BookmarkView|func allBookmarks|func removeBookmark|func bookmarks" macos/Generated/sillon_ffi.swift
```

Attendu maintenant : code de sortie 1 (des lignes sont trouvées).

- [ ] **Step 2 : retirer côté Rust** — dans `ffi/src/lib.rs` :

L'enum devient :

```rust
pub enum ReaderCommand {
    TogglePlay,
    Pause,
    /// Gardée pour l'outil de mesure de l'app (`Measure.swift`, saut de mots).
    Words { delta: i32 },
    Sentences { delta: i32 },
    Sections { delta: i32 },
    SeekTo { sentence: u32 },
    SeekToWord { sentence: u32, word: u32 },
    SetSpeed { speed: f32 },
    ChangeSpeed { delta: f32 },
    Search { query: String },
    SearchAdvance { delta: i32 },
    SearchCancel,
}
```

(si le plan 1 y a ajouté une variante, la garder) ; dans `impl From<ReaderCommand> for Command`, supprimer les bras `Blocks`, `RestartSentence`, `Start`, `End`, `ChangeVolume`, `ToggleMute`, `ToggleBookmark`. Supprimer le champ `pub bookmarked: bool` de `Snapshot` et la ligne `bookmarked: s.bookmarked,` de `snapshot_of` ; supprimer `pub struct BookmarkView { … }`, `pub struct StoredBookmarkView { … }`, `pub fn bookmarks(&self)`, `pub fn all_bookmarks(&self)`, `pub fn remove_bookmark(&self, …)`, `fn stored_bookmark_of(…)`. Retirer `data` de `use sillon::reader::{data, preview, settings, view, Reader};` s'il n'a plus d'usage (`grep -n "data::" ffi/src/lib.rs` : vide). Dans le test `the_silent_reader_writes_and_erases_nothing`, supprimer la ligne `assert!(reader.remove_bookmark("x".into(), 0).is_err());`.

Ajouter dans `mod tests` :

```rust
    /// Les commandes que l'app envoie encore traversent le pont telles quelles.
    #[test]
    fn the_commands_left_cross_the_bridge() {
        assert_eq!(Command::from(ReaderCommand::Sentences { delta: -1 }), Command::Sentences(-1));
        assert_eq!(Command::from(ReaderCommand::Words { delta: 3 }), Command::Words(3));
        assert_eq!(Command::from(ReaderCommand::SeekTo { sentence: 4 }), Command::SeekTo(4));
        assert_eq!(Command::from(ReaderCommand::SetSpeed { speed: 1.25 }), Command::SetSpeed(1.25));
    }
```

- [ ] **Step 3 : vérifier Rust** — `cargo test --workspace --locked` : PASS ; `cargo clippy --workspace --all-targets --locked -- -D warnings` : aucun avertissement (en particulier aucun « unused import » pour `data`).

- [ ] **Step 4 : régénérer et adapter Swift** — **`FFI`** (`./scripts/build-ffi.sh`), puis le contrôle de l'étape 1 :

```bash
! grep -nE "case toggleBookmark|case blocks|case changeVolume|case toggleMute|case restartSentence|case start$|case end$|bookmarked|BookmarkView|func allBookmarks|func removeBookmark|func bookmarks" macos/Generated/sillon_ffi.swift
```

Attendu : code de sortie 0 (rien trouvé).

`macos/Tests/ReaderModelTests.swift`, ligne 70 : `m.send(.end)` → `m.send(.sections(delta: 1))` (le document du test a deux titres : on passe au second, le fil d'Ariane attendu reste « Monades ▸ Exemples »).

**`TEST`** (suite entière) : PASS. **`UIBUILD`** : réussit.

- [ ] **Step 5 : commit**

```bash
git add ffi/src/lib.rs macos/Tests/ReaderModelTests.swift
git commit -m "$(cat <<'EOF'
refactor(ffi): le pont perd les signets (Snapshot.bookmarked, BookmarkView, StoredBookmarkView, bookmarks, all_bookmarks, remove_bookmark) et les commandes que l'app n'envoie plus (ToggleBookmark, Blocks, ChangeVolume, ToggleMute, RestartSentence, Start, End)

Words reste : l'outil de mesure de l'app s'en sert. Le module bookmark du cœur et Command::ToggleBookmark restent pour le terminal (plan 3).

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Tâche 12 : vérification de bout en bout

Pas de code nouveau : la preuve que le lot construit, passe ses tests et fait ce que dit la spec.

**Files:** aucun (sauf correction d'un défaut trouvé ici, dans sa propre tâche de correction avec son test).

- [ ] **Step 1 : tout reconstruire et tout tester.**

```bash
./scripts/build-ffi.sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
(cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' -derivedDataPath build/derived test -only-testing:SillonTests)
(cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' -derivedDataPath build/derived build-for-testing)
```

Attendu : tout passe ; le journal de `xcodebuild` ne contient **aucun** nouvel avertissement de concurrence (`grep -c "warning:"` à comparer avec la construction de la branche avant la Tâche 1).

- [ ] **Step 2 : ce qui ne doit plus exister.**

```bash
grep -rnE "Notes|NotesStore|setActivationPolicy|SettingsTab|ModelsTab|DisplayDefaults|playPause|nextSentence|previousSentence|onDoubleClick|bookmark" macos/Sources
```

Attendu : aucune ligne (une ligne de commentaire qui parle des signets du terminal se corrige).

- [ ] **Step 3 : les tests d'interface — seulement avec le feu vert explicite de l'utilisateur.** Lui demander de quitter le sillon installé (il tiendrait ⌃⌥S et aurait aussi une icône dans la barre des menus), puis, avec son accord :

```bash
(cd macos && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' -derivedDataPath build/derived test -only-testing:SillonUITests)
```

Attendu : tout passe (`SettingsUITests.testThePageIsKeyboardReachable` peut être sauté sans « Navigation au clavier »).

- [ ] **Step 4 : vérification manuelle, les quatre chemins de lecture** (avec l'utilisateur, sur l'app construite par `./scripts/build-macos.sh`) :
  1. Sélectionner un paragraphe dans Safari, ⌃⌥Espace : le lecteur s'ouvre dans sa dernière taille et lit.
  2. Copier un texte, ⌘V dans le lecteur : il le lit. Copier une image de texte, ⌘V : l'OCR la lit.
  3. Déposer un PDF sur le lecteur, puis ⌘O et un `.md` : chacun s'ouvre et se lit.
  4. Au terminal, `sillon` (commande de l'app) sans argument : le lecteur vide s'ouvre en grand ; `sillon cours.md` : il le lit.
  Puis : ⌘M aller-retour (cadre animé, lecture continue) ; ⌘W (pause, rien à l'écran) ; ⌃⌥S (réouverture dans la dernière taille, sur la même phrase) ; sélection + ⌘C pendant la lecture (la sélection reste, le presse-papiers la contient) ; clic droit « Lire à partir d'ici » ; bouton Modifier, Échap (la saisie est gardée) ; ⌘, (une page) ; changer la police : le lecteur suit, `config.toml` contient `font` (section `[reading]`) ; Dock : aucune icône à aucun moment.

- [ ] **Step 5 : mesurer ce qui tourne au repos** — lecteur ouvert en pause, puis fenêtre fermée, 20 s chacun : `top -l 20 -s 1 -pid $(pgrep -x Sillon) -stats cpu` ; attendu ~0 % (aucune minuterie nouvelle : la ligne d'état et la timeline cliquable n'ajoutent rien à la cadence existante). Un chiffre au-dessus de 1 % en moyenne s'explique avant de conclure.

- [ ] **Step 6 : rendre compte** à l'utilisateur : ce qui passe, ce qui a été sauté et pourquoi, les écarts trouvés à l'étape 4.

---

## Couverture de la spec (section 1, et section 4 côté macOS)

| Spec | Tâche |
|---|---|
| 1.1 ⌃⌥Espace inchangé ; ⌃⌥S affiche / masque ; ouverture manuelle → lecteur vide avec l'invitation ; ⌘V ; glisser-déposer et ⌘O ; rien à la connexion | 7 (⌃⌥S, dépôt), 2 (dernière taille), 1 et 3 (lancement manuel, invitation), 5 (⌘V, ⌘O) |
| 1.2 Menu de cinq lignes (+ autorisation d'accessibilité) | 7 |
| 1.3 `DisplayMode` mini / reader / hidden ; en-tête, barre, ligne d'état ; mini par phrase ; ⌘M animé ; fermer = pause, jamais le mini ; Échap ferme d'abord recherche ou sommaire ; dernière taille ; lecteur vide toujours en grand | 2, 3, 4, 10 |
| 1.4 Sélection, ⌘C, menu contextuel, sélection gardée par le suivi ; édition par Modifier ou `e`, fond teinté, Annuler / Terminé, Échap et ⌘↩ valident | 6 (Annuler / Terminé / Échap / ⌘↩ existent déjà dans `TextEditingView` et `PanelKeyRouter`) |
| 1.5 Clavier et aide complète | 5 |
| 1.6 Réglages sur une page ; police et taille dans `config.toml` avec reprise | 9, 8 |
| 1.7 Installation automatique, progression, « Réessayer » | 10 |
| 1.8 Retraits (Notes, onglets, raccourcis globaux, `LSUIElement` sans exception) | 1, 9, 7 |
| 3 FFI : signets et commandes sans appelant | 11 |
| 4 Tests d'interface listés | 3 (lancement manuel), 5 (⌘V), 6 (⌘C, Modifier, double clic), 2 (⌘M, ⌘W, réouverture), 7 (⌃⌥S), 9 (une page) ; exécution : 12 |

## Interprétations retenues (à valider par l'utilisateur à la relecture du plan)

- **⌃⌥S qui masque** se comporte comme « Fermer » : la lecture se met en pause (la spec range ⌃⌥S à part ; écouter sans fenêtre reste possible par les touches média, et le mini sert à écouter sans suivre).
- **Réouverture après fermeture** : le lecteur revient sur la même phrase, **en pause** ; Espace relance. (« la réouverture reprend là » est lu comme « au même endroit ».)
- **⌘,** reste une touche du lecteur et figure dans l'aide, bien qu'absent de la table 1.5 (raccourci standard de macOS, non listé dans les retraits).
- **Mini sans bouton fermer** (la spec ne liste que ⏮ ▶ ⏭, progression, agrandir) : il se ferme par ⌘W ou Échap, ou en agrandissant puis fermant.
- **Ligne d'état** : en plus des problèmes de voix, les messages passagers d'une action de l'utilisateur (« Phrase copiée », « Impossible d'ouvrir … », « Sortie audio débranchée ») restent affichés quelques secondes, sous la voix ; sans eux, ⌘V sur un presse-papiers vide ou un fichier illisible ne dirait rien.
- **Hauteur du lecteur** : l'ancien panneau moyen suivait la hauteur du texte ; le lecteur garde la taille que l'utilisateur lui donne (spec : « redimensionnable, taille et position retenues »).
- **`ReaderCommand::Words` reste** dans le pont : `Measure.swift` (outil de mesure `--measure`, saut de mots) l'envoie.
- **« Lire la sélection » depuis le menu** passe par la même chaîne de capture que ⌃⌥Espace ; si le clic dans le menu fait perdre la sélection à l'application au premier plan, elle retombe sur le presse-papiers (comme aujourd'hui). À observer à la Tâche 12.
