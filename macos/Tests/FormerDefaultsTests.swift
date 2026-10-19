import XCTest
@testable import Sillage

final class FormerDefaultsTests: XCTestCase {
    private var defaults: UserDefaults!
    private let suiteName = "fr.sillage.tests.reprise"

    override func setUp() {
        defaults = UserDefaults(suiteName: suiteName)
        defaults.removePersistentDomain(forName: suiteName)
    }

    override func tearDown() {
        defaults.removePersistentDomain(forName: suiteName)
    }

    func testLesReglagesDeSillonSontRepris() {
        FormerDefaults.adopt(into: defaults, former: ["dictationModel": "phonon_2"], environment: [:])
        XCTAssertEqual(defaults.string(forKey: "dictationModel"), "phonon_2")
        XCTAssertTrue(defaults.bool(forKey: FormerDefaults.doneKey))
    }

    func testUneCleDejaPoseeNestPasEcrasee() {
        defaults.set("parakeet_tdt_v3", forKey: "dictationModel")
        FormerDefaults.adopt(into: defaults, former: ["dictationModel": "phonon_2"], environment: [:])
        XCTAssertEqual(defaults.string(forKey: "dictationModel"), "parakeet_tdt_v3")
    }

    func testLaRepriseNeSeFaitQuUneFois() {
        FormerDefaults.adopt(into: defaults, former: [:], environment: [:])
        FormerDefaults.adopt(into: defaults, former: ["dictationModel": "phonon_2"], environment: [:])
        XCTAssertNil(defaults.string(forKey: "dictationModel"))
    }

    func testRienSousLesTests() {
        FormerDefaults.adopt(into: defaults, former: ["dictationModel": "phonon_2"],
                             environment: ["XCTestConfigurationFilePath": "/x"])
        XCTAssertNil(defaults.string(forKey: "dictationModel"))
        XCTAssertFalse(defaults.bool(forKey: FormerDefaults.doneKey))
    }
}
