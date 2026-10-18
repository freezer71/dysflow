import AppKit
import XCTest
@testable import Sillage

@MainActor
final class ErrorTextTests: XCTestCase {
    /// Chaque échec est dit à VoiceOver, même quand le texte se répète ;
    /// effacer ne dit rien.
    func testEveryFailureIsAnnouncedEvenWhenRepeated() {
        var said: [String] = []
        let note = ErrorNote(announce: { said.append($0) })
        note.fail("disque plein")
        note.fail("disque plein")
        note.clear()
        XCTAssertNil(note.text)
        XCTAssertEqual(said, ["disque plein", "disque plein"])
    }

    /// Une écriture refusée deux fois de suite pour la même raison est dite
    /// deux fois.
    func testARefusedWriteIsAnnouncedEachTime() async {
        var said: [String] = []
        let backend = FakeSettingsBackend()
        let store = SettingsStore(backend: backend, delay: .milliseconds(10),
                                  announce: { said.append($0) })
        await store.load()
        backend.failNext = "config.toml en lecture seule"
        store.set(\.speed, 1.4)
        _ = await eventuallyTrue { said.count == 1 }
        backend.failNext = "config.toml en lecture seule"
        store.set(\.speed, 1.6)
        let twice = await eventuallyTrue { said.count == 2 }
        XCTAssertTrue(twice)
        XCTAssertEqual(store.error, "config.toml en lecture seule")
    }

    /// 1a : le refus d'« Écouter » est dit à VoiceOver, pas seulement affiché.
    func testARefusedPreviewIsAnnounced() async {
        var said: [String] = []
        let backend = FakeSettingsBackend()
        backend.failPreview = "sortie audio indisponible"
        let store = SettingsStore(backend: backend, announce: { said.append($0) })
        await store.load()
        await store.listen(language: "fr")
        await store.listen(language: "fr")
        XCTAssertEqual(store.previewNote, "sortie audio indisponible")
        XCTAssertEqual(said, ["sortie audio indisponible", "sortie audio indisponible"])
    }

    /// 1d : le rouge des erreurs atteint 4,5:1 sur le fond de la fenêtre et
    /// sur celui d'une ligne de formulaire, en clair comme en sombre.
    func testTheErrorRedReadsInBothAppearances() throws {
        // Les lignes d'un `Form` groupé, mesurées : blanc en clair, gris en sombre.
        let rows: [NSAppearance.Name: [NSColor]] = [
            .aqua: [.white, NSColor(white: 0.90, alpha: 1)],
            .darkAqua: [NSColor(white: 0.20, alpha: 1), NSColor(white: 0.275, alpha: 1)],
        ]
        for (name, rowColors) in rows {
            let appearance = try XCTUnwrap(NSAppearance(named: name))
            var fg: NSColor?
            var backgrounds: [NSColor] = []
            appearance.performAsCurrentDrawingAppearance {
                fg = ErrorStyle.nsColor.usingColorSpace(.sRGB)
                backgrounds = [NSColor.windowBackgroundColor, .controlBackgroundColor]
                    .compactMap { $0.usingColorSpace(.sRGB) }
            }
            let red = try XCTUnwrap(fg)
            for bg in backgrounds + rowColors.compactMap({ $0.usingColorSpace(.sRGB) }) {
                XCTAssertGreaterThanOrEqual(Self.contrast(red, bg), 4.5, "\(name.rawValue) sur \(bg)")
            }
        }
    }

    private static func luminance(_ c: NSColor) -> Double {
        func linear(_ v: CGFloat) -> Double {
            let v = Double(v)
            return v <= 0.03928 ? v / 12.92 : pow((v + 0.055) / 1.055, 2.4)
        }
        return 0.2126 * linear(c.redComponent) + 0.7152 * linear(c.greenComponent)
            + 0.0722 * linear(c.blueComponent)
    }

    private static func contrast(_ a: NSColor, _ b: NSColor) -> Double {
        let (la, lb) = (luminance(a), luminance(b))
        return (max(la, lb) + 0.05) / (min(la, lb) + 0.05)
    }
}
