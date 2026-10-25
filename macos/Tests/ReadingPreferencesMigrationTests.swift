import XCTest
@testable import Dysflow

/// Police et taille : des préférences vers config.toml, une fois.
@MainActor
final class ReadingPreferencesMigrationTests: XCTestCase {
    private func freshDefaults() -> UserDefaults {
        let name = "dysflow.tests.reprise-lecture"
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
