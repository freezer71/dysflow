import XCTest
@testable import Sillage

final class AudioOutputTests: XCTestCase {
    private let outputs = AudioOutputsView(devices: ["Haut-parleurs MacBook Pro", "AirPods Pro"],
                                           system: "AirPods Pro")

    func testTheSystemComesFirstThenEachOutput() {
        let choices = AudioOutputChoices.choices(outputs, chosen: "")
        XCTAssertEqual(choices.map(\.title),
                       ["Sortie du système (AirPods Pro)", "Haut-parleurs MacBook Pro", "AirPods Pro"])
        XCTAssertEqual(choices.map(\.value), ["", "Haut-parleurs MacBook Pro", "AirPods Pro"])
    }

    /// Les écouteurs choisis puis retirés restent dans la liste : le choix
    /// tient, et l'écran dit pourquoi rien ne sort.
    func testAnUnpluggedChoiceStaysListed() {
        let unplugged = AudioOutputsView(devices: ["Haut-parleurs MacBook Pro"], system: "Haut-parleurs MacBook Pro")
        let choices = AudioOutputChoices.choices(unplugged, chosen: "AirPods Pro")
        XCTAssertEqual(choices.last, .init(title: "AirPods Pro (débranché)", value: "AirPods Pro"))
    }

    func testTheButtonShowsWhatTheSoundGoesThrough() {
        XCTAssertEqual(AudioOutputChoices.symbol(for: ""), "airplayaudio")
        XCTAssertEqual(AudioOutputChoices.symbol(for: "AirPods Pro de Tharsan"), "headphones")
        XCTAssertEqual(AudioOutputChoices.symbol(for: "Haut-parleurs MacBook Pro"), "airplayaudio",
                       "jamais un haut-parleur : c'est le symbole du volume")
    }

    /// Le bouton du panneau nomme la sortie : celle choisie, sinon celle du système.
    func testTheButtonNamesTheOutput() {
        XCTAssertEqual(AudioOutputChoices.buttonTitle(outputs, chosen: ""), "AirPods Pro")
        XCTAssertEqual(AudioOutputChoices.buttonTitle(outputs, chosen: "Haut-parleurs MacBook Pro"),
                       "Haut-parleurs MacBook Pro")
        XCTAssertEqual(AudioOutputChoices.buttonTitle(AudioOutputsView(devices: [], system: nil), chosen: ""),
                       "Système")
    }
}
