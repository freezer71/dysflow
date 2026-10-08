# Quatre modes d'affichage dans l'app macOS — plan d'implémentation

> **Pour les agents :** sous-skill requis : superpowers:subagent-driven-development
> (recommandé) ou superpowers:executing-plans, tâche par tâche. Les étapes
> utilisent des cases à cocher (`- [ ]`).

**Objectif :** `Sillon.app` se montre en quatre modes — petit (barre de
360 × 44 pt), moyen (le panneau actuel), grand (le lecteur agrandi actuel),
arrière-plan (aucune fenêtre, la lecture continue) — et se pilote sans
fenêtre : touches média, Centre de contrôle (« À l'écoute »), menu de la barre
des menus et raccourcis globaux.

**Architecture :** un type pur `DisplayState` porte le mode courant, le
dernier mode visible et toutes les transitions (⌘1/2/3, Échap, ⌃⌥S,
⌃⌥Espace, fermeture) ; `DisplayModeStore` les retient dans les préférences
(suite isolée sous les tests) avec la place du mode petit, ramenée sur un
écran visible par `SmallPlacement`. `PanelController` applique le mode à
l'unique `ReadingPanel` (taille, contenu, identifiant, masquage sans pause).
`MiniPlayerView` est la barre du mode petit ; une copie cachée de la vue de
lecture garde ↑ ↓ actifs. `Transport` traduit les demandes extérieures en
commandes du lecteur ; `NowPlaying` publie l'état à « À l'écoute » au
changement d'état seulement (crochet `ReaderModel.onChange`) et branche les
commandes média, derrière des protocoles que les tests remplacent par des faux.

**Pile :** Swift 6.x en mode de langage 5, SwiftUI + AppKit, MediaPlayer,
macOS 14+, XcodeGen, KeyboardShortcuts 2.x, XCTest / XCUITest. Aucun
changement Rust.

**Spec :** `docs/superpowers/specs/2026-10-08-modes-affichage-design.md`
(elle prolonge `2026-10-05-interface-macos-design.md` et
`2026-10-06-reglages-complets-design.md`). La lire avant toute tâche.

## Contraintes globales

- macOS 14 minimum, Apple Silicon (`arm64`) seul.
- Swift en **mode de langage 5** (`SWIFT_VERSION: "5.0"` dans `macos/project.yml`).
- Texte d'interface en police système **13 pt, fixe** ; textes, commentaires
  et noms d'interface en **français**, comme le code existant.
- Chaque contrôle est atteignable au clavier et porte un **libellé VoiceOver**
  (et une valeur quand il en a une) : la timeline du mode petit annonce
  « 0:56 sur 2:56 ».
- **Coût au repos nul** : aucune minuterie, aucun lien d'affichage tant que
  rien ne joue ; le mode petit suit la cadence existante de `ReaderModel`
  (lecture et courte grâce seulement). « À l'écoute » est mis à jour **au
  changement d'état** (lecture, pause, phrase, vitesse, durée, document),
  **jamais à chaque image ni à chaque seconde**. Seule exception, unique et
  non répétée : une relecture différée après une pause, au moment où le
  moteur met la voix au repos (Tâche 5).
- Les tests ne touchent jamais aux données de l'utilisateur
  (`~/.config/sillon`, `~/Library/Application Support/sillon`, préférences
  réelles) : lecteur muet (`SillonReader.silent()`), suites `UserDefaults`
  isolées et vidées, faux centre « À l'écoute » et fausses commandes média.
  Sous les tests (unitaires ou d'interface), `NowPlaying` n'est pas créé.
- **Tests d'interface** : écrits et compilés (`build-for-testing`), **jamais
  exécutés par l'implémenteur** ; leur exécution est une étape à part,
  soumise au feu vert explicite de l'utilisateur.
- Ne jamais journaliser une touche ni un texte lu.
- Ne jamais indexer `vendor/audio.cpp`, `macos/Generated`, `macos/Libraries`,
  `macos/*.xcodeproj`, `macos/build` : `git add` de chemins explicites
  uniquement.
- **Pas de changement Rust** : `togglePlay`, `pause`, `sentences(delta:)`,
  `seekTo(sentence:)`, `seekToWord` existent. Si une tâche semble en exiger
  un, s'arrêter et le signaler.
- Tests unitaires :
  `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests`
  (filtrer avec `-only-testing:SillonTests/<Classe>` pendant une tâche, puis
  la suite entière avant le commit).
- Compilation des tests d'interface : même commande avec `build-for-testing`
  à la place de `test -only-testing:SillonTests`.
- Messages de commit en français, style du dépôt (`feat(macos): …`),
  terminés par `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Points d'attention

Cinq situations que la spec implique sans les tester, les plus susceptibles
de mordre un utilisateur ; chacune a reçu son test dans la tâche indiquée.

1. **Mode petit retenu sur un écran débranché** : la place enregistrée ne
   tombe plus sur aucun écran ; attendu : la barre revient sur l'écran
   principal, entière. → Tâche 1, `testAPlaceOnAnUnpluggedScreenComesBackToTheMainScreen`
   et `testABarHalfOffTheEdgeIsPulledBackOnScreen` ; Tâche 3,
   `testASmallBarLeftOnAnUnpluggedScreenComesBackToTheMainScreen`.
2. **Touche média ou raccourci global sans document ouvert** : attendu : rien
   ne part au lecteur, « À l'écoute » n'affiche rien. → Tâche 4,
   `testNothingIsSentWithoutADocument` ; Tâche 5, `testNothingIsShownWithoutADocument`
   et `testMediaCommandsDriveTheReader`.
3. **Timeline glissée au-delà de la fin, avant le début, ou durée encore
   inconnue** : attendu : dernière phrase, première phrase, rien. → Tâche 4,
   `testBeyondTheEndIsTheLastSentenceAndBeforeTheStartTheFirst`,
   `testWithoutAnEstimateTheTimelineDoesNothing`.
4. **Changer de mode pendant que la fenêtre des réglages est ouverte** : le
   panneau passe sous les réglages (niveau `.normal`) ; un changement de mode
   ne doit pas le remettre au-dessus. → Tâche 3,
   `testModesKeepThePanelUnderTheOpenSettings`.
5. **« À l'écoute » republié à chaque seconde pendant la lecture** (le temps
   publié change chaque seconde, l'avancement au millième) : attendu : une
   seule publication tant que seul le temps avance. → Tâche 5,
   `testTimePassingAloneRepublishesNothing`.

Couverts aussi : ⌃⌥S quand le dernier mode était l'arrière-plan (le dernier
mode visible n'est jamais l'arrière-plan, Tâche 1) ; relance en arrière-plan
(rien ne s'affiche, Tâche 3 ; le menu garde « Afficher sillon » et
« Afficher », Tâche 6).

## Structure des fichiers

| Fichier | Rôle |
|---|---|
| `macos/Sources/Panel/DisplayMode.swift` (nouveau) | `DisplayMode`, `DisplayEvent`, `DisplayState` : modes et transitions, sans état |
| `macos/Sources/Panel/DisplayModeStore.swift` (nouveau) | `DisplayModeStore` (préférences, suite isolée sous test), `SmallPlacement` (taille et place du mode petit) |
| `macos/Sources/Panel/PlaybackTime.swift` (nouveau) | temps restant, valeur VoiceOver de la timeline, ligne du menu, « Rien à lire » |
| `macos/Sources/Panel/MiniPlayerView.swift` (nouveau) | la barre du mode petit |
| `macos/Sources/Panel/PlayerBar.swift` | `PlayButton`, `VoiceSettingsButton` extraits et partagés |
| `macos/Sources/Panel/PanelContent.swift` | contenu par mode, copie cachée de la vue de lecture, `PanelInvitation.keys`, lignes d'aide |
| `macos/Sources/Panel/ReadingPanel.swift` | `PanelMode` retiré, `PanelGeometry.horizontalPadding` |
| `macos/Sources/Panel/PanelController.swift` | applique les modes, retient la place du mode petit |
| `macos/Sources/Reading/ReadingView.swift` | paramètre `shadow` (copie invisible) |
| `macos/Sources/Input/KeyMap.swift` | ⌘1, ⌘2, ⌘3 → `.display(_)` |
| `macos/Sources/Model/ReaderModel.swift` | crochet `onChange`, `restVoice()` relit l'état |
| `macos/Sources/App/Transport.swift` (nouveau) | `RemoteAction`, `Transport` (commandes et saut de timeline) |
| `macos/Sources/App/NowPlaying.swift` (nouveau) | `NowPlayingInfo`, `NowPlaying` |
| `macos/Sources/App/NowPlayingSystem.swift` (nouveau) | protocoles `NowPlayingCenter`, `RemoteCommandSource` et leur version système |
| `macos/Sources/App/Shortcuts.swift` | `playPause`, `nextSentence`, `previousSentence` |
| `macos/Sources/AppDelegate.swift` | restauration du mode, `Transport`, `NowPlaying`, raccourcis globaux |
| `macos/Sources/MenuContent.swift` | lecture/pause, phrases, temps restant, sous-menu « Afficher » |
| `macos/Sources/Settings/ShortcutsTab.swift` | trois lignes de plus |
| `macos/project.yml` | lien `MediaPlayer.framework` |
| `macos/Tests/DisplayModeTests.swift`, `DisplayModeStoreTests.swift`, `MiniPlayerTests.swift`, `TransportTests.swift`, `NowPlayingTests.swift`, `HelpOverlayTests.swift` (nouveaux) | tests unitaires |
| `macos/Tests/PanelControllerTests.swift`, `KeyMapTests.swift`, `ReaderModelTests.swift`, `Samples.swift` | tests modifiés ou complétés |
| `macos/UITests/DisplayModeUITests.swift` (nouveau) | ⌘1/2/3, Échap, audit du mode petit |

---

### Tâche 1 : les modes, leurs transitions et ce qui en est retenu

Un type pur pour les quatre modes et leurs transitions, un magasin qui les
retient avec la place du mode petit, et le calcul de cette place (ramenée sur
un écran visible).

**Fichiers :**
- Créer : `macos/Sources/Panel/DisplayMode.swift`
- Créer : `macos/Sources/Panel/DisplayModeStore.swift`
- Créer : `macos/Tests/DisplayModeTests.swift`
- Créer : `macos/Tests/DisplayModeStoreTests.swift`

**Interfaces :**
- Consomme : rien.
- Produit :
  - `enum DisplayMode: String, Equatable, CaseIterable { case small, medium, large, background; var isVisible: Bool }`
  - `enum DisplayEvent: Equatable { case choose(DisplayMode), escape, hide, showLast, readSelection }`
  - `struct DisplayState: Equatable { private(set) var current: DisplayMode; private(set) var lastVisible: DisplayMode; init(current:lastVisible:); static let initial; func applying(_ event: DisplayEvent) -> DisplayState }`
  - `final class DisplayModeStore { init(defaults: UserDefaults); var state: DisplayState { get set }; var smallOrigin: CGPoint? { get set }; static func processDefaults() -> UserDefaults; static let currentKey, lastVisibleKey, smallOriginKey: String }`
  - `enum SmallPlacement { static let size: CGSize /* 360 × 44 */; static func frame(savedOrigin: CGPoint?, screens: [CGRect], main: CGRect) -> CGRect; static func defaultFrame(on: CGRect) -> CGRect }`

- [ ] **Étape 1 : écrire les tests qui échouent**

`macos/Tests/DisplayModeTests.swift` :

```swift
import XCTest
@testable import Sillon

final class DisplayModeTests: XCTestCase {
    private let start = DisplayState.initial

    func testAtFirstNothingIsShownAndMediumComesBack() {
        XCTAssertEqual(start.current, .background)
        XCTAssertEqual(start.lastVisible, .medium)
    }

    func testChoosingAVisibleModeRemembersIt() {
        let s = start.applying(.choose(.small))
        XCTAssertEqual(s.current, .small)
        XCTAssertEqual(s.lastVisible, .small)
    }

    /// Échap dans le grand mode : d'abord le moyen, puis l'arrière-plan.
    func testEscapeIsTwoStepsFromLarge() {
        let large = start.applying(.choose(.large))
        let medium = large.applying(.escape)
        XCTAssertEqual(medium.current, .medium)
        XCTAssertEqual(medium.lastVisible, .medium)
        let hidden = medium.applying(.escape)
        XCTAssertEqual(hidden.current, .background)
        XCTAssertEqual(hidden.lastVisible, .medium)
    }

    func testEscapeFromSmallGoesToTheBackground() {
        let s = start.applying(.choose(.small)).applying(.escape)
        XCTAssertEqual(s.current, .background)
        XCTAssertEqual(s.lastVisible, .small)
    }

    func testEscapeInTheBackgroundStaysThere() {
        XCTAssertEqual(start.applying(.escape), start)
    }

    /// ⌘W, fermeture : l'arrière-plan d'un coup, même depuis le grand mode.
    func testHideGoesStraightToTheBackground() {
        let s = start.applying(.choose(.large)).applying(.hide)
        XCTAssertEqual(s.current, .background)
        XCTAssertEqual(s.lastVisible, .large)
    }

    /// ⌃⌥S : le dernier mode visible.
    func testShowLastBringsBackTheLastVisibleMode() {
        let s = start.applying(.choose(.small)).applying(.escape).applying(.showLast)
        XCTAssertEqual(s.current, .small)
    }

    /// Le dernier mode visible n'est jamais l'arrière-plan : ⌃⌥S montre
    /// toujours quelque chose.
    func testLastVisibleIsNeverTheBackground() {
        let s = DisplayState(current: .background, lastVisible: .background)
        XCTAssertEqual(s.lastVisible, .medium)
        XCTAssertEqual(s.applying(.showLast).current, .medium)
        XCTAssertEqual(start.applying(.choose(.background)).lastVisible, .medium)
    }

    /// ⌃⌥Espace garde le mode visible ; depuis l'arrière-plan, le moyen.
    func testReadSelectionKeepsAVisibleModeAndOpensMediumFromTheBackground() {
        let small = start.applying(.choose(.small))
        XCTAssertEqual(small.applying(.readSelection), small)
        let opened = small.applying(.escape).applying(.readSelection)
        XCTAssertEqual(opened.current, .medium)
        XCTAssertEqual(opened.lastVisible, .medium)
    }

    func testOnlyTheBackgroundIsInvisible() {
        XCTAssertEqual(DisplayMode.allCases.filter { !$0.isVisible }, [.background])
    }
}
```

`macos/Tests/DisplayModeStoreTests.swift` :

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

    func testAFreshStoreStartsInTheBackground() {
        XCTAssertEqual(DisplayModeStore(defaults: freshDefaults()).state, .initial)
        XCTAssertNil(DisplayModeStore(defaults: freshDefaults()).smallOrigin)
    }

    func testTheModeSurvivesARelaunch() {
        let defaults = freshDefaults()
        let state = DisplayState.initial.applying(.choose(.small)).applying(.escape)
        DisplayModeStore(defaults: defaults).state = state
        XCTAssertEqual(DisplayModeStore(defaults: defaults).state, state)
        XCTAssertEqual(DisplayModeStore(defaults: defaults).state.lastVisible, .small)
    }

    func testUnreadableValuesFallBackToTheDefaults() {
        let defaults = freshDefaults()
        defaults.set("géant", forKey: DisplayModeStore.currentKey)
        defaults.set("background", forKey: DisplayModeStore.lastVisibleKey)
        XCTAssertEqual(DisplayModeStore(defaults: defaults).state, .initial)
    }

    func testTheSmallBarPlaceSurvivesARelaunch() {
        let defaults = freshDefaults()
        DisplayModeStore(defaults: defaults).smallOrigin = CGPoint(x: 120, y: 640)
        XCTAssertEqual(DisplayModeStore(defaults: defaults).smallOrigin, CGPoint(x: 120, y: 640))
        defaults.set(["a"], forKey: DisplayModeStore.smallOriginKey)
        XCTAssertNil(DisplayModeStore(defaults: defaults).smallOrigin, "valeur illisible : pas de place")
        let store = DisplayModeStore(defaults: defaults)
        store.smallOrigin = CGPoint(x: 1, y: 2)
        store.smallOrigin = nil
        XCTAssertNil(DisplayModeStore(defaults: defaults).smallOrigin)
    }

    /// Sous les tests, le mode ne va jamais dans les préférences réelles.
    func testUnderTestsTheModeIsNotStoredInTheUserDefaults() {
        let defaults = DisplayModeStore.processDefaults()
        XCTAssertFalse(defaults === UserDefaults.standard)
        let before = UserDefaults.standard.string(forKey: DisplayModeStore.currentKey)
        DisplayModeStore(defaults: defaults).state = DisplayState(current: .large, lastVisible: .large)
        XCTAssertEqual(UserDefaults.standard.string(forKey: DisplayModeStore.currentKey), before)
    }
}

final class SmallPlacementTests: XCTestCase {
    private let main = CGRect(x: 0, y: 0, width: 1440, height: 875)
    private let side = CGRect(x: 1440, y: 0, width: 1920, height: 1055)

    func testWithoutASavedPlaceTheBarSitsOnTheMainScreen() {
        let frame = SmallPlacement.frame(savedOrigin: nil, screens: [main, side], main: main)
        XCTAssertEqual(frame, SmallPlacement.defaultFrame(on: main))
        XCTAssertEqual(frame.size, SmallPlacement.size)
        XCTAssertTrue(main.contains(frame))
    }

    func testASavedPlaceOnAScreenIsKept() {
        let frame = SmallPlacement.frame(savedOrigin: CGPoint(x: 1600, y: 500), screens: [main, side], main: main)
        XCTAssertEqual(frame.origin, CGPoint(x: 1600, y: 500))
    }

    func testAPlaceOnAnUnpluggedScreenComesBackToTheMainScreen() {
        let frame = SmallPlacement.frame(savedOrigin: CGPoint(x: 1600, y: 500), screens: [main], main: main)
        XCTAssertEqual(frame, SmallPlacement.defaultFrame(on: main))
    }

    func testABarHalfOffTheEdgeIsPulledBackOnScreen() {
        let frame = SmallPlacement.frame(savedOrigin: CGPoint(x: 1400, y: 100), screens: [main], main: main)
        XCTAssertEqual(frame.origin, CGPoint(x: 1440 - 360, y: 100))
        XCTAssertTrue(main.contains(frame))
        // Deux écrans : celui qui en montre le plus la reçoit.
        let shared = SmallPlacement.frame(savedOrigin: CGPoint(x: 1400, y: 100), screens: [main, side], main: main)
        XCTAssertEqual(shared.origin, CGPoint(x: 1440, y: 100))
    }

    func testWithoutAnyScreenTheMainFrameIsUsed() {
        XCTAssertEqual(SmallPlacement.frame(savedOrigin: CGPoint(x: 5, y: 5), screens: [], main: main),
                       SmallPlacement.defaultFrame(on: main))
    }
}
```

- [ ] **Étape 2 : vérifier l'échec**

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests/DisplayModeTests -only-testing:SillonTests/DisplayModeStoreTests -only-testing:SillonTests/SmallPlacementTests`
Expected: FAIL — `cannot find 'DisplayState' in scope`, `cannot find 'DisplayModeStore' in scope`, `cannot find 'SmallPlacement' in scope`.

- [ ] **Étape 3 : écrire l'implémentation**

`macos/Sources/Panel/DisplayMode.swift` :

```swift
/// Les quatre façons de montrer sillon. Une seule fenêtre : chaque mode
/// change son contenu et sa taille ; l'arrière-plan la retire, la lecture
/// continue.
enum DisplayMode: String, Equatable, CaseIterable {
    case small, medium, large, background

    var isVisible: Bool { self != .background }
}

/// Ce qui fait changer de mode.
enum DisplayEvent: Equatable {
    /// ⌘1, ⌘2, ⌘3, ⏎, le sous-menu « Afficher ».
    case choose(DisplayMode)
    /// Échap : du grand au moyen, sinon l'arrière-plan.
    case escape
    /// ⌘W, fermeture de la fenêtre : l'arrière-plan d'un coup.
    case hide
    /// ⌃⌥S, « Afficher sillon » : le dernier mode visible.
    case showLast
    /// ⌃⌥Espace : le mode en cours ; depuis l'arrière-plan, le moyen.
    case readSelection
}

/// Le mode courant et le dernier mode visible — jamais l'arrière-plan, pour
/// que ⌃⌥S montre toujours quelque chose. Sans état caché : testé seul.
struct DisplayState: Equatable {
    private(set) var current: DisplayMode
    private(set) var lastVisible: DisplayMode

    static let initial = DisplayState(current: .background, lastVisible: .medium)

    init(current: DisplayMode, lastVisible: DisplayMode) {
        self.current = current
        if current.isVisible {
            self.lastVisible = current
        } else {
            // Un dernier mode « arrière-plan » (préférence abîmée) : le moyen.
            self.lastVisible = lastVisible.isVisible ? lastVisible : .medium
        }
    }

    func applying(_ event: DisplayEvent) -> DisplayState {
        switch event {
        case .choose(let mode):
            return DisplayState(current: mode, lastVisible: lastVisible)
        case .escape:
            let next: DisplayMode = current == .large ? .medium : .background
            return DisplayState(current: next, lastVisible: lastVisible)
        case .hide:
            return DisplayState(current: .background, lastVisible: lastVisible)
        case .showLast:
            return DisplayState(current: lastVisible, lastVisible: lastVisible)
        case .readSelection:
            return current.isVisible ? self : DisplayState(current: .medium, lastVisible: lastVisible)
        }
    }
}
```

`macos/Sources/Panel/DisplayModeStore.swift` :

```swift
import Foundation

/// Retient le mode courant, le dernier mode visible et la place du mode
/// petit, d'un lancement à l'autre.
final class DisplayModeStore {
    static let currentKey = "affichage.mode"
    static let lastVisibleKey = "affichage.dernierVisible"
    static let smallOriginKey = "affichage.petit.origine"

    private let defaults: UserDefaults

    init(defaults: UserDefaults) {
        self.defaults = defaults
    }

    /// Une valeur illisible retombe sur l'état initial (arrière-plan, moyen).
    var state: DisplayState {
        get {
            let current = defaults.string(forKey: Self.currentKey).flatMap(DisplayMode.init(rawValue:))
                ?? DisplayState.initial.current
            let last = defaults.string(forKey: Self.lastVisibleKey).flatMap(DisplayMode.init(rawValue:))
                ?? DisplayState.initial.lastVisible
            return DisplayState(current: current, lastVisible: last)
        }
        set {
            defaults.set(newValue.current.rawValue, forKey: Self.currentKey)
            defaults.set(newValue.lastVisible.rawValue, forKey: Self.lastVisibleKey)
        }
    }

    /// Le coin bas gauche de la barre du mode petit, là où l'utilisateur l'a
    /// laissée ; nil tant qu'il ne l'a pas déplacée.
    var smallOrigin: CGPoint? {
        get {
            guard let xy = defaults.array(forKey: Self.smallOriginKey) as? [Double], xy.count == 2,
                  xy.allSatisfy(\.isFinite) else { return nil }
            return CGPoint(x: xy[0], y: xy[1])
        }
        set {
            if let p = newValue {
                defaults.set([Double(p.x), Double(p.y)], forKey: Self.smallOriginKey)
            } else {
                defaults.removeObject(forKey: Self.smallOriginKey)
            }
        }
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

/// La barre du mode petit : taille fixe, place retenue, toujours sur un
/// écran visible.
enum SmallPlacement {
    static let size = CGSize(width: 360, height: 44)

    /// La place retenue si un écran en montre au moins un bout (ramenée
    /// entière sur l'écran qui en montre le plus) ; sinon — écran débranché,
    /// rien de retenu — la place par défaut sur l'écran principal.
    static func frame(savedOrigin: CGPoint?, screens: [CGRect], main: CGRect) -> CGRect {
        if let origin = savedOrigin {
            let wanted = CGRect(origin: origin, size: size)
            let best = screens.max { area($0.intersection(wanted)) < area($1.intersection(wanted)) }
            if let screen = best, area(screen.intersection(wanted)) > 0 {
                return clamp(wanted, into: screen)
            }
        }
        return defaultFrame(on: main)
    }

    /// Centrée, le haut au quart de l'écran, comme le panneau.
    static func defaultFrame(on screen: CGRect) -> CGRect {
        let top = screen.maxY - screen.height * 0.25
        return CGRect(x: screen.midX - size.width / 2, y: top - size.height,
                      width: size.width, height: size.height)
    }

    private static func clamp(_ r: CGRect, into s: CGRect) -> CGRect {
        CGRect(x: min(max(r.minX, s.minX), s.maxX - r.width),
               y: min(max(r.minY, s.minY), s.maxY - r.height),
               width: r.width, height: r.height)
    }

    private static func area(_ r: CGRect) -> CGFloat {
        r.isNull ? 0 : r.width * r.height
    }
}
```

- [ ] **Étape 4 : vérifier que les tests passent**

Run: la commande de l'étape 2.
Expected: `** TEST SUCCEEDED **`.

- [ ] **Étape 5 : commit**

```bash
git add macos/Sources/Panel/DisplayMode.swift macos/Sources/Panel/DisplayModeStore.swift macos/Tests/DisplayModeTests.swift macos/Tests/DisplayModeStoreTests.swift
git commit -m "$(cat <<'EOF'
feat(macos): quatre modes d'affichage et leurs transitions, retenus avec la place du mode petit

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Tâche 2 : la barre du mode petit

`MiniPlayerView` : lecture/pause, timeline, temps restant, vitesse, écrou ;
« Rien à lire » et le raccourci sans document. Le bouton de lecture et
l'écrou sortent de `PlayerBar` pour être partagés (identifiants et libellés
inchangés). Les textes de temps sont calculés à part, testés seuls.

**Fichiers :**
- Créer : `macos/Sources/Panel/PlaybackTime.swift`
- Créer : `macos/Sources/Panel/MiniPlayerView.swift`
- Modifier : `macos/Sources/Panel/PlayerBar.swift` (boutons lecture et écrou)
- Modifier : `macos/Sources/Panel/PanelContent.swift` (`PanelInvitation`)
- Créer : `macos/Tests/MiniPlayerTests.swift`

**Interfaces :**
- Consomme : `SmallPlacement.size` (Tâche 1), `PlayerBar.time(_:)`, `PlayerBar.speedText(_:)`, `PlayerBar.speedSpoken(_:)`, `Timeline`, `ReaderModel`, `Theme`, `KeyboardShortcuts.Name.readSelection`.
- Produit :
  - `enum PlaybackTime { static func remaining(elapsed: Double?, total: Double?) -> Double?; static func shortRemaining(elapsed:total:) -> String; static func spokenRemaining(elapsed:total:) -> String; static func timelineValue(elapsed: Double?, total: Double?, progress: Float) -> String; static func menuLine(elapsed:total:) -> String?; @MainActor static func emptyText(shortcut: KeyboardShortcuts.Shortcut?) -> String }`
  - `struct MiniPlayerView: View { let model: ReaderModel; let theme: Theme }` — identifiant `barre-mini`, texte vide `rien-a-lire`
  - `struct PlayButton: View { let model: ReaderModel }` (identifiant `lecture`), `struct VoiceSettingsButton: View` (identifiant `reglages-voix`)
  - `PanelInvitation.keys(_ shortcut: KeyboardShortcuts.Shortcut) -> String` (`@MainActor`)

- [ ] **Étape 1 : écrire les tests qui échouent**

`macos/Tests/MiniPlayerTests.swift` :

```swift
import KeyboardShortcuts
import SwiftUI
import XCTest
@testable import Sillon

@MainActor
final class PlaybackTimeTests: XCTestCase {
    func testTheRemainingTimeIsShownWithAMinus() {
        XCTAssertEqual(PlaybackTime.shortRemaining(elapsed: 56, total: 176), "-2:00")
        XCTAssertEqual(PlaybackTime.shortRemaining(elapsed: nil, total: 176), "–:––")
        XCTAssertEqual(PlaybackTime.shortRemaining(elapsed: 200, total: 176), "-0:00", "jamais négatif")
        XCTAssertEqual(PlaybackTime.shortRemaining(elapsed: 5, total: .nan), "–:––")
    }

    func testTheRemainingTimeIsSpokenWithoutTheMinus() {
        XCTAssertEqual(PlaybackTime.spokenRemaining(elapsed: 56, total: 176), "2:00")
        XCTAssertEqual(PlaybackTime.spokenRemaining(elapsed: nil, total: nil), "inconnu")
    }

    /// La timeline annonce « 0:56 sur 2:56 » ; sans estimation, le pourcentage.
    func testTheTimelineValue() {
        XCTAssertEqual(PlaybackTime.timelineValue(elapsed: 56, total: 176, progress: 0.3), "0:56 sur 2:56")
        XCTAssertEqual(PlaybackTime.timelineValue(elapsed: nil, total: nil, progress: 0.4), "40 %")
    }

    func testTheMenuLine() {
        XCTAssertEqual(PlaybackTime.menuLine(elapsed: 56, total: 176), "Reste environ 2:00")
        XCTAssertNil(PlaybackTime.menuLine(elapsed: nil, total: 176))
    }

    func testNothingToReadNamesTheChosenShortcut() {
        let space = KeyboardShortcuts.Shortcut(.space, modifiers: [.control, .option])
        XCTAssertEqual(PlaybackTime.emptyText(shortcut: space), "Rien à lire — ⌃⌥Espace")
        XCTAssertEqual(PlaybackTime.emptyText(shortcut: nil), "Rien à lire — raccourci : ⌘,")
    }
}

@MainActor
final class MiniPlayerViewTests: XCTestCase {
    /// La barre tient dans ses 44 pt, avec ou sans document.
    func testTheBarFitsInItsHeight() {
        let model = ReaderModel(reader: SillonReader.silent())
        let empty = NSHostingView(rootView: MiniPlayerView(model: model, theme: .paper))
        empty.frame = CGRect(origin: .zero, size: SmallPlacement.size)
        empty.layoutSubtreeIfNeeded()
        XCTAssertLessThanOrEqual(empty.fittingSize.height, SmallPlacement.size.height)

        model.open(text: "Un. Deux.", source: "test")
        XCTAssertTrue(waitUntil { model.refresh(); return model.snapshot.hasDocument })
        let full = NSHostingView(rootView: MiniPlayerView(model: model, theme: .paper))
        full.frame = CGRect(origin: .zero, size: SmallPlacement.size)
        full.layoutSubtreeIfNeeded()
        XCTAssertLessThanOrEqual(full.fittingSize.height, SmallPlacement.size.height)
    }
}
```

- [ ] **Étape 2 : vérifier l'échec**

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests/PlaybackTimeTests -only-testing:SillonTests/MiniPlayerViewTests`
Expected: FAIL — `cannot find 'PlaybackTime' in scope`, `cannot find 'MiniPlayerView' in scope`.

- [ ] **Étape 3 : partager le bouton de lecture et l'écrou**

Dans `macos/Sources/Panel/PlayerBar.swift`, remplacer le premier bouton :

```swift
            Button {
                model.send(.togglePlay)
            } label: {
                Image(systemName: s.playing ? "pause.fill" : "play.fill")
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("lecture")
            .accessibilityLabel(s.playing ? "Pause" : "Lecture")
```

par :

```swift
            PlayButton(model: model)
```

et le bloc de l'écrou (de `// L'écrou : les réglages de la voix…` à
`.accessibilityIdentifier("reglages-voix")`) par :

```swift
            // L'écrou : les réglages de la voix, pour en essayer une autre.
            VoiceSettingsButton()
```

Puis ajouter à la fin du fichier :

```swift
/// Lecture / pause, dans la barre du panneau comme dans la barre réduite.
struct PlayButton: View {
    let model: ReaderModel

    var body: some View {
        let playing = model.snapshot.playing
        Button {
            model.send(.togglePlay)
        } label: {
            Image(systemName: playing ? "pause.fill" : "play.fill")
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("lecture")
        .accessibilityLabel(playing ? "Pause" : "Lecture")
    }
}

/// L'écrou : les réglages de la voix (onglet Voix).
struct VoiceSettingsButton: View {
    var body: some View {
        Button {
            SettingsOpener.shared.open(tab: .voice)
        } label: {
            Image(systemName: "gearshape")
        }
        .buttonStyle(.plain)
        // Cible élargie sans toucher à la hauteur de la barre.
        .padding(.horizontal, 5)
        .contentShape(Rectangle())
        .help("Réglages de la voix (v)")
        .accessibilityLabel("Réglages de la voix")
        .accessibilityIdentifier("reglages-voix")
    }
}
```

Dans `macos/Sources/Panel/PanelContent.swift`, remplacer `enum PanelInvitation`
en entier par :

```swift
/// L'invitation du panneau vide, avec le raccourci « Lire » réellement choisi.
enum PanelInvitation {
    @MainActor
    static func text(for shortcut: KeyboardShortcuts.Shortcut?) -> String {
        let start = "Sélectionnez ou copiez du texte, puis "
        guard let shortcut else { return start + "choisissez un raccourci : ⌘," }
        return start + keys(shortcut)
    }

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

- [ ] **Étape 4 : écrire les temps et la barre**

`macos/Sources/Panel/PlaybackTime.swift` :

```swift
import KeyboardShortcuts

/// Les temps de la barre réduite et du menu, calculés sans vue.
enum PlaybackTime {
    /// Le temps restant estimé, jamais négatif ; nil tant que la durée
    /// n'est pas connue.
    static func remaining(elapsed: Double?, total: Double?) -> Double? {
        guard let elapsed, let total, elapsed.isFinite, total.isFinite else { return nil }
        return max(0, total - elapsed)
    }

    /// « -2:00 », ou « –:–– » tant que la durée n'est pas connue.
    static func shortRemaining(elapsed: Double?, total: Double?) -> String {
        remaining(elapsed: elapsed, total: total).map { "-" + PlayerBar.time($0) } ?? PlayerBar.time(nil)
    }

    /// Ce que VoiceOver dit du temps restant : « 2:00 ».
    static func spokenRemaining(elapsed: Double?, total: Double?) -> String {
        remaining(elapsed: elapsed, total: total).map { PlayerBar.time($0) } ?? "inconnu"
    }

    /// La valeur de la timeline pour VoiceOver : « 0:56 sur 2:56 », ou le
    /// pourcentage lu tant que la durée n'est pas connue.
    static func timelineValue(elapsed: Double?, total: Double?, progress: Float) -> String {
        if let elapsed, let total, elapsed.isFinite, total.isFinite {
            return "\(PlayerBar.time(elapsed)) sur \(PlayerBar.time(total))"
        }
        return "\(Int(Timeline.clamped(Double(progress)) * 100)) %"
    }

    /// La ligne du menu de la barre des menus.
    static func menuLine(elapsed: Double?, total: Double?) -> String? {
        remaining(elapsed: elapsed, total: total).map { "Reste environ " + PlayerBar.time($0) }
    }

    /// La barre réduite sans document.
    @MainActor
    static func emptyText(shortcut: KeyboardShortcuts.Shortcut?) -> String {
        guard let shortcut else { return "Rien à lire — raccourci : ⌘," }
        return "Rien à lire — " + PanelInvitation.keys(shortcut)
    }
}
```

`macos/Sources/Panel/MiniPlayerView.swift` :

```swift
import KeyboardShortcuts
import SwiftUI

/// La barre du mode petit : lecture, ligne de temps, temps restant,
/// vitesse, écrou. Contrôles à 13 pt, libellés comme la barre du panneau.
/// Elle lit l'instantané du modèle, à son rythme : rien ne tourne en pause.
struct MiniPlayerView: View {
    let model: ReaderModel
    let theme: Theme

    var body: some View {
        let s = model.snapshot
        HStack(spacing: 10) {
            if s.hasDocument {
                PlayButton(model: model)
                Timeline(progress: Double(s.progress), markers: [], theme: theme)
                    .frame(height: 9)
                    .accessibilityRepresentation {
                        ProgressView(value: Timeline.clamped(Double(s.progress)))
                            .accessibilityLabel("Progression")
                            .accessibilityValue(PlaybackTime.timelineValue(elapsed: s.elapsed, total: s.total,
                                                                           progress: s.progress))
                    }
                Text(PlaybackTime.shortRemaining(elapsed: s.elapsed, total: s.total))
                    .monospacedDigit()
                    .fontWeight(.medium)
                    .accessibilityLabel("Temps restant")
                    .accessibilityValue(PlaybackTime.spokenRemaining(elapsed: s.elapsed, total: s.total))
                Text(PlayerBar.speedText(s.speed))
                    .accessibilityLabel("Vitesse")
                    .accessibilityValue(PlayerBar.speedSpoken(s.speed))
            } else {
                Text(PlaybackTime.emptyText(shortcut: KeyboardShortcuts.getShortcut(for: .readSelection)))
                    .lineLimit(1)
                    .accessibilityIdentifier("rien-a-lire")
                Spacer(minLength: 0)
            }
            VoiceSettingsButton()
        }
        .font(.system(size: 13))
        .foregroundStyle(Color(nsColor: theme.chrome))
        .padding(.horizontal, 14)
        // Souple : la fenêtre fixe la taille (SmallPlacement.size).
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("barre-mini")
    }
}
```

- [ ] **Étape 5 : vérifier que les tests passent**

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests`
Expected: `** TEST SUCCEEDED **` (dont `PanelInvitationTests` et `PlayerBarFormatTests`, inchangés).

- [ ] **Étape 6 : commit**

```bash
git add macos/Sources/Panel/PlaybackTime.swift macos/Sources/Panel/MiniPlayerView.swift macos/Sources/Panel/PlayerBar.swift macos/Sources/Panel/PanelContent.swift macos/Tests/MiniPlayerTests.swift
git commit -m "$(cat <<'EOF'
feat(macos): la barre du mode petit — lecture, timeline, temps restant, vitesse, écrou

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Tâche 3 : ⌘1, ⌘2, ⌘3 et le panneau qui applique les modes

`KeyMap` donne ⌘1/⌘2/⌘3 ; `PanelController` applique chaque mode à
l'unique fenêtre (taille, contenu, identifiant), retient la place du mode
petit, restaure le mode au lancement. **Changement de comportement voulu par
la spec** : Échap et ⌘W ne mettent plus en pause — l'arrière-plan garde la
lecture. Le mode petit garde ↑ ↓ grâce à une copie cachée de la vue de
lecture, à la largeur du mode moyen.

**Fichiers :**
- Modifier : `macos/Sources/Input/KeyMap.swift`
- Modifier : `macos/Sources/Panel/ReadingPanel.swift` (retirer `PanelMode`, ajouter `horizontalPadding`)
- Modifier : `macos/Sources/Panel/PanelController.swift` (réécrit en entier)
- Modifier : `macos/Sources/Panel/PanelContent.swift` (`struct PanelContent`)
- Modifier : `macos/Sources/Reading/ReadingView.swift` (paramètre `shadow`)
- Modifier : `macos/Sources/AppDelegate.swift` (restauration, `--show-panel`, `ReadFlow`)
- Modifier : `macos/Tests/KeyMapTests.swift`
- Modifier : `macos/Tests/PanelControllerTests.swift`

**Interfaces :**
- Consomme : `DisplayMode`, `DisplayEvent`, `DisplayState`, `DisplayModeStore`, `SmallPlacement` (Tâche 1) ; `MiniPlayerView` (Tâche 2).
- Produit :
  - `ReaderAction.display(DisplayMode)`
  - `PanelController.init(model: ReaderModel, store: DisplayModeStore? = nil)`, `let store: DisplayModeStore`, `private(set) var mode: DisplayMode`, `func apply(_ event: DisplayEvent)`, `func show()` (= `.showLast`), `func restore()`, `func expand()`, `func collapseOrHide()` (= `.escape`), `func windowMoved()`, `var shadowTextWidth: CGFloat`
  - identifiants de la vue hôte : `lecteur-reduit` (« Lecteur réduit »), `panneau-lecture`, `lecteur-agrandi`
  - `ReadingView(model:style:lines:shadow:onContentHeight:)`
  - `PanelGeometry.horizontalPadding: CGFloat` (22)

- [ ] **Étape 1 : écrire les tests qui échouent**

Dans `macos/Tests/KeyMapTests.swift`, ajouter à `KeyMapTests` :

```swift
    func testCommandDigitsChooseTheDisplayMode() {
        XCTAssertEqual(act("1", command: true), .display(.small))
        XCTAssertEqual(act("2", command: true), .display(.medium))
        XCTAssertEqual(act("3", command: true), .display(.large))
        // AZERTY : les mêmes touches, sans Maj.
        XCTAssertEqual(act("&", command: true), .display(.small))
        XCTAssertEqual(act("é", command: true), .display(.medium))
        XCTAssertEqual(act("\"", command: true), .display(.large))
        XCTAssertNil(act("4", command: true))
        // Sans ⌘, les chiffres restent des vitesses.
        XCTAssertEqual(act("1"), .command(.setSpeed(speed: 0.75)))
    }
```

Dans `macos/Tests/PanelControllerTests.swift` :

1. Remplacer `shownPanel` par :

```swift
    /// Des préférences à part, vidées : jamais celles de l'utilisateur.
    private func freshDefaults() -> UserDefaults {
        let name = "sillon.tests.affichage.panneau"
        let defaults = UserDefaults(suiteName: name)!
        defaults.removePersistentDomain(forName: name)
        return defaults
    }

    private func shownPanel(_ text: String, defaults: UserDefaults? = nil) -> PanelController {
        let model = ReaderModel(reader: SillonReader.silent())
        model.open(text: text, source: "test")
        XCTAssertTrue(waitUntil { model.refresh(); return model.snapshot.revision == 1 })
        let c = PanelController(model: model, store: DisplayModeStore(defaults: defaults ?? freshDefaults()))
        controller = c
        c.show()
        settle(c)
        return c
    }
```

2. Dans `testNoAnnouncementWhenHiddenNorOnShowAfterHide`, remplacer :

```swift
        c.collapseOrHide()  // masque : pause
        XCTAssertNil(c.announcementText(for: c.model.snapshot), "rien panneau masqué")
        XCTAssertTrue(waitUntil { c.model.refresh(); return !c.model.snapshot.playing })
```

par :

```swift
        c.collapseOrHide()  // arrière-plan : la lecture continue
        XCTAssertNil(c.announcementText(for: c.model.snapshot), "rien en arrière-plan")
        c.model.send(.pause)
        XCTAssertTrue(waitUntil { c.model.refresh(); return !c.model.snapshot.playing })
        XCTAssertNil(c.announcementText(for: c.model.snapshot), "rien en arrière-plan")
```

3. Dans `testCollapsingFromExpandedMeasuresAgainAtPanelWidth`, remplacer
`XCTAssertEqual(c.mode, .expanded)` par `XCTAssertEqual(c.mode, .large)` et
`XCTAssertEqual(c.mode, .panel)` par `XCTAssertEqual(c.mode, .medium)`.

4. Remplacer `testClosingTheWindowGoesThroughTheHidePath` et
`testCloseDirectlyAlsoHides` par :

```swift
    /// ⌘W : l'arrière-plan, sans pause (la spec : la lecture continue).
    func testClosingTheWindowGoesToTheBackgroundWithoutPausing() {
        let c = shownPanel("Un. Deux.")
        XCTAssertTrue(c.model.isDisplayed)
        let commands = c.model.commandCount
        c.window.performClose(nil)
        XCTAssertEqual(c.mode, .background)
        XCTAssertFalse(c.model.isDisplayed)
        XCTAssertFalse(c.window.isVisible)
        XCTAssertEqual(c.model.commandCount, commands, "aucune pause envoyée")
        XCTAssertEqual(c.store.state.current, .background)
    }

    func testCloseDirectlyAlsoHides() {
        let c = shownPanel("Un. Deux.")
        c.window.close()
        XCTAssertEqual(c.mode, .background)
        XCTAssertFalse(c.model.isDisplayed)
        // Réafficher fonctionne encore.
        c.show()
        XCTAssertEqual(c.mode, .medium)
        XCTAssertTrue(c.window.isVisible)
    }
```

5. Ajouter à `PanelControllerTests` :

```swift
    private var screen: CGRect { (NSScreen.main ?? NSScreen.screens[0]).visibleFrame }

    func testCommandDigitsGiveEachModeItsSize() {
        let c = shownPanel("Une seule phrase, assez courte.")
        let medium = c.window.frame
        c.perform(.display(.small))
        settle(c)
        XCTAssertEqual(c.mode, .small)
        XCTAssertEqual(c.window.frame.width, SmallPlacement.size.width, accuracy: 1)
        XCTAssertEqual(c.window.frame.height, SmallPlacement.size.height, accuracy: 1)
        XCTAssertEqual(c.window.contentView?.accessibilityIdentifier(), "lecteur-reduit")
        c.perform(.display(.large))
        settle(c)
        XCTAssertEqual(c.mode, .large)
        XCTAssertEqual(c.window.frame, PanelGeometry.expandedFrame(screen: screen))
        XCTAssertEqual(c.window.contentView?.accessibilityIdentifier(), "lecteur-agrandi")
        c.perform(.display(.medium))
        settle(c)
        XCTAssertEqual(c.mode, .medium)
        XCTAssertEqual(c.window.frame.width, medium.width, accuracy: 1)
        XCTAssertEqual(c.window.frame.height, medium.height, accuracy: 1)
        XCTAssertEqual(c.window.contentView?.accessibilityIdentifier(), "panneau-lecture")
    }

    func testEscapeGoesFromLargeToMediumThenToTheBackgroundWithoutPausing() {
        let c = shownPanel("Un. Deux.")
        c.perform(.display(.large))
        settle(c)
        c.perform(.command(.togglePlay))
        let commands = c.model.commandCount
        c.perform(.collapseOrHide)
        XCTAssertEqual(c.mode, .medium)
        settle(c)
        c.perform(.collapseOrHide)
        XCTAssertEqual(c.mode, .background)
        XCTAssertFalse(c.window.isVisible)
        XCTAssertEqual(c.model.commandCount, commands, "la lecture continue en arrière-plan")
    }

    func testEscapeFromTheSmallBarGoesToTheBackground() {
        let c = shownPanel("Un. Deux.")
        c.perform(.display(.small))
        settle(c)
        c.perform(.collapseOrHide)
        XCTAssertEqual(c.mode, .background)
        XCTAssertFalse(c.window.isVisible)
    }

    /// ⌃⌥S : le dernier mode visible ; ⏎ depuis la barre : le grand mode.
    func testShowBringsBackTheLastVisibleMode() {
        let c = shownPanel("Un. Deux.")
        c.perform(.display(.small))
        settle(c)
        c.collapseOrHide()
        c.show()
        settle(c)
        XCTAssertEqual(c.mode, .small)
        XCTAssertTrue(c.window.isVisible)
        c.perform(.expand)
        settle(c)
        XCTAssertEqual(c.mode, .large)
    }

    /// ⌃⌥Espace garde le mode ; depuis l'arrière-plan, il ouvre le moyen.
    func testReadingASelectionKeepsTheModeOrOpensTheMediumOne() {
        let c = shownPanel("Un. Deux.")
        c.perform(.display(.small))
        settle(c)
        c.apply(.readSelection)
        XCTAssertEqual(c.mode, .small)
        c.collapseOrHide()
        XCTAssertEqual(c.mode, .background)
        c.apply(.readSelection)
        XCTAssertEqual(c.mode, .medium)
        XCTAssertTrue(c.window.isVisible)
    }

    func testTheSmallBarKeepsItsPlaceFromOneTimeToTheNext() {
        let defaults = freshDefaults()
        let c = shownPanel("Un. Deux.", defaults: defaults)
        c.perform(.display(.small))
        settle(c)
        let main = NSScreen.screens[0].visibleFrame
        let spot = CGPoint(x: main.minX + 40, y: main.minY + 40)
        c.window.setFrameOrigin(spot)
        XCTAssertTrue(waitUntil { c.store.smallOrigin == spot }, "le déplacement n'est pas retenu")
        c.collapseOrHide()
        c.show()
        settle(c)
        XCTAssertEqual(c.window.frame.minX, spot.x, accuracy: 1)
        XCTAssertEqual(c.window.frame.minY, spot.y, accuracy: 1)
        // Relance : un nouveau contrôleur, mêmes préférences.
        c.collapseOrHide()
        let again = PanelController(model: c.model, store: DisplayModeStore(defaults: defaults))
        controller = again
        again.show()
        settle(again)
        XCTAssertEqual(again.mode, .small)
        XCTAssertEqual(again.window.frame.minX, spot.x, accuracy: 1)
        XCTAssertEqual(again.window.frame.minY, spot.y, accuracy: 1)
    }

    func testASmallBarLeftOnAnUnpluggedScreenComesBackToTheMainScreen() {
        let defaults = freshDefaults()
        DisplayModeStore(defaults: defaults).smallOrigin = CGPoint(x: -50_000, y: -50_000)
        let c = shownPanel("Un.", defaults: defaults)
        c.perform(.display(.small))
        settle(c)
        XCTAssertTrue(NSScreen.screens[0].visibleFrame.contains(c.window.frame),
                      "barre hors écran : \(c.window.frame)")
    }

    /// Relance : le mode visible retenu reparaît ; l'arrière-plan, rien.
    func testTheModeComesBackAfterARelaunchButNotTheBackground() {
        let defaults = freshDefaults()
        let c = shownPanel("Un.", defaults: defaults)
        c.perform(.display(.large))
        settle(c)
        let relaunched = PanelController(model: c.model, store: DisplayModeStore(defaults: defaults))
        relaunched.restore()
        settle(relaunched)
        XCTAssertEqual(relaunched.mode, .large)
        XCTAssertTrue(relaunched.window.isVisible)
        relaunched.collapseOrHide()
        relaunched.collapseOrHide()
        let third = PanelController(model: c.model, store: DisplayModeStore(defaults: defaults))
        third.restore()
        XCTAssertEqual(third.mode, .background)
        XCTAssertFalse(third.window.isVisible)
        third.show()
        XCTAssertEqual(third.mode, .medium, "Échap du grand mode avait retenu le moyen")
        third.collapseOrHide()
    }

    /// Les réglages ouverts, aucun mode ne remet le panneau au-dessus.
    func testModesKeepThePanelUnderTheOpenSettings() {
        let c = shownPanel("Un. Deux.")
        let settings = NSWindow()
        settings.setAccessibilityIdentifier("Réglages")
        NotificationCenter.default.post(name: NSWindow.didBecomeKeyNotification, object: settings)
        for mode in [DisplayMode.small, .large, .medium] {
            c.perform(.display(mode))
            settle(c)
            XCTAssertEqual(c.window.level, .normal, "\(mode)")
        }
        c.collapseOrHide()
        c.show()
        XCTAssertEqual(c.window.level, .normal, "réaffiché")
        NotificationCenter.default.post(name: NSWindow.willCloseNotification, object: settings)
        XCTAssertEqual(c.window.level, .floating)
    }

    /// ↑ ↓ dans la barre : la ligne suit le texte même invisible, aux
    /// lignes du mode moyen.
    func testLinesFollowTheTextEvenInTheSmallBar() throws {
        let words = (1...60).map { "mot\($0)" }
        let c = shownPanel(words.joined(separator: " ") + ".")
        let text = try XCTUnwrap(textView(c))
        let string = text.string as NSString
        var ranges: [NSRange] = []
        string.enumerateSubstrings(in: NSRange(location: 0, length: string.length),
                                   options: .byWords) { _, range, _, _ in ranges.append(range) }
        let top = { (r: NSRange) in text.rectForTesting(r)!.minY }
        let next = try XCTUnwrap(ranges.firstIndex { top($0) > top(ranges[0]) + 1 },
                                 "le texte doit tenir sur plusieurs lignes")
        let expected = c.model.document.blocks.flatMap(\.sentences).flatMap(\.words)[next].index

        c.perform(.display(.small))
        settle(c)
        XCTAssertTrue(waitUntil { self.textView(c).map { $0 !== text } ?? false },
                      "la copie cachée de la vue de lecture manque")
        c.perform(.line(delta: 1))
        XCTAssertTrue(waitUntil { c.model.refresh(); return c.model.snapshot.word == expected },
                      "mot visé : \(expected), lu : \(String(describing: c.model.snapshot.word))")
    }
```

- [ ] **Étape 2 : vérifier l'échec**

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests/KeyMapTests -only-testing:SillonTests/PanelControllerTests`
Expected: FAIL — `type 'ReaderAction' has no member 'display'`, `extra argument 'store' in call`.

- [ ] **Étape 3 : ⌘1, ⌘2, ⌘3 dans `KeyMap`**

Dans `macos/Sources/Input/KeyMap.swift`, remplacer :

```swift
    case expand, collapseOrHide, copySentence, openSearch
```

par :

```swift
    case expand, collapseOrHide, copySentence, openSearch
    /// ⌘1, ⌘2, ⌘3 : petit, moyen, grand.
    case display(DisplayMode)
```

et le bloc `if key.command { … }` par :

```swift
        if key.command {
            switch key.characters {
            case "c": return .copySentence
            case ",": return .openSettings
            // QWERTY et AZERTY non décalé : les mêmes touches physiques.
            case "1", "&": return .display(.small)
            case "2", "é": return .display(.medium)
            case "3", "\"": return .display(.large)
            default: return nil
            }
        }
```

- [ ] **Étape 4 : la géométrie et la copie cachée de la vue de lecture**

Dans `macos/Sources/Panel/ReadingPanel.swift`, supprimer la ligne
`enum PanelMode: Equatable { case hidden, panel, expanded }` et ajouter dans
`enum PanelGeometry`, sous `maximumWidth` :

```swift
    /// La marge de chaque côté du contenu, moyen et grand modes.
    static let horizontalPadding: CGFloat = 22
```

Dans `macos/Sources/Reading/ReadingView.swift`, sous `var lines: LineNavigator?`,
ajouter :

```swift
    /// Copie invisible (mode petit) : elle ne sert qu'à ↑ ↓. Ni trait animé,
    /// ni VoiceOver (une vue cachée sort de l'arbre d'accessibilité).
    var shadow = false
```

dans `makeNSView`, avant `return scroll` :

```swift
        scroll.isHidden = shadow
```

et dans `updateNSView`, remplacer `text.playing = snapshot.playing` par :

```swift
        text.playing = snapshot.playing && !shadow
```

- [ ] **Étape 5 : le contenu de la fenêtre par mode**

Dans `macos/Sources/Panel/PanelContent.swift`, remplacer `var body` de
`struct PanelContent` et ce qui le suit jusqu'à `private func hint`
(exclu) par :

```swift
    var body: some View {
        content(controller.model)
            .background(
                RoundedRectangle(cornerRadius: 14)
                    .fill(Color(nsColor: theme.background))
                    .overlay(RoundedRectangle(cornerRadius: 14)
                        .stroke(Color(nsColor: theme.border), lineWidth: 1))
            )
            .overlay { if controller.helpVisible { HelpOverlay(theme: theme) } }
            // La barre de titre est transparente et invisible : sans cela, sa
            // zone sûre repousse le contenu de ~30 pt et rogne le texte.
            .ignoresSafeArea()
            .onChange(of: controller.model.snapshot) { _, s in controller.announce(s) }
            // Un réglage écrit (interlignage, règle, pas d'avance) s'applique
            // au panneau ouvert, sans attendre sa prochaine ouverture.
            .onChange(of: controller.model.settingsRevision) { _, _ in controller.reloadReading() }
            .onReceive(NSWorkspace.shared.notificationCenter.publisher(
                for: NSWorkspace.accessibilityDisplayOptionsDidChangeNotification)) { _ in
                increaseContrast = NSWorkspace.shared.accessibilityDisplayShouldIncreaseContrast
            }
    }

    @ViewBuilder
    private func content(_ model: ReaderModel) -> some View {
        switch controller.mode {
        case .small:
            MiniPlayerView(model: model, theme: theme)
                // ↑ ↓ suivent le texte même invisible : une copie cachée de
                // la vue de lecture, à la largeur du mode moyen. Un fond ne
                // compte pas dans la taille de la barre.
                .background(alignment: .topLeading) {
                    ReadingView(model: model, style: style, lines: controller.lines, shadow: true)
                        .frame(width: controller.shadowTextWidth, height: 40)
                }
        case .large:
            ExpandedReader(controller: controller, style: style)
                .padding(.horizontal, PanelGeometry.horizontalPadding)
                .padding(.vertical, 18)
        case .medium, .background:
            VStack(alignment: .leading, spacing: 14) {
                ReadingView(model: model, style: style, lines: controller.lines) { height in
                    controller.contentHeightChanged(height)
                }
                // Sans document, l'invitation occupe la zone du texte, au
                // centre, à la taille des contrôles.
                .overlay {
                    if !model.snapshot.hasDocument {
                        Text(PanelInvitation.text(for: KeyboardShortcuts.getShortcut(for: .readSelection)))
                            .font(.system(size: 13))
                            .multilineTextAlignment(.center)
                            .foregroundStyle(Color(nsColor: theme.chrome))
                            .accessibilityIdentifier("invitation")
                    }
                }
                PlayerBar(model: model, theme: theme)
                HStack(spacing: 14) {
                    hint("⏎", "agrandir")
                    hint("esc", "masquer")
                    hint("?", "raccourcis")
                    // Le message prend place dans la rangée des aides : la
                    // hauteur réservée autour du texte (PanelGeometry.chrome)
                    // ne change pas quand il paraît.
                    if let message = PanelStatus.line(message: model.message, snapshot: model.snapshot) {
                        Spacer(minLength: 8)
                        Text(message)
                            .font(.system(size: 13))
                            .lineLimit(1)
                            .foregroundStyle(Color(nsColor: theme.chrome))
                    }
                }
            }
            .padding(.horizontal, PanelGeometry.horizontalPadding)
            .padding(.vertical, 18)
        }
    }

```

- [ ] **Étape 6 : `PanelController`, en entier**

`macos/Sources/Panel/PanelController.swift` :

```swift
import AppKit
import SwiftUI

/// Montre la fenêtre dans l'un des quatre modes — petit, moyen, grand,
/// arrière-plan — et route le clavier. Une seule fenêtre : chaque mode change
/// son contenu et sa taille ; l'arrière-plan la retire et la lecture continue.
@MainActor
@Observable
final class PanelController {
    @ObservationIgnored let model: ReaderModel
    @ObservationIgnored let window = ReadingPanel()
    /// Mode courant, dernier mode visible et place du mode petit, retenus
    /// d'un lancement à l'autre.
    @ObservationIgnored let store: DisplayModeStore
    /// Durée d'affichage d'un message passager (« Phrase copiée »).
    @ObservationIgnored var messageDuration: TimeInterval = 2
    @ObservationIgnored private var keyMonitor: Any?
    @ObservationIgnored private var contentHeight: CGFloat = 0
    @ObservationIgnored private var resizePending = false
    @ObservationIgnored private var closeGuard: PanelCloseGuard?
    @ObservationIgnored private var announcer = Announcer()
    /// ↑ ↓ : la vue de lecture affichée (ou sa copie cachée du mode petit)
    /// dit où commence la ligne voisine.
    @ObservationIgnored let lines = LineNavigator()
    /// Les bornes de taille hors du mode petit, qui ne se redimensionne pas.
    @ObservationIgnored private var normalMinSize = NSSize.zero
    @ObservationIgnored private var normalMaxSize = NSSize.zero
    /// Vrai pendant que le code place la fenêtre : seul un déplacement de
    /// l'utilisateur fixe la place du mode petit.
    @ObservationIgnored private var placing = false
    @ObservationIgnored private var moveObserver: NSObjectProtocol?

    private(set) var mode: DisplayMode = .background
    /// Réglages de lecture du moteur (interlignage, règle), relus à chaque
    /// ouverture et à chaque réglage écrit.
    var reading = ReadingDisplay()
    var sidebarVisible = true
    var bookmarksVisible = false
    var searchActive = false
    /// Compte les « / » : chacun rend le focus au champ, même déjà ouvert.
    private(set) var openSearchRequests = 0
    var helpVisible = false

    init(model: ReaderModel, store: DisplayModeStore? = nil) {
        self.model = model
        self.store = store ?? DisplayModeStore(defaults: DisplayModeStore.processDefaults())
        let host = NSHostingView(rootView: PanelContent(controller: self))
        window.contentView = host
        host.setAccessibilityIdentifier("panneau-lecture")
        host.setAccessibilityLabel("Panneau de lecture")
        // Toute fermeture (⌘W, close(), performClose:) passe à l'arrière-plan.
        let guardian = PanelCloseGuard { [weak self] in self?.apply(.hide) }
        closeGuard = guardian
        window.delegate = guardian
        normalMinSize = window.minSize
        normalMaxSize = window.maxSize
        observeSettingsWindow()
        observeMoves()
    }

    private var screenFrame: CGRect {
        (NSScreen.main ?? NSScreen.screens[0]).visibleFrame
    }

    /// L'écran principal (celui de la barre des menus).
    private var primaryScreenFrame: CGRect {
        NSScreen.screens.first?.visibleFrame ?? screenFrame
    }

    /// La largeur du texte du mode moyen : la copie cachée du mode petit la
    /// garde, pour que ↑ ↓ visent les mêmes lignes.
    var shadowTextWidth: CGFloat {
        PanelGeometry.panelFrame(contentHeight: 0, screen: screenFrame).width
            - 2 * PanelGeometry.horizontalPadding
    }

    // MARK: - Modes

    /// Une transition (⌘1/2/3, Échap, ⌃⌥S, ⌃⌥Espace, fermeture), retenue
    /// puis appliquée.
    func apply(_ event: DisplayEvent) {
        let next = store.state.applying(event)
        store.state = next
        present(next.current)
    }

    /// ⌃⌥S, « Afficher sillon » : le dernier mode visible.
    func show() {
        apply(.showLast)
    }

    /// Au lancement : le mode retenu reparaît, sans prendre le clavier à
    /// l'application au premier plan. Arrière-plan : rien à montrer.
    func restore() {
        let current = store.state.current
        guard current.isVisible else { return }
        present(current, takeKey: false)
    }

    /// ⏎ : le grand mode, depuis le moyen comme depuis la barre.
    func expand() {
        guard mode == .small || mode == .medium else { return }
        apply(.choose(.large))
    }

    /// Échap : du grand au moyen, sinon l'arrière-plan.
    func collapseOrHide() {
        apply(.escape)
    }

    private func present(_ target: DisplayMode, takeKey: Bool = true) {
        guard target.isVisible else { hideWindow(); return }
        // Lu hors du fil principal : s'applique dès qu'il revient.
        reloadReading()
        let wasHidden = mode == .background || !window.isVisible
        if wasHidden || target != mode {
            mode = target
            applySizeLimits(for: target)
            place(frame(for: target), animated: !wasHidden)
            updateIdentifier()
        }
        if takeKey { window.makeKeyAndOrderFront(nil) } else { window.orderFrontRegardless() }
        // Le modèle ne suit l'état en pause (coup d'œil lent) que fenêtre visible.
        model.isDisplayed = true
        // L'annonceur ne voit rien en arrière-plan : on repart de l'état
        // courant pour ne rien annoncer de ce qui s'est passé entre-temps.
        model.refresh()
        announcer.reseed(with: model.snapshot)
        installKeyMonitor()
    }

    /// L'arrière-plan : la fenêtre se retire, la lecture continue. Le suivi à
    /// chaque image ne tourne plus que si la voix lit (cadence du modèle).
    private func hideWindow() {
        guard mode != .background else { return }
        mode = .background
        if window.isVisible { window.orderOut(nil) }
        model.isDisplayed = false
        removeKeyMonitor()
    }

    private func frame(for mode: DisplayMode) -> CGRect {
        switch mode {
        case .small:
            return SmallPlacement.frame(savedOrigin: store.smallOrigin,
                                        screens: NSScreen.screens.map(\.visibleFrame),
                                        main: primaryScreenFrame)
        case .large:
            return PanelGeometry.expandedFrame(screen: screenFrame)
        case .medium, .background:
            return PanelGeometry.panelFrame(contentHeight: contentHeight, screen: screenFrame)
        }
    }

    /// Le mode petit ne se redimensionne pas ; les autres reprennent leurs
    /// bornes. Avant de placer la fenêtre : des bornes de barre bloqueraient
    /// le retour au panneau.
    private func applySizeLimits(for mode: DisplayMode) {
        if mode == .small {
            window.minSize = SmallPlacement.size
            window.maxSize = SmallPlacement.size
        } else {
            window.minSize = normalMinSize
            window.maxSize = normalMaxSize
        }
    }

    private func place(_ frame: CGRect, animated: Bool) {
        placing = true
        defer { placing = false }
        if animated, !NSWorkspace.shared.accessibilityDisplayShouldReduceMotion {
            window.setFrame(frame, display: true, animate: true)
        } else {
            window.setFrame(frame, display: animated)
        }
    }

    private func observeMoves() {
        moveObserver = NotificationCenter.default.addObserver(
            forName: NSWindow.didMoveNotification, object: window, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.windowMoved() }
        }
    }

    /// La barre déplacée par l'utilisateur : sa place est retenue.
    func windowMoved() {
        guard mode == .small, !placing else { return }
        store.smallOrigin = window.frame.origin
    }

    // MARK: - Réglages

    /// Relit ce que le panneau affiche des réglages, hors du fil principal :
    /// un changement dans la fenêtre des réglages s'y voit aussitôt.
    /// Des relectures qui se chevauchent : seule la dernière demandée compte.
    func reloadReading() {
        readingGeneration &+= 1
        let generation = readingGeneration
        Task {
            let loaded = ReadingDisplay(await model.loadSettings())
            guard generation == readingGeneration else { return }
            reading = loaded
        }
    }
    @ObservationIgnored private var readingGeneration = 0

    /// Fenêtre des réglages ouverte : le panneau reste affiché à côté, mais
    /// ne la recouvre plus, quel que soit le mode.
    private(set) var settingsVisible = false

    func settingsVisibilityChanged(_ visible: Bool) {
        guard visible != settingsVisible else { return }
        settingsVisible = visible
        window.level = PanelLevel.level(settingsVisible: visible)
    }

    private func observeSettingsWindow() {
        let center = NotificationCenter.default
        settingsObservers = [
            center.addObserver(forName: NSWindow.didBecomeKeyNotification, object: nil, queue: .main) { [weak self] note in
                MainActor.assumeIsolated {
                    guard let self, let w = note.object as? NSWindow, PanelLevel.isSettingsWindow(w) else { return }
                    self.settingsVisibilityChanged(true)
                    // Devant le panneau, sans passer au-dessus des autres apps.
                    w.orderFront(nil)
                }
            },
            center.addObserver(forName: NSWindow.willCloseNotification, object: nil, queue: .main) { [weak self] note in
                MainActor.assumeIsolated {
                    guard let self, let w = note.object as? NSWindow, PanelLevel.isSettingsWindow(w) else { return }
                    self.settingsVisibilityChanged(false)
                }
            },
        ]
    }
    @ObservationIgnored private var settingsObservers: [NSObjectProtocol] = []

    // MARK: - Clavier

    /// L'action d'une touche, avec le pas d'avance rapide des réglages.
    func action(for press: KeyPress) -> ReaderAction? {
        KeyMap.action(for: press, seekStep: Int32(reading.seekStep))
    }

    /// La phrase à dire, ou rien : jamais en arrière-plan (fenêtre retirée).
    /// L'annonceur voit tous les instantanés visibles, VoiceOver ou non, pour
    /// que son état reste à jour.
    func announcementText(for s: Snapshot) -> String? {
        let text = announcer.announcement(for: s, doc: model.document)
        return mode == .background ? nil : text
    }

    /// Dit à VoiceOver un changement d'état (lecture, pause, section), jamais
    /// un mot. Sans effet quand VoiceOver est éteint.
    func announce(_ s: Snapshot) {
        guard let text = announcementText(for: s), NSWorkspace.shared.isVoiceOverEnabled else { return }
        NSAccessibility.post(element: window, notification: .announcementRequested,
                             userInfo: [.announcement: text,
                                        .priority: NSAccessibilityPriorityLevel.medium.rawValue])
    }

    func contentHeightChanged(_ height: CGFloat) {
        guard height != contentHeight else { return }
        contentHeight = height
        // Appelé pendant la mise à jour SwiftUI : redimensionner la fenêtre
        // au milieu de cette passe est annulé. On le fait au tour suivant,
        // une seule fois pour toutes les hauteurs reçues entre-temps.
        guard !resizePending else { return }
        resizePending = true
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            self.resizePending = false
            guard self.mode == .medium else { return }
            self.place(PanelGeometry.panelFrame(contentHeight: self.contentHeight,
                                                screen: self.screenFrame), animated: true)
        }
    }

    func perform(_ action: ReaderAction) {
        switch action {
        case .command(let command): model.send(command)
        case .line(let delta): moveLine(delta)
        case .expand: expand()
        case .display(let target): apply(.choose(target))
        case .collapseOrHide:
            if helpVisible { helpVisible = false }
            else if searchActive { searchActive = false; model.send(.searchCancel) }
            else { collapseOrHide() }
        case .copySentence:
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(model.currentSentenceText, forType: .string)
            flash("Phrase copiée")
        case .openSearch:
            if mode != .large { apply(.choose(.large)) }
            searchActive = true
            openSearchRequests &+= 1
        case .toggleSidebar:
            if mode == .large { sidebarVisible.toggle() }
        case .toggleBookmarks:
            if mode != .large { apply(.choose(.large)) }
            sidebarVisible = true
            bookmarksVisible.toggle()
        case .showHelp:
            // La barre du mode petit n'a pas la place : l'aide s'ouvre au moyen.
            if mode == .small {
                apply(.choose(.medium))
                helpVisible = true
            } else {
                helpVisible.toggle()
            }
        case .openSettings:
            SettingsOpener.shared.open()
        case .openVoiceSettings:
            SettingsOpener.shared.open(tab: .voice)
        }
    }

    /// La voix reprend au premier mot de la ligne affichée voisine ; rien
    /// au-delà de la première ou de la dernière.
    private func moveLine(_ delta: Int) {
        let s = model.snapshot
        guard s.hasDocument else { return }
        guard let target = lines.target(word: s.word, sentence: s.sentence, delta: delta) else {
            flash("Aucune ligne de ce côté")
            return
        }
        model.send(.seekToWord(sentence: target.sentence, word: target.word))
    }

    /// Un message qui s'efface seul, sauf si un autre l'a remplacé.
    private func flash(_ text: String) {
        model.message = text
        messageToken &+= 1
        let token = messageToken
        DispatchQueue.main.asyncAfter(deadline: .now() + messageDuration) { [weak self] in
            guard let self, self.messageToken == token, self.model.message == text else { return }
            self.model.message = nil
        }
    }
    @ObservationIgnored private var messageToken = 0

    /// L'identifiant pour les tests, le nom pour VoiceOver.
    private func updateIdentifier() {
        let identifier: String
        let label: String
        switch mode {
        case .small:
            identifier = "lecteur-reduit"; label = "Lecteur réduit"
        case .large:
            identifier = "lecteur-agrandi"; label = "Lecteur agrandi"
        case .medium, .background:
            identifier = "panneau-lecture"; label = "Panneau de lecture"
        }
        window.contentView?.setAccessibilityIdentifier(identifier)
        window.contentView?.setAccessibilityLabel(label)
    }

    private func installKeyMonitor() {
        guard keyMonitor == nil else { return }
        keyMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self, event.window === self.window else { return event }
            // Un champ de saisie garde ses touches, sauf Échap.
            if self.window.firstResponder is NSTextView,
               !(self.window.firstResponder is ReadingTextView),
               event.keyCode != KeyCode.escape {
                return event
            }
            // Une liste (sommaire, signets) garde ses flèches verticales et
            // Entrée : on la parcourt, puis on y va.
            if self.window.firstResponder is NSTableView,
               [KeyCode.up, KeyCode.down, KeyCode.enter].contains(event.keyCode),
               !event.modifierFlags.contains(.option) {
                return event
            }
            guard let action = self.action(for: KeyPress(event)) else { return event }
            self.perform(action)
            return nil
        }
    }

    private func removeKeyMonitor() {
        if let keyMonitor { NSEvent.removeMonitor(keyMonitor) }
        keyMonitor = nil
    }
}

/// Le délégué de la fenêtre : une fermeture demandée devient l'arrière-plan.
@MainActor
private final class PanelCloseGuard: NSObject, NSWindowDelegate {
    let hide: () -> Void
    init(hide: @escaping () -> Void) { self.hide = hide }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        hide()
        return false
    }

    /// `close()` ne demande pas l'avis du délégué : on remet l'état d'accord.
    func windowWillClose(_ notification: Notification) {
        hide()
    }
}
```

- [ ] **Étape 7 : le lancement et « Lire » suivent les modes**

Dans `macos/Sources/AppDelegate.swift`, remplacer :

```swift
        panel = PanelController(model: model)
        if isUITesting {
            model.open(text: Self.sample, source: "exemple")
            if ProcessInfo.processInfo.arguments.contains("--show-panel") { panel.show() }
```

par :

```swift
        panel = PanelController(model: model)
        // Le mode laissé à la dernière fermeture (sous les tests : suite
        // vidée, donc l'arrière-plan).
        panel.restore()
        if isUITesting {
            model.open(text: Self.sample, source: "exemple")
            if ProcessInfo.processInfo.arguments.contains("--show-panel") { panel.apply(.choose(.medium)) }
```

et, dans `read(capture:)`, remplacer `show: { panel.show() }` par :

```swift
                // Le mode en cours ; depuis l'arrière-plan, le moyen.
                show: { panel.apply(.readSelection) }
```

- [ ] **Étape 8 : vérifier que les tests passent**

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests`
Expected: `** TEST SUCCEEDED **`. Si `testCommandDigitsGiveEachModeItsSize`
trouve une hauteur supérieure à 44 pt, vérifier d'abord que `MiniPlayerView`
garde `.frame(maxWidth: .infinity, maxHeight: .infinity)` (la vue hôte borne
la fenêtre à la taille de son contenu) — ne pas assouplir le test.

- [ ] **Étape 9 : commit**

```bash
git add macos/Sources/Input/KeyMap.swift macos/Sources/Panel/ReadingPanel.swift macos/Sources/Panel/PanelController.swift macos/Sources/Panel/PanelContent.swift macos/Sources/Reading/ReadingView.swift macos/Sources/AppDelegate.swift macos/Tests/KeyMapTests.swift macos/Tests/PanelControllerTests.swift
git commit -m "$(cat <<'EOF'
feat(macos): ⌘1 ⌘2 ⌘3 — la fenêtre en petit, moyen ou grand ; Échap passe en arrière-plan sans pause

Le mode et la place de la barre sont retenus ; ⌃⌥S rend le dernier mode
visible, ⌃⌥Espace ouvre le moyen depuis l'arrière-plan, ↑ ↓ suivent le
texte même dans la barre.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Tâche 4 : les commandes venues d'ailleurs que du panneau

`Transport` traduit lecture, pause, phrase suivante ou précédente et le saut
de timeline en commandes du lecteur ; sans document, rien ne part.
`ReaderModel` prévient d'un changement d'instantané publié (crochet
`onChange`, jamais à chaque image) et relit l'état après « Mettre la voix au
repos ».

**Fichiers :**
- Créer : `macos/Sources/App/Transport.swift`
- Modifier : `macos/Sources/Model/ReaderModel.swift` (`onChange`, `refresh()`, `restVoice()`)
- Modifier : `macos/Tests/Samples.swift` (`Snapshot.sample`)
- Créer : `macos/Tests/TransportTests.swift`
- Modifier : `macos/Tests/ReaderModelTests.swift`

**Interfaces :**
- Consomme : `ReaderModel.{snapshot, document, refresh(), send(_:)}`, `ReaderCommand`.
- Produit :
  - `enum RemoteAction: Equatable { case play, pause, toggle, next, previous, seek(TimeInterval) }`
  - `@MainActor struct Transport { init(snapshot: @escaping () -> Snapshot, document: @escaping () -> DocView, send: @escaping (ReaderCommand) -> Void); init(model: ReaderModel); @discardableResult func perform(_ action: RemoteAction) -> Bool; static func sentence(at position: TimeInterval, snapshot: Snapshot, document: DocView) -> UInt32? }`
  - `ReaderModel.onChange: (@MainActor (Snapshot, DocView) -> Void)?`
  - (tests) `Snapshot.sample(playing:sentence:elapsed:total:speed:voice:hasDocument:revision:)`

- [ ] **Étape 1 : écrire les tests qui échouent**

Ajouter à `macos/Tests/Samples.swift` :

```swift
extension Snapshot {
    /// Un instantané de lecteur muet, aux valeurs choisies.
    static func sample(playing: Bool = false, sentence: UInt32 = 0, elapsed: Double? = 56,
                       total: Double? = 176, speed: Float = 1, voice: VoiceState = .ready,
                       hasDocument: Bool = true, revision: UInt64 = 1) -> Snapshot {
        var s = SillonReader.silent().snapshot()
        s.hasDocument = hasDocument
        s.playing = playing
        s.sentence = sentence
        s.elapsed = elapsed
        s.total = total
        s.speed = speed
        s.voice = voice
        s.revision = revision
        return s
    }
}
```

`macos/Tests/TransportTests.swift` :

```swift
import XCTest
@testable import Sillon

@MainActor
final class TransportTests: XCTestCase {
    private var sent: [ReaderCommand] = []
    private var current = Snapshot.sample()
    private var doc = DocView(title: "t", complete: true, sentenceCount: 0, sections: [], blocks: [])

    private func transport() -> Transport {
        Transport(snapshot: { [unowned self] in self.current },
                  document: { [unowned self] in self.doc },
                  send: { [unowned self] in self.sent.append($0) })
    }

    /// Un vrai document, découpé par le moteur (lecteur muet).
    private func document(_ text: String) -> DocView {
        let m = ReaderModel(reader: SillonReader.silent())
        m.open(text: text, source: "test")
        XCTAssertTrue(waitUntil { m.refresh(); return m.snapshot.revision == 1 })
        return m.document
    }

    private func threeSentences() -> [UInt32] {
        doc = document("Un deux trois quatre. Cinq six. Sept huit neuf dix.")
        let sentences = doc.blocks.flatMap(\.sentences)
        XCTAssertEqual(sentences.map(\.words.count), [4, 2, 4], "découpage attendu par ces tests")
        return sentences.map(\.id)
    }

    func testPlayPauseAndSentences() {
        let t = transport()
        current = .sample(playing: false)
        XCTAssertTrue(t.perform(.toggle))
        XCTAssertTrue(t.perform(.play))
        XCTAssertTrue(t.perform(.pause), "déjà en pause : rien à envoyer, mais servie")
        XCTAssertTrue(t.perform(.next))
        XCTAssertTrue(t.perform(.previous))
        XCTAssertEqual(sent, [.togglePlay, .togglePlay, .sentences(delta: 1), .sentences(delta: -1)])
        sent = []
        current = .sample(playing: true)
        t.perform(.play)
        t.perform(.pause)
        XCTAssertEqual(sent, [.pause], "lecture déjà en cours : seule la pause part")
    }

    func testNothingIsSentWithoutADocument() {
        let t = transport()
        current = .sample(hasDocument: false)
        for action in [RemoteAction.play, .pause, .toggle, .next, .previous, .seek(10)] {
            XCTAssertFalse(t.perform(action), "\(action)")
        }
        XCTAssertEqual(sent, [])
    }

    /// La timeline vise la phrase dont le début est le plus proche (temps
    /// proportionnel aux mots, comme l'estimation du moteur).
    func testTheTimelineLandsOnTheNearestSentence() {
        let ids = threeSentences()
        let s = Snapshot.sample(total: 100)
        XCTAssertEqual(Transport.sentence(at: 0, snapshot: s, document: doc), ids[0])
        XCTAssertEqual(Transport.sentence(at: 35, snapshot: s, document: doc), ids[1])
        XCTAssertEqual(Transport.sentence(at: 55, snapshot: s, document: doc), ids[2])
    }

    func testBeyondTheEndIsTheLastSentenceAndBeforeTheStartTheFirst() {
        let ids = threeSentences()
        let s = Snapshot.sample(total: 100)
        XCTAssertEqual(Transport.sentence(at: 150, snapshot: s, document: doc), ids[2])
        XCTAssertEqual(Transport.sentence(at: 100, snapshot: s, document: doc), ids[2])
        XCTAssertEqual(Transport.sentence(at: -5, snapshot: s, document: doc), ids[0])
    }

    /// « À l'écoute » compte au rythme 1× : à 2×, 70 s visées = 35 % du texte.
    func testTheSpeedIsTakenIntoAccount() {
        let ids = threeSentences()
        XCTAssertEqual(Transport.sentence(at: 70, snapshot: .sample(total: 100, speed: 2), document: doc), ids[1])
    }

    func testWithoutAnEstimateTheTimelineDoesNothing() {
        _ = threeSentences()
        current = .sample(total: nil)
        XCTAssertNil(Transport.sentence(at: 10, snapshot: current, document: doc))
        XCTAssertFalse(transport().perform(.seek(10)))
        XCTAssertNil(Transport.sentence(at: .nan, snapshot: .sample(total: 100), document: doc))
        XCTAssertEqual(sent, [])
    }

    func testSeekSendsTheSentence() {
        let ids = threeSentences()
        current = .sample(total: 100)
        XCTAssertTrue(transport().perform(.seek(150)))
        XCTAssertEqual(sent, [.seekTo(sentence: ids[2])])
    }
}
```

Dans `macos/Tests/ReaderModelTests.swift`, ajouter à `ReaderModelTests` :

```swift
    /// Le crochet ne sonne qu'à un instantané publié qui change, pas à
    /// chaque relecture identique.
    func testOnChangeFollowsPublishedChangesOnly() {
        let m = model("Un. Deux. Trois.")
        var sentences: [UInt32] = []
        m.onChange = { s, _ in sentences.append(s.sentence) }
        // Une fois l'état posé (voix prête), deux relectures identiques ne
        // préviennent personne.
        XCTAssertTrue(waitUntil {
            m.refresh()
            let before = sentences.count
            m.refresh()
            return sentences.count == before
        })
        m.send(.sentences(delta: 1))
        XCTAssertTrue(waitUntil { m.refresh(); return m.snapshot.sentence == 1 })
        XCTAssertEqual(sentences.last, 1)
    }
```

et à `ReaderModelTickingTests` :

```swift
    /// « Mettre la voix au repos » : l'état est relu (grâce), même panneau
    /// masqué — « À l'écoute » et le menu le voient.
    func testRestingTheVoiceIsSeenWithoutWaitingForTheScreen() {
        let m = hiddenModel()
        XCTAssertFalse(m.isTicking)
        m.restVoice()
        XCTAssertTrue(m.isTicking, "la grâce relance le suivi")
        XCTAssertTrue(waitUntil { () -> Bool in
            self.frames.tick()
            if case .resting = m.snapshot.voice { return true }
            return false
        })
    }
```

- [ ] **Étape 2 : vérifier l'échec**

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests/TransportTests -only-testing:SillonTests/ReaderModelTests -only-testing:SillonTests/ReaderModelTickingTests`
Expected: FAIL — `cannot find 'Transport' in scope`, `value of type 'ReaderModel' has no member 'onChange'`.

- [ ] **Étape 3 : le crochet et le repos dans `ReaderModel`**

Dans `macos/Sources/Model/ReaderModel.swift`, sous `private(set) var breadcrumb = ""`,
ajouter :

```swift
    /// Prévenu quand l'instantané publié change (à la seconde, au millième
    /// d'avancement, à l'état) — jamais à chaque image : « À l'écoute » suit
    /// l'état sans minuterie à lui.
    @ObservationIgnored var onChange: (@MainActor (Snapshot, DocView) -> Void)?
```

remplacer `func refresh()` par :

```swift
    func refresh() {
        let next = Self.published(reader.snapshot())
        let changed = next != snapshot
        let newDocument = next.revision != snapshot.revision
        let newSection = next.section != snapshot.section
        // Ne publier qu'un changement réel : SwiftUI ne redessine pas pour rien.
        if changed {
            snapshot = next
        }
        if newDocument {
            document = reader.document()   // recalcule sommaire, repères et fil
        } else if newSection {
            breadcrumb = SectionMarkers.breadcrumb(document, section: snapshot.section)
        }
        if changed { onChange?(snapshot, document) }
        updateTicking()
    }
```

et `func restVoice() { reader.restVoice() }` par :

```swift
    /// Le repos se voit tout de suite (grâce), même fenêtre masquée.
    func restVoice() {
        reader.restVoice()
        wake()
    }
```

- [ ] **Étape 4 : écrire `Transport`**

`macos/Sources/App/Transport.swift` :

```swift
import Foundation

/// Ce que l'on demande à la lecture hors du panneau : touches média, AirPods,
/// Centre de contrôle, menu de la barre des menus, raccourcis globaux.
enum RemoteAction: Equatable {
    case play, pause, toggle, next, previous
    /// Un point de la timeline d'« À l'écoute », en secondes au rythme 1×.
    case seek(TimeInterval)
}

/// Traduit ces demandes en commandes du lecteur. Sans document, rien ne part
/// et la demande est dite non servie.
@MainActor
struct Transport {
    let snapshot: () -> Snapshot
    let document: () -> DocView
    let send: (ReaderCommand) -> Void

    init(snapshot: @escaping () -> Snapshot, document: @escaping () -> DocView,
         send: @escaping (ReaderCommand) -> Void) {
        self.snapshot = snapshot
        self.document = document
        self.send = send
    }

    /// Fenêtre masquée et lecture en pause, le modèle ne suit rien : l'état
    /// est relu à la demande.
    init(model: ReaderModel) {
        self.init(snapshot: { model.refresh(); return model.snapshot },
                  document: { model.document },
                  send: { model.send($0) })
    }

    /// Vrai si la demande a servi.
    @discardableResult
    func perform(_ action: RemoteAction) -> Bool {
        let s = snapshot()
        guard s.hasDocument else { return false }
        switch action {
        case .play:
            if !s.playing { send(.togglePlay) }
        case .pause:
            if s.playing { send(.pause) }
        case .toggle:
            send(.togglePlay)
        case .next:
            send(.sentences(delta: 1))
        case .previous:
            send(.sentences(delta: -1))
        case .seek(let position):
            guard let target = Self.sentence(at: position, snapshot: s, document: document()) else { return false }
            send(.seekTo(sentence: target))
        }
        return true
    }

    /// La phrase dont le début est le plus proche de `position`, en secondes
    /// au rythme 1× (durée estimée × vitesse, comme « À l'écoute » les
    /// compte). Le temps est proportionnel aux mots, comme l'estimation du
    /// moteur. Avant le début : la première ; à la fin ou au-delà : la
    /// dernière. Nil si la durée n'est pas encore connue ou le document vide.
    static func sentence(at position: TimeInterval, snapshot s: Snapshot, document: DocView) -> UInt32? {
        let sentences = document.blocks.flatMap(\.sentences)
        guard let first = sentences.first, let last = sentences.last,
              let total = s.total, total.isFinite, total > 0, position.isFinite else { return nil }
        let fraction = position / (total * Double(max(s.speed, 0.1)))
        if fraction >= 1 { return last.id }
        if fraction <= 0 { return first.id }
        let words = sentences.map(\.words.count)
        let target = fraction * Double(words.reduce(0, +))
        var start = 0
        var best = first.id
        var bestDistance = Double.infinity
        for (sentence, count) in zip(sentences, words) {
            let distance = abs(Double(start) - target)
            if distance < bestDistance {
                best = sentence.id
                bestDistance = distance
            }
            start += count
        }
        return best
    }
}
```

- [ ] **Étape 5 : vérifier que les tests passent**

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests`
Expected: `** TEST SUCCEEDED **`.

- [ ] **Étape 6 : commit**

```bash
git add macos/Sources/App/Transport.swift macos/Sources/Model/ReaderModel.swift macos/Tests/Samples.swift macos/Tests/TransportTests.swift macos/Tests/ReaderModelTests.swift
git commit -m "$(cat <<'EOF'
feat(macos): commandes de lecture hors du panneau et saut de timeline à la phrase la plus proche

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Tâche 5 : « À l'écoute » et les touches média

`NowPlaying` publie titre, durée, position, état et vitesse au changement
d'état, branche ⏯ ⏭ ⏮ et la timeline sur `Transport`, et retire tout quand
la voix se repose. Derrière deux protocoles : les tests passent un faux
centre et de fausses commandes. Créé seulement hors des tests.

**Convention des temps** (celle des apps de podcast) : durée et position au
rythme 1× (temps estimé × vitesse), débit = vitesse pendant la lecture, 0 en
pause. macOS avance la position seul : rien n'est republié à chaque seconde.

**Fichiers :**
- Créer : `macos/Sources/App/NowPlayingSystem.swift`
- Créer : `macos/Sources/App/NowPlaying.swift`
- Modifier : `macos/project.yml` (lien `MediaPlayer.framework`)
- Modifier : `macos/Sources/AppDelegate.swift` (`transport`, `nowPlaying`)
- Créer : `macos/Tests/NowPlayingTests.swift`

**Interfaces :**
- Consomme : `RemoteAction`, `Transport` (Tâche 4), `ReaderModel.onChange`, `ReaderModel.loadSettings()`, `SettingsView.keepVoiceMinutes: Int64`, `Snapshot.sample` (tests).
- Produit :
  - `protocol NowPlayingCenter: AnyObject { var nowPlayingInfo: [String: Any]? { get set }; var playbackState: MPNowPlayingPlaybackState { get set } }` ; `extension MPNowPlayingInfoCenter: NowPlayingCenter`
  - `protocol RemoteCommandSource: AnyObject { func install(_ handler: @escaping @MainActor (RemoteAction) -> Bool); func setEnabled(_ enabled: Bool) }` ; `final class SystemRemoteCommands: RemoteCommandSource`
  - `struct NowPlayingInfo: Equatable { title, duration, elapsed, speed, playing; static func from(_:document:) -> NowPlayingInfo?; var dictionary: [String: Any] }`
  - `@MainActor final class NowPlaying { typealias Schedule; init(center:remote:transport:restDelay:schedule:refresh:); func update(_ s: Snapshot, document: DocView); private(set) var publications: Int; static func restDelay(keepVoiceMinutes: Int64) -> TimeInterval?; static func onMainQueue(_:_:) }`
  - `AppDelegate.transport: Transport!`, `AppDelegate.nowPlaying: NowPlaying?`

- [ ] **Étape 1 : écrire les tests qui échouent**

`macos/Tests/NowPlayingTests.swift` :

```swift
import MediaPlayer
import XCTest
@testable import Sillon

final class FakeNowPlayingCenter: NowPlayingCenter {
    var nowPlayingInfo: [String: Any]? { didSet { writes += 1 } }
    var playbackState: MPNowPlayingPlaybackState = .unknown
    private(set) var writes = 0
}

final class FakeRemoteCommands: RemoteCommandSource {
    private(set) var handler: (@MainActor (RemoteAction) -> Bool)?
    private(set) var installs = 0
    private(set) var enabled = false
    func install(_ handler: @escaping @MainActor (RemoteAction) -> Bool) {
        installs += 1
        self.handler = handler
    }
    func setEnabled(_ enabled: Bool) { self.enabled = enabled }
}

@MainActor
final class NowPlayingTests: XCTestCase {
    private let center = FakeNowPlayingCenter()
    private let remote = FakeRemoteCommands()
    private let doc = DocView(title: "Les monades", complete: true, sentenceCount: 3, sections: [], blocks: [])
    private var current = Snapshot.sample()
    private var sent: [ReaderCommand] = []
    private var delays: [TimeInterval] = []
    private var cancels = 0
    private var pending: (@MainActor () -> Void)?
    private var refreshes = 0

    private func make(restDelay: TimeInterval? = 602) -> NowPlaying {
        let transport = Transport(snapshot: { [unowned self] in self.current },
                                  document: { [unowned self] in self.doc },
                                  send: { [unowned self] in self.sent.append($0) })
        return NowPlaying(center: center, remote: remote, transport: transport,
                          restDelay: { restDelay },
                          schedule: { [unowned self] delay, work in
                              self.delays.append(delay)
                              self.pending = work
                              return { self.cancels += 1 }
                          },
                          refresh: { [unowned self] in self.refreshes += 1 })
    }

    func testPublishesTitleDurationPositionStateAndSpeed() throws {
        let now = make()
        now.update(.sample(playing: true, elapsed: 56, total: 176, speed: 1.5), document: doc)
        let info = try XCTUnwrap(center.nowPlayingInfo)
        XCTAssertEqual(info[MPMediaItemPropertyTitle] as? String, "Les monades")
        XCTAssertEqual(info[MPMediaItemPropertyPlaybackDuration] as? Double, 264, "176 s × 1,5")
        XCTAssertEqual(info[MPNowPlayingInfoPropertyElapsedPlaybackTime] as? Double, 84, "56 s × 1,5")
        XCTAssertEqual(info[MPNowPlayingInfoPropertyPlaybackRate] as? Double, 1.5)
        XCTAssertEqual(center.playbackState, .playing)
        XCTAssertTrue(remote.enabled)
        XCTAssertEqual(remote.installs, 1)
    }

    /// Une durée encore inconnue n'est pas inventée.
    func testAnUnknownDurationIsLeftOut() throws {
        let now = make()
        now.update(.sample(elapsed: nil, total: nil), document: doc)
        let info = try XCTUnwrap(center.nowPlayingInfo)
        XCTAssertNil(info[MPMediaItemPropertyPlaybackDuration])
        XCTAssertNil(info[MPNowPlayingInfoPropertyElapsedPlaybackTime])
    }

    /// Le temps qui passe seul ne republie rien : macOS avance la position.
    func testTimePassingAloneRepublishesNothing() {
        let now = make()
        for second in 56...80 {
            now.update(.sample(playing: true, elapsed: Double(second)), document: doc)
        }
        XCTAssertEqual(now.publications, 1)
        now.update(.sample(playing: true, sentence: 1, elapsed: 81), document: doc)
        XCTAssertEqual(now.publications, 2, "nouvelle phrase : position recalée")
        now.update(.sample(playing: false, sentence: 1, elapsed: 81), document: doc)
        XCTAssertEqual(now.publications, 3)
        XCTAssertEqual(center.playbackState, .paused)
        XCTAssertEqual(center.nowPlayingInfo?[MPNowPlayingInfoPropertyPlaybackRate] as? Double, 0)
        now.update(.sample(playing: false, sentence: 1, elapsed: 81, speed: 1.2), document: doc)
        XCTAssertEqual(now.publications, 4, "vitesse changée")
    }

    func testNothingIsShownWithoutADocument() {
        let now = make()
        now.update(.sample(hasDocument: false), document: doc)
        XCTAssertNil(center.nowPlayingInfo)
        XCTAssertEqual(center.writes, 0)
        XCTAssertEqual(remote.installs, 0)
        XCTAssertFalse(remote.enabled)
    }

    func testTheInfoGoesAwayWhenTheVoiceRests() {
        let now = make()
        now.update(.sample(), document: doc)
        XCTAssertNotNil(center.nowPlayingInfo)
        now.update(.sample(voice: .resting), document: doc)
        XCTAssertNil(center.nowPlayingInfo)
        XCTAssertEqual(center.playbackState, .stopped)
        XCTAssertFalse(remote.enabled)
        let writes = center.writes
        now.update(.sample(voice: .resting), document: doc)
        XCTAssertEqual(center.writes, writes, "rien à retirer deux fois")
    }

    func testMediaCommandsDriveTheReader() throws {
        let now = make()
        current = .sample(playing: false)
        now.update(current, document: doc)
        let handler = try XCTUnwrap(remote.handler)
        XCTAssertTrue(handler(.toggle))
        XCTAssertTrue(handler(.play))
        XCTAssertTrue(handler(.next))
        XCTAssertTrue(handler(.previous))
        XCTAssertEqual(sent, [.togglePlay, .togglePlay, .sentences(delta: 1), .sentences(delta: -1)])
        current = .sample(hasDocument: false)
        XCTAssertFalse(handler(.toggle), "sans document : ignorée")
        XCTAssertEqual(sent.count, 4)
        // Branchées une seule fois, quel que soit le nombre de publications.
        now.update(.sample(sentence: 2), document: doc)
        XCTAssertEqual(remote.installs, 1)
    }

    /// Une pause prévoit une seule relecture, au moment où le moteur met la
    /// voix au repos ; la lecture qui reprend l'annule.
    func testAPauseWaitsOnceForTheVoiceToRestAndPlayingCancelsIt() async {
        let now = make(restDelay: 602)
        now.update(.sample(playing: true), document: doc)
        try? await Task.sleep(for: .milliseconds(50))
        XCTAssertEqual(delays, [], "rien ne se prévoit pendant la lecture")
        now.update(.sample(playing: false), document: doc)
        let armed = await eventuallyTrue { self.delays == [602] }
        XCTAssertTrue(armed)
        pending?()
        XCTAssertEqual(refreshes, 1, "une seule relecture")
        now.update(.sample(playing: true, sentence: 1), document: doc)
        XCTAssertEqual(cancels, 1)
    }

    func testAVoiceKeptForeverSchedulesNothing() async {
        let now = make(restDelay: nil)
        now.update(.sample(playing: false), document: doc)
        try? await Task.sleep(for: .milliseconds(50))
        XCTAssertEqual(delays, [])
    }

    func testRestDelayFollowsKeepTheVoiceLoaded() {
        XCTAssertNil(NowPlaying.restDelay(keepVoiceMinutes: -1), "toujours chargée")
        XCTAssertEqual(NowPlaying.restDelay(keepVoiceMinutes: 0), 2)
        XCTAssertEqual(NowPlaying.restDelay(keepVoiceMinutes: 10), 602)
    }
}
```

- [ ] **Étape 2 : vérifier l'échec**

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests/NowPlayingTests`
Expected: FAIL — `cannot find type 'NowPlayingCenter' in scope`, `cannot find 'NowPlaying' in scope`.

- [ ] **Étape 3 : lier MediaPlayer**

Dans `macos/project.yml`, cible `Sillon`, remplacer :

```yaml
    dependencies:
      - package: KeyboardShortcuts
```

par :

```yaml
    dependencies:
      - package: KeyboardShortcuts
      - sdk: MediaPlayer.framework
```

- [ ] **Étape 4 : les deux protocoles et leur version système**

`macos/Sources/App/NowPlayingSystem.swift` :

```swift
import Foundation
import MediaPlayer

/// Le centre « À l'écoute », derrière un protocole : les tests passent un faux.
protocol NowPlayingCenter: AnyObject {
    var nowPlayingInfo: [String: Any]? { get set }
    var playbackState: MPNowPlayingPlaybackState { get set }
}

extension MPNowPlayingInfoCenter: NowPlayingCenter {}

/// Les commandes média (clavier, AirPods, Centre de contrôle).
protocol RemoteCommandSource: AnyObject {
    /// Branche les commandes ; `handler` rend vrai si la commande a servi.
    func install(_ handler: @escaping @MainActor (RemoteAction) -> Bool)
    func setEnabled(_ enabled: Bool)
}

/// Les commandes du système, branchées sur `MPRemoteCommandCenter`.
final class SystemRemoteCommands: RemoteCommandSource {
    private let center = MPRemoteCommandCenter.shared()

    private var commands: [MPRemoteCommand] {
        [center.playCommand, center.pauseCommand, center.togglePlayPauseCommand,
         center.nextTrackCommand, center.previousTrackCommand, center.changePlaybackPositionCommand]
    }

    func install(_ handler: @escaping @MainActor (RemoteAction) -> Bool) {
        let pairs: [(MPRemoteCommand, RemoteAction)] = [
            (center.playCommand, .play), (center.pauseCommand, .pause),
            (center.togglePlayPauseCommand, .toggle),
            (center.nextTrackCommand, .next), (center.previousTrackCommand, .previous),
        ]
        for (command, action) in pairs {
            command.addTarget { _ in
                Self.onMain { handler(action) } ? .success : .noActionableNowPlayingItem
            }
        }
        center.changePlaybackPositionCommand.addTarget { event in
            guard let event = event as? MPChangePlaybackPositionCommandEvent else { return .commandFailed }
            let position = event.positionTime
            return Self.onMain { handler(.seek(position)) } ? .success : .commandFailed
        }
    }

    func setEnabled(_ enabled: Bool) {
        for command in commands { command.isEnabled = enabled }
    }

    /// macOS appelle les commandes sur le fil principal ; sinon on l'y attend.
    private static func onMain(_ body: @MainActor () -> Bool) -> Bool {
        if Thread.isMainThread { return MainActor.assumeIsolated { body() } }
        return DispatchQueue.main.sync { MainActor.assumeIsolated { body() } }
    }
}
```

- [ ] **Étape 5 : `NowPlaying`**

`macos/Sources/App/NowPlaying.swift` :

```swift
import Foundation
import MediaPlayer

/// Ce que « À l'écoute » montre. Durée et position au rythme 1× (temps
/// estimé × vitesse), la vitesse passe par le débit : macOS fait avancer la
/// position seul, sans qu'on la republie chaque seconde.
struct NowPlayingInfo: Equatable {
    var title: String
    var duration: Double?
    var elapsed: Double?
    var speed: Double
    var playing: Bool

    /// Rien sans document, ni quand la voix est au repos.
    static func from(_ s: Snapshot, document: DocView) -> NowPlayingInfo? {
        guard s.hasDocument else { return nil }
        if case .resting = s.voice, !s.playing { return nil }
        let speed = Double(s.speed)
        func content(_ t: Double?) -> Double? { t.flatMap { $0.isFinite ? $0 * speed : nil } }
        return NowPlayingInfo(title: document.title, duration: content(s.total),
                              elapsed: content(s.elapsed), speed: speed, playing: s.playing)
    }

    var dictionary: [String: Any] {
        var d: [String: Any] = [
            MPMediaItemPropertyTitle: title,
            MPNowPlayingInfoPropertyPlaybackRate: playing ? speed : 0,
            MPNowPlayingInfoPropertyDefaultPlaybackRate: speed,
            MPNowPlayingInfoPropertyMediaType: MPNowPlayingInfoMediaType.audio.rawValue,
        ]
        if let duration { d[MPMediaItemPropertyPlaybackDuration] = duration }
        if let elapsed { d[MPNowPlayingInfoPropertyElapsedPlaybackTime] = elapsed }
        return d
    }
}

/// Publie l'état de la lecture à « À l'écoute » et y branche les commandes
/// média. Appelé à chaque instantané publié (`ReaderModel.onChange`), il
/// n'écrit qu'aux changements d'état : lecture, pause, phrase, vitesse,
/// durée, document — jamais pour le seul temps qui passe.
@MainActor
final class NowPlaying {
    typealias Schedule = @MainActor (TimeInterval, @escaping @MainActor () -> Void) -> @MainActor () -> Void

    private let center: NowPlayingCenter
    private let remote: RemoteCommandSource
    private let transport: Transport
    private let restDelay: @MainActor () async -> TimeInterval?
    private let schedule: Schedule
    private let refresh: @MainActor () -> Void
    /// Ce qui a été publié, sans la position.
    private var last: Key?
    private var installed = false
    private var cancelRestCheck: (@MainActor () -> Void)?
    private var restGeneration = 0
    /// Une par changement d'état : les tests le comptent.
    private(set) var publications = 0

    private struct Key: Equatable {
        var title: String
        var duration: Double?
        var speed: Double
        var playing: Bool
        var sentence: UInt32
        var revision: UInt64
    }

    init(center: NowPlayingCenter, remote: RemoteCommandSource, transport: Transport,
         restDelay: @escaping @MainActor () async -> TimeInterval?,
         schedule: @escaping Schedule = NowPlaying.onMainQueue,
         refresh: @escaping @MainActor () -> Void) {
        self.center = center
        self.remote = remote
        self.transport = transport
        self.restDelay = restDelay
        self.schedule = schedule
        self.refresh = refresh
    }

    func update(_ s: Snapshot, document: DocView) {
        guard let info = NowPlayingInfo.from(s, document: document) else {
            clear()
            return
        }
        let key = Key(title: info.title, duration: info.duration, speed: info.speed,
                      playing: info.playing, sentence: s.sentence, revision: s.revision)
        guard key != last else { return }
        last = key
        if !installed {
            installed = true
            let transport = transport
            remote.install { action in transport.perform(action) }
        }
        remote.setEnabled(true)
        center.nowPlayingInfo = info.dictionary
        center.playbackState = info.playing ? .playing : .paused
        publications += 1
        if info.playing { stopRestCheck() } else { armRestCheck() }
    }

    private func clear() {
        stopRestCheck()
        guard last != nil else { return }
        last = nil
        center.nowPlayingInfo = nil
        center.playbackState = .stopped
        remote.setEnabled(false)
    }

    /// En pause, une seule relecture différée, au moment où le moteur met la
    /// voix au repos : elle verra `.resting` et retirera l'affichage. Pas de
    /// minuterie qui revient.
    private func armRestCheck() {
        stopRestCheck()
        let generation = restGeneration
        Task { @MainActor [weak self] in
            guard let self, let delay = await self.restDelay(),
                  generation == self.restGeneration else { return }
            let refresh = self.refresh
            self.cancelRestCheck = self.schedule(delay) { refresh() }
        }
    }

    private func stopRestCheck() {
        restGeneration &+= 1
        cancelRestCheck?()
        cancelRestCheck = nil
    }

    /// Le moment de relire l'état après une pause : la durée « Garder la
    /// voix chargée », plus 2 s de marge. Nil : la voix reste chargée.
    nonisolated static func restDelay(keepVoiceMinutes: Int64) -> TimeInterval? {
        keepVoiceMinutes < 0 ? nil : TimeInterval(keepVoiceMinutes) * 60 + 2
    }

    static func onMainQueue(_ delay: TimeInterval,
                            _ work: @escaping @MainActor () -> Void) -> @MainActor () -> Void {
        let item = DispatchWorkItem { MainActor.assumeIsolated { work() } }
        DispatchQueue.main.asyncAfter(deadline: .now() + delay, execute: item)
        return { item.cancel() }
    }
}
```

- [ ] **Étape 6 : brancher dans `AppDelegate`**

Dans `macos/Sources/AppDelegate.swift`, ajouter `import MediaPlayer` sous
`import KeyboardShortcuts`, puis sous `private(set) var panel: PanelController!` :

```swift
    /// Lecture, pause, phrases et timeline venues d'ailleurs que du panneau.
    private(set) var transport: Transport!
    /// « À l'écoute » et les touches média ; jamais avec le lecteur muet des
    /// tests (il prendrait les touches média de l'utilisateur).
    private(set) var nowPlaying: NowPlaying?
```

Remplacer :

```swift
        panel = PanelController(model: model)
        // Le mode laissé à la dernière fermeture (sous les tests : suite
```

par :

```swift
        panel = PanelController(model: model)
        transport = Transport(model: model)
        if !readerIsSilent { installNowPlaying() }
        // Le mode laissé à la dernière fermeture (sous les tests : suite
```

et ajouter, avant `private func installShortcuts()` :

```swift
    private func installNowPlaying() {
        let model = model!
        let nowPlaying = NowPlaying(
            center: MPNowPlayingInfoCenter.default(),
            remote: SystemRemoteCommands(),
            transport: transport,
            restDelay: { NowPlaying.restDelay(keepVoiceMinutes: await model.loadSettings().keepVoiceMinutes) },
            refresh: { model.refresh() })
        model.onChange = { snapshot, document in nowPlaying.update(snapshot, document: document) }
        self.nowPlaying = nowPlaying
    }
```

- [ ] **Étape 7 : vérifier que les tests passent**

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests`
Expected: `** TEST SUCCEEDED **`.

- [ ] **Étape 8 : commit**

```bash
git add macos/project.yml macos/Sources/App/NowPlayingSystem.swift macos/Sources/App/NowPlaying.swift macos/Sources/AppDelegate.swift macos/Tests/NowPlayingTests.swift
git commit -m "$(cat <<'EOF'
feat(macos): « À l'écoute » et touches média — publié au changement d'état, retiré au repos de la voix

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Tâche 6 : menu de la barre des menus, raccourcis globaux, aide

Le menu gagne lecture/pause, phrase précédente/suivante, le temps restant et
le sous-menu « Afficher » (⌘1, ⌘2, ⌘3) ; trois raccourcis globaux (⌃⌥P,
⌃⌥→, ⌃⌥←) modifiables dans l'onglet Raccourcis ; l'aide `?` liste ⌘1 ⌘2 ⌘3.

**Fichiers :**
- Modifier : `macos/Sources/App/Shortcuts.swift`
- Modifier : `macos/Sources/AppDelegate.swift` (`installShortcuts`)
- Modifier : `macos/Sources/MenuContent.swift` (réécrit en entier)
- Modifier : `macos/Sources/Settings/ShortcutsTab.swift`
- Modifier : `macos/Sources/Panel/PanelContent.swift` (`HelpOverlay.rows`)
- Créer : `macos/Tests/HelpOverlayTests.swift`

**Interfaces :**
- Consomme : `AppDelegate.{transport, panel, model}`, `Transport.perform(_:)`, `PanelController.{show(), apply(_:)}`, `PlaybackTime.menuLine(elapsed:total:)`.
- Produit : `KeyboardShortcuts.Name.{playPause, nextSentence, previousSentence}`.

- [ ] **Étape 1 : écrire le test qui échoue**

`macos/Tests/HelpOverlayTests.swift` :

```swift
import XCTest
@testable import Sillon

@MainActor
final class HelpOverlayTests: XCTestCase {
    func testTheHelpListsTheDisplayModes() {
        XCTAssertTrue(HelpOverlay.rows.contains { $0 == ("⌘1 ⌘2 ⌘3", "Petit · moyen · grand") })
        XCTAssertTrue(HelpOverlay.rows.contains { $0 == ("Échap", "Revenir · arrière-plan") })
        XCTAssertFalse(HelpOverlay.rows.contains { $0.1 == "Revenir · masquer" })
    }
}
```

- [ ] **Étape 2 : vérifier l'échec**

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests/HelpOverlayTests`
Expected: FAIL — `XCTAssertTrue failed` (la ligne ⌘1 ⌘2 ⌘3 manque).

- [ ] **Étape 3 : l'aide**

Dans `macos/Sources/Panel/PanelContent.swift`, `HelpOverlay.rows`, remplacer :

```swift
        ("⏎", "Agrandir"), ("⌘C", "Copier la phrase"), ("⌘,", "Réglages"),
        ("v", "Réglages de la voix"),
        ("Échap", "Revenir · masquer"),
```

par :

```swift
        ("⏎", "Agrandir"), ("⌘1 ⌘2 ⌘3", "Petit · moyen · grand"),
        ("⌘C", "Copier la phrase"), ("⌘,", "Réglages"),
        ("v", "Réglages de la voix"),
        ("Échap", "Revenir · arrière-plan"),
```

- [ ] **Étape 4 : les raccourcis globaux**

Dans `macos/Sources/App/Shortcuts.swift`, compléter l'extension :

```swift
extension KeyboardShortcuts.Name {
    static let readSelection = Self("readSelection", default: .init(.space, modifiers: [.control, .option]))
    static let showPanel = Self("showPanel", default: .init(.s, modifiers: [.control, .option]))
    static let playPause = Self("playPause", default: .init(.p, modifiers: [.control, .option]))
    static let nextSentence = Self("nextSentence", default: .init(.rightArrow, modifiers: [.control, .option]))
    static let previousSentence = Self("previousSentence", default: .init(.leftArrow, modifiers: [.control, .option]))
}
```

Dans `macos/Sources/AppDelegate.swift`, à la fin de `installShortcuts()`, après
la ligne `.showPanel`, ajouter :

```swift
        // Sans document, rien ne part (Transport).
        KeyboardShortcuts.onKeyUp(for: .playPause) { [unowned self] in _ = self.transport.perform(.toggle) }
        KeyboardShortcuts.onKeyUp(for: .nextSentence) { [unowned self] in _ = self.transport.perform(.next) }
        KeyboardShortcuts.onKeyUp(for: .previousSentence) { [unowned self] in _ = self.transport.perform(.previous) }
```

`macos/Sources/Settings/ShortcutsTab.swift`, en entier :

```swift
import KeyboardShortcuts
import SwiftUI

/// Onglet Raccourcis : les raccourcis globaux. Un raccourci déjà pris est
/// signalé par l'enregistreur lui-même.
struct ShortcutsTab: View {
    var body: some View {
        Form {
            KeyboardShortcuts.Recorder("Lire la sélection :", name: .readSelection)
            KeyboardShortcuts.Recorder("Afficher sillon :", name: .showPanel)
            KeyboardShortcuts.Recorder("Lecture / pause :", name: .playPause)
            KeyboardShortcuts.Recorder("Phrase suivante :", name: .nextSentence)
            KeyboardShortcuts.Recorder("Phrase précédente :", name: .previousSentence)
        }
        .formStyle(.grouped)
        .accessibilityIdentifier("onglet-raccourcis")
    }
}
```

- [ ] **Étape 5 : le menu de la barre des menus**

`macos/Sources/MenuContent.swift`, en entier :

```swift
import KeyboardShortcuts
import SwiftUI

struct MenuContent: View {
    private var app: AppDelegate? { AppDelegate.shared }
    private var model: ReaderModel? { AppDelegate.shared?.model }

    var body: some View {
        let s = model?.snapshot
        let hasDocument = s?.hasDocument ?? false
        // Un clic dans le menu a déjà pris le focus à l'application : seule
        // la copie déjà faite est atteignable d'ici.
        Button("Lire le presse-papiers") {
            Task { @MainActor in await AppDelegate.shared?.read() }
        }
        // Toujours là, même relancée en arrière-plan : de quoi revenir.
        Button("Afficher sillon") { app?.panel.show() }
        Menu("Afficher") {
            Button("Petit") { app?.panel.apply(.choose(.small)) }
                .keyboardShortcut("1", modifiers: .command)
            Button("Moyen") { app?.panel.apply(.choose(.medium)) }
                .keyboardShortcut("2", modifiers: .command)
            Button("Grand") { app?.panel.apply(.choose(.large)) }
                .keyboardShortcut("3", modifiers: .command)
        }
        Divider()
        Button(s?.playing == true ? "Pause" : "Lecture") { _ = app?.transport.perform(.toggle) }
            .globalKeyboardShortcut(.playPause)
            .disabled(!hasDocument)
        Button("Phrase précédente") { _ = app?.transport.perform(.previous) }
            .globalKeyboardShortcut(.previousSentence)
            .disabled(!hasDocument)
        Button("Phrase suivante") { _ = app?.transport.perform(.next) }
            .globalKeyboardShortcut(.nextSentence)
            .disabled(!hasDocument)
        if hasDocument, let line = PlaybackTime.menuLine(elapsed: s?.elapsed, total: s?.total) {
            Text(line)
        }
        Divider()
        Text(voiceLine)
        if Onboarding.needsReminder {
            Button("Autoriser la lecture de la sélection…") {
                Onboarding.reminded()
                let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")!
                NSWorkspace.shared.open(url)
            }
        }
        Button("Mettre la voix au repos") { model?.restVoice() }
        Divider()
        SettingsLink { Text("Réglages…") }
            .keyboardShortcut(",")
        Button("Quitter sillon") { NSApp.terminate(nil) }
            .keyboardShortcut("q")
    }

    private var voiceLine: String {
        switch model?.snapshot.voice {
        case .ready: "Voix prête"
        case .loading: "Voix en chargement…"
        case .fallback(let reason): "Voix du système (\(reason))"
        default: "Voix au repos"
        }
    }
}
```

- [ ] **Étape 6 : vérifier que les tests passent**

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test -only-testing:SillonTests`
Expected: `** TEST SUCCEEDED **`.

- [ ] **Étape 7 : commit**

```bash
git add macos/Sources/App/Shortcuts.swift macos/Sources/AppDelegate.swift macos/Sources/MenuContent.swift macos/Sources/Settings/ShortcutsTab.swift macos/Sources/Panel/PanelContent.swift macos/Tests/HelpOverlayTests.swift
git commit -m "$(cat <<'EOF'
feat(macos): menu de la barre des menus, ⌃⌥P ⌃⌥→ ⌃⌥← globaux, aide ⌘1 ⌘2 ⌘3

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Tâche 7 : tests d'interface des modes et audit du mode petit

⌘1 montre la barre, ⌘2 le panneau, ⌘3 le lecteur agrandi, Échap revient puis
masque ; le mode petit passe l'audit d'accessibilité. Écrits et compilés ici ;
**leur exécution attend le feu vert de l'utilisateur** (étape 4). Le lecteur
des tests est muet et hors disque ; les préférences d'affichage sont une suite
isolée, vidée à chaque lancement.

**Fichiers :**
- Créer : `macos/UITests/DisplayModeUITests.swift`

**Interfaces :**
- Consomme : identifiants `panneau-lecture`, `lecteur-reduit`, `lecteur-agrandi` (Tâche 3), `lecture`, `reglages-voix` (Tâche 2), `waitForLabel` (`PanelUITests.swift`), `performAccessibilityAuditOutsideTouchBar()` (`AuditSupport.swift`).
- Produit : aucune interface.

- [ ] **Étape 1 : écrire les tests**

`macos/UITests/DisplayModeUITests.swift` :

```swift
import XCTest

/// Les quatre modes au clavier et l'audit du mode petit. Lancé à la main,
/// avec l'accord de l'utilisateur :
/// `-only-testing:SillonUITests/DisplayModeUITests`.
final class DisplayModeUITests: XCTestCase {
    private func launch() -> XCUIApplication {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--show-panel"]
        app.launch()
        XCTAssertTrue(app.groups["panneau-lecture"].waitForExistence(timeout: 5))
        return app
    }

    func testCommandDigitsSwitchModesAndEscapeHides() {
        let app = launch()
        app.typeKey("1", modifierFlags: .command)
        let small = app.groups["lecteur-reduit"]
        XCTAssertTrue(small.waitForExistence(timeout: 2), "⌘1 : pas de barre")
        XCTAssertTrue(small.buttons["lecture"].exists)
        XCTAssertTrue(small.buttons["reglages-voix"].exists)
        // Les touches de lecture restent actives dans la barre.
        app.typeKey(" ", modifierFlags: [])
        XCTAssertTrue(app.buttons["lecture"].waitForLabel("Pause"))
        app.typeKey(" ", modifierFlags: [])
        XCTAssertTrue(app.buttons["lecture"].waitForLabel("Lecture"))

        app.typeKey("2", modifierFlags: .command)
        XCTAssertTrue(app.groups["panneau-lecture"].waitForExistence(timeout: 2), "⌘2 : pas de panneau")
        app.typeKey("3", modifierFlags: .command)
        XCTAssertTrue(app.groups["lecteur-agrandi"].waitForExistence(timeout: 2), "⌘3 : pas de lecteur agrandi")

        app.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(app.groups["panneau-lecture"].waitForExistence(timeout: 2), "Échap : d'abord le moyen")
        app.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(app.groups["panneau-lecture"].waitForNonExistence(timeout: 2), "Échap : puis masqué")
    }

    func testEscapeFromTheSmallBarHides() {
        let app = launch()
        app.typeKey("1", modifierFlags: .command)
        let small = app.groups["lecteur-reduit"]
        XCTAssertTrue(small.waitForExistence(timeout: 2))
        app.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(small.waitForNonExistence(timeout: 2))
    }

    func testTheSmallBarPassesTheAudit() throws {
        let app = launch()
        app.typeKey("1", modifierFlags: .command)
        XCTAssertTrue(app.groups["lecteur-reduit"].waitForExistence(timeout: 2))
        try app.performAccessibilityAuditOutsideTouchBar()
    }
}
```

- [ ] **Étape 2 : vérifier que les tests compilent**

Run: `cd macos && xcodegen generate && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' build-for-testing`
Expected: `** TEST BUILD SUCCEEDED **`.

- [ ] **Étape 3 : commit**

```bash
git add macos/UITests/DisplayModeUITests.swift
git commit -m "$(cat <<'EOF'
test(macos): ⌘1 ⌘2 ⌘3 et Échap au clavier, audit d'accessibilité du mode petit

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

- [ ] **Étape 4 : exécuter les tests d'interface — seulement avec le feu vert de l'utilisateur**

Ne pas lancer sans accord. Demander d'abord : « Les tests d'interface
prennent la main sur le clavier et la souris pendant une à deux minutes. Je
les lance ? » Sans accord explicite, s'arrêter ici et le signaler dans le
compte rendu.

Run: `cd macos && xcodebuild -project Sillon.xcodeproj -scheme Sillon -destination 'platform=macOS,arch=arm64' test-without-building -only-testing:SillonUITests/DisplayModeUITests -only-testing:SillonUITests/PanelUITests -only-testing:SillonUITests/ExpandedUITests`
Expected: `** TEST SUCCEEDED **`. Un échec d'audit se corrige dans
`MiniPlayerView` (libellé, valeur, contraste), dans un commit à part
`fix(macos): …`, puis on relance cette étape.

---

## Couverture de la spec

| Exigence de la spec | Tâche |
|---|---|
| Petit ⌘1 (≈ 360 × 44 : lecture/pause, timeline, temps restant, vitesse, ⚙) | 2, 3 |
| Moyen ⌘2, Grand ⌘3 (⏎ y mène toujours), Arrière-plan Échap, lecture continue | 3 |
| Une seule fenêtre, mode petit déplaçable, place retenue, flotte comme le panneau | 1, 3 |
| ⌃⌥S → dernier mode visible ; mode et dernier mode retenus après relance | 1, 3 |
| Touches de lecture actives dans le mode petit, ↑ ↓ suivent le texte invisible | 3 |
| ⌃⌥Espace garde le mode ; depuis l'arrière-plan, ouvre le moyen | 1, 3 |
| Échap en deux temps depuis le grand mode | 1, 3 |
| Aide `?` et menu listent ⌘1, ⌘2, ⌘3 | 6 |
| Mode petit : 13 pt, libellés VoiceOver, timeline « 0:56 sur 2:56 », « Rien à lire » + ⌃⌥Espace, coût au repos nul | 2 |
| « À l'écoute » : nom, durée, position, état, vitesse ; ⏯ ⏭ ⏮ ; timeline → phrase la plus proche | 4, 5 |
| Menu de la barre des menus : lecture/pause, phrases, temps restant, « Afficher » | 6 |
| Raccourcis globaux ⌃⌥P, ⌃⌥→, ⌃⌥←, modifiables | 6 |
| Au repos : « À l'écoute » retiré, aucune minuterie en continu | 4, 5 |
| `DisplayMode`, `DisplayModeStore`, `PanelController`, `MiniPlayerView`, `NowPlaying`, `KeyMap`, raccourcis | 1–6 |
| Erreurs : raccourci pris (enregistreur), commande sans document, saut au-delà de la fin, place hors écran | 6, 4–5, 4, 1 + 3 |
| Tests unitaires et tests d'interface (compilés, exécutés avec feu vert) | 1–7 |
| Hors périmètre respecté : pas de pochette, barre non redimensionnable | 3, 5 |

Décisions prises là où la spec se tait :

- **Échap et ⌘W ne mettent plus en pause** : l'arrière-plan garde la lecture
  (la spec le dit d'Échap ; ⌘W suit). ⌘W va droit à l'arrière-plan, même
  depuis le grand mode (Tâches 1, 3). Les tests existants qui attendaient
  la pause sont réécrits.
- **Relance** : le mode visible retenu reparaît au lancement, **sans prendre
  le clavier** à l'app au premier plan (`orderFrontRegardless`) ; retenu en
  arrière-plan, rien ne s'affiche et le menu garde « Afficher sillon » et
  « Afficher » (Tâches 3, 6).
- **⏎, `/`, `B` depuis la barre** mènent au grand mode ; **`?`** ouvre le
  moyen avec l'aide (la barre n'a pas la place) (Tâche 3).
- **↑ ↓ dans la barre** : une copie cachée de la vue de lecture, à la
  largeur du mode moyen, sans trait animé ni VoiceOver (Tâche 3).
- **Temps d'« À l'écoute » au rythme 1×**, débit = vitesse (convention des
  apps de podcast) : la vitesse se voit et macOS avance la position seul ;
  la barre et le menu montrent, eux, le temps réel estimé (Tâche 5).
- **Retrait au repos** : le moteur met la voix au repos sur son propre fil ;
  Swift ne relit rien en pause. Une seule relecture différée par pause
  (durée « Garder la voix chargée » + 2 s, annulée à la reprise), et
  « Mettre la voix au repos » relit tout de suite l'état (Tâches 4, 5).
- **Touches média et « À l'écoute » absents sous les tests** (lecteur muet),
  pour ne pas prendre les touches média de l'utilisateur (Tâche 5).
- **Commandes média activées seulement pendant une publication**, branchées
  à la première (Tâche 5).
- **Aucun changement Rust.**
