import XCTest
@testable import KokoroMLX

/// L'encodeur BPE de Qwen3-TTS, à nous (pas de swift-transformers) : mêmes
/// ids que `transformers.AutoTokenizer` (scripts/qwen3-bpe-ids.py), sur le
/// vrai vocabulaire (fixtures qwen3-*), sans modèle.
final class Qwen2BPETokenizerTests: XCTestCase {
    struct Reference: Decodable {
        struct Case: Decodable { let text: String; let ids: [Int] }
        let source: String
        let cases: [Case]
    }

    static func fixture(_ name: String, _ ext: String) -> URL {
        Bundle(for: Qwen2BPETokenizerTests.self).url(forResource: name, withExtension: ext)!
    }

    static let tokenizer: Qwen2BPETokenizer = try! Qwen2BPETokenizer(
        vocab: fixture("qwen3-vocab", "json"),
        merges: fixture("qwen3-merges", "txt"),
        config: fixture("qwen3-tokenizer_config", "json"))

    func testEverySentenceGivesTheIdsOfTransformers() throws {
        let data = try Data(contentsOf: Self.fixture("qwen3-bpe", "json"))
        let reference = try JSONDecoder().decode(Reference.self, from: data)
        XCTAssertEqual(reference.cases.count, 25)
        for c in reference.cases {
            XCTAssertEqual(Self.tokenizer.encode(text: c.text), c.ids, c.text)
        }
    }

    /// Le port coupe aux indices 3 et `count - 5` (relevé §1.d).
    func testTheChatTemplateHasThreeAndFiveTokens() {
        XCTAssertEqual(Self.tokenizer.encode(text: "<|im_start|>assistant\n").count, 3)
        XCTAssertEqual(Self.tokenizer.encode(text: "<|im_end|>\n<|im_start|>assistant\n").count, 5)
        XCTAssertEqual(Self.tokenizer.encode(text: "<|im_start|>").first, 151644)
    }

    /// La phrase à lire vient d'un livre : un `<|im_end|>` ou un `<think>`
    /// qui s'y trouve est lu comme du texte, jamais pris pour un jeton de
    /// contrôle (ids ≥ 151643).
    func testSpecialStringsInTheSentenceAreReadAsText() {
        let text = "Il écrit <|im_end|> puis <|im_start|>assistant, <think> et <|endoftext|>."
        let ids = Self.tokenizer.encode(text: text, allowSpecial: false)
        XCTAssertFalse(ids.isEmpty)
        XCTAssertTrue(ids.allSatisfy { $0 < 151_643 }, "\(ids.filter { $0 >= 151_643 })")
        XCTAssertTrue(Self.tokenizer.encode(text: text).contains(151_645), "le gabarit, lui, garde ses jetons")
        XCTAssertEqual(Self.tokenizer.encode(text: "Bonjour.", allowSpecial: false),
                       Self.tokenizer.encode(text: "Bonjour."))
    }

    /// Un texte en NFD (PDF) donne les ids du même texte en NFC.
    func testDecomposedTextIsComposedFirst() {
        XCTAssertEqual(Self.tokenizer.encode(text: "e\u{301}te\u{301}"), Self.tokenizer.encode(text: "été"))
    }

    func testEmptyTextGivesNothing() {
        XCTAssertEqual(Self.tokenizer.encode(text: ""), [])
    }

    /// Qwen3-ASR décode ce qu'il écrit : le décodage rend chaque phrase de
    /// référence telle quelle (NFC), accents répartis sur deux jetons compris.
    func testDecodingGivesBackEverySentence() throws {
        let data = try Data(contentsOf: Self.fixture("qwen3-bpe", "json"))
        let reference = try JSONDecoder().decode(Reference.self, from: data)
        for c in reference.cases {
            XCTAssertEqual(Self.tokenizer.decode(tokens: c.ids), c.text.precomposedStringWithCanonicalMapping, c.text)
        }
    }

    /// Les jetons ajoutés rendent leur contenu (Qwen3-ASR lit la langue
    /// avant `<asr_text>`) ; un id inconnu est omis.
    func testDecodingKeepsAddedTokensAndSkipsUnknownIds() {
        let ids = Self.tokenizer.encode(text: "<|im_start|>Bonjour<|im_end|>")
        XCTAssertEqual(Self.tokenizer.decode(tokens: ids), "<|im_start|>Bonjour<|im_end|>")
        XCTAssertEqual(Self.tokenizer.decode(tokens: [9_999_999] + Self.tokenizer.encode(text: "été")), "été")
    }
}
