import CryptoKit
import KokoroMLX
import MLX
import os
import XCTest
@testable import Dysflow

/// La porte du lot 3 : les deux copies de MLX sous charge, dans un même
/// processus, avant que la copie Rust n'entre dans l'app livrée.
///
/// À chaque tour, Kokoro sur la copie Rust (`mlxRustKokoroCheck`, fixture de
/// parité, graine 7) sur un fil et Parakeet v3 sur la copie Swift (chemin de
/// l'app : `SpeechModels`, `MLXGate.shared`, audio fixé) sur un autre, en
/// même temps. Puis Kokoro et Parakeet alternés pendant une durée donnée.
/// Plafonds de l'app posés (`MLXBudget` « dictee », budget Rust « kokoro »),
/// noyaux de la copie Rust à côté de l'exécutable, `HOME` temporaire sans
/// `~/.mlx`.
///
/// Assertions, à chaque passage : durées identiques à la fixture et écart
/// moyen < 0,01 ; texte identique ; jamais deux calculs à la fois (compté de
/// l'intérieur des deux calculs, voir `LoadComputeCounter`) ; aucun blocage
/// (30 s par tour au plus) ; mémoire résidente et empreinte physique du
/// processus : croissance ≤ 5 % entre le tour de référence (le 20e) et le
/// dernier.
///
/// Seulement par `scripts/mlx-charge.sh` (Release, moteur `ocr,mlx`) :
/// `DYSFLOW_KOKORO_DIR` (le dossier de Kokoro), `DYSFLOW_STT_MODELS` (celui
/// qui contient `parakeet_tdt_v3/`), `DYSFLOW_CHARGE_TOURS` (200) et
/// `DYSFLOW_CHARGE_ALTERNANCE_S` (600). Sans eux : ignoré. Les relevés sont
/// les lignes `MESURE charge` de la sortie ; celles de R15, `MESURE r15`,
/// viennent d'un test à part (`testR15RacesMeasuredApart`). Chaque étape
/// MLX, chargement et tour 0 compris, a son échéance de 30 s.
final class MLXLoadTests: XCTestCase {
    /// La dictée fixée : Kokoro, voix ff_siwis, graine 7, ramenée à 16 kHz par
    /// afconvert, 4,2 s (`tests/fixtures/mlx/make_dictees.sh`).
    static let dictationSHA256 = "23781dd70d485faf21d078a9849c2f3ae94754a5a42788afe28d129767fd675b"
    /// Ce que Parakeet v3 en lit : le texte de l'entrée « francais » de
    /// `tests/fixtures/mlx/parakeet_tdt_v3.json`, que le Swift a écrit
    /// (23 nov. 2026).
    static let dictationText = "N'oublie pas d'envoyer le devis au client avant la fin de la semaine."
    static let roundTimeout: TimeInterval = 30
    static let maximumGrowth = 0.05
    static let referenceRound = 20

    struct Reference: Decodable {
        let ids: [Int32]
        let style: [Float]
        let durations: [Int32]
        let audio_i16_base64: String
    }

    override func setUp() {
        continueAfterFailure = false
    }

    #if DYSFLOW_MLX_RUST
    /// Ce que les deux tests partagent : modèles chargés (chaque étape avec
    /// son échéance), sonde, nombre de tours et durée de l'alternance.
    struct Setting {
        let counter: LoadComputeCounter
        let kokoro: LoadKokoro
        let dictation: LoadDictation
        let rounds: Int
        let alternation: TimeInterval
    }

    /// `nil` : pas lancé par `scripts/mlx-charge.sh` (test ignoré).
    private func prepare() throws -> Setting? {
        let env = ProcessInfo.processInfo.environment
        guard let kokoroDir = env["DYSFLOW_KOKORO_DIR"], let sttRoot = env["DYSFLOW_STT_MODELS"] else {
            return nil
        }
        try checkTheSetting(kokoroDir: kokoroDir, sttRoot: sttRoot)
        let fixtures = URL(fileURLWithPath: #filePath).deletingLastPathComponent().appendingPathComponent("Fixtures")
        let reference = try JSONDecoder().decode(
            Reference.self, from: Data(contentsOf: fixtures.appendingPathComponent("kokoro-parite.json")))
        let expected = Data(base64Encoded: reference.audio_i16_base64)!.withUnsafeBytes {
            $0.bindMemory(to: Int16.self).map { Float(Int16(littleEndian: $0)) / 32767 }
        }
        let wav = try Data(contentsOf: fixtures.appendingPathComponent("dictee-charge.wav"))
        XCTAssertEqual(SHA256.hash(data: wav).map { String(format: "%02x", $0) }.joined(), Self.dictationSHA256)
        let samples = try Self.pcm16Samples(wav)
        let counter = LoadComputeCounter()
        let kokoro = LoadKokoro(dir: kokoroDir, reference: reference, expected: expected, counter: counter)
        let parakeetDir = URL(fileURLWithPath: sttRoot).appendingPathComponent("parakeet_tdt_v3")
        let dictation = try Self.within("chargement de Parakeet") {
            try LoadDictation(dir: parakeetDir, samples: samples, counter: counter)
        }
        return Setting(counter: counter, kokoro: kokoro, dictation: dictation,
                       rounds: Int(env["DYSFLOW_CHARGE_TOURS"] ?? "") ?? 200,
                       alternation: TimeInterval(env["DYSFLOW_CHARGE_ALTERNANCE_S"] ?? "") ?? 600)
    }
    #endif

    /// `body` sur un fil à lui, fini avant `roundTimeout` : un blocage est un
    /// échec du test, pas une porte qui attend sans fin.
    @discardableResult
    static func within<T>(_ what: String, _ body: @escaping @Sendable () throws -> T) throws -> T {
        let run = LoadThread("charge-\(what)", body)
        guard let result = run.result(before: .now() + roundTimeout) else {
            throw GateFailure("\(what) : bloqué plus de \(Int(roundTimeout)) s")
        }
        return try result.get()
    }

    func testBothMLXCopiesUnderLoad() throws {
        #if DYSFLOW_MLX_RUST
        print("MESURE charge debut \(LoadReadings.machine())")
        guard let setting = try prepare() else {
            throw XCTSkip("DYSFLOW_KOKORO_DIR ou DYSFLOW_STT_MODELS absent : scripts/mlx-charge.sh")
        }
        let (counter, kokoro, dictation, rounds) = (setting.counter, setting.kokoro, setting.dictation, setting.rounds)
        print("MESURE charge configuration=\(Self.configuration) tours=\(rounds) alternance_s=\(Int(setting.alternation))")

        // Une première fois chacun, seuls (Parakeet avant tout calcul de la
        // copie Rust) : les noyaux, et les plafonds posés.
        try Self.within("tour 0 : Parakeet") { try dictation.pass(round: 0) }
        let first = try Self.within("tour 0 : Kokoro") { try kokoro.pass(round: 0) }
        XCTAssertEqual(first.mlxMemoryLimit, 1 << 30, "plafond de Kokoro (copie Rust)")
        let swiftLimit = try Self.within("plafond de la copie Swift") { MLXGate.shared.run { Memory.memoryLimit } }
        XCTAssertEqual(swiftLimit, MLXBudget.shared.total.memory, "plafonds de l'app (copie Swift)")
        XCTAssertGreaterThan(swiftLimit, 0)

        // Le tour de référence de la croissance : le 20e, ou le milieu d'un
        // passage court.
        let referenceRound = min(Self.referenceRound, max(1, rounds / 2))
        var referenceResident = 0, referenceFootprint = 0
        var longest: TimeInterval = 0
        for round in 1...rounds {
            let started = Date()
            let rustPass = try Self.concurrentRound(round, kokoro: kokoro, dictation: dictation)
            longest = max(longest, Date().timeIntervalSince(started))
            XCTAssertEqual(counter.most, 1, "tour \(round) : deux calculs MLX à la fois")
            if round == referenceRound {
                referenceResident = LoadReadings.resident()
                referenceFootprint = LoadReadings.footprint()
            }
            if round == 1 || round == rounds || round % 50 == 0 || round == referenceRound {
                let copies = try Self.within("relevé des copies") { LoadReadings.copies(rust: rustPass) }
                print("MESURE charge tour=\(round) \(copies) \(LoadReadings.process()) \(LoadReadings.machine())")
            }
        }
        let finalResident = LoadReadings.resident(), finalFootprint = LoadReadings.footprint()
        let growth = Double(finalResident - referenceResident) / Double(referenceResident)
        // La mémoire de Metal (les tableaux des deux copies) n'est pas dans
        // la résidente : l'empreinte physique la compte. Même seuil.
        let footprintGrowth = Double(finalFootprint - referenceFootprint) / Double(referenceFootprint)
        print(String(format: "MESURE charge croissance_residente=%.2f %% (tour %d : %d Mo, tour %d : %d Mo) duree_tour_max=%.2f s",
                     growth * 100, referenceRound, referenceResident >> 20,
                     rounds, finalResident >> 20, longest))
        print(String(format: "MESURE charge croissance_empreinte=%.2f %% (tour %d : %d Mo, tour %d : %d Mo)",
                     footprintGrowth * 100, referenceRound, referenceFootprint >> 20, rounds, finalFootprint >> 20))
        XCTAssertLessThanOrEqual(growth, Self.maximumGrowth, "mémoire résidente : croissance de plus de 5 %")
        XCTAssertLessThanOrEqual(footprintGrowth, Self.maximumGrowth, "empreinte physique : croissance de plus de 5 %")

        // Lecture et dictée alternées.
        let end = Date(timeIntervalSinceNow: setting.alternation)
        var turns = (kokoro: 0, parakeet: 0)
        var lastRust = first
        while Date() < end {
            lastRust = try Self.within("alternance : Kokoro") { try kokoro.pass(round: -1) }
            turns.kokoro += 1
            try Self.within("alternance : Parakeet") { try dictation.pass(round: -1) }
            turns.parakeet += 1
        }
        XCTAssertEqual(counter.most, 1, "deux calculs MLX à la fois")
        let lastPass = lastRust
        let copies = try Self.within("relevé des copies") { LoadReadings.copies(rust: lastPass) }
        print("MESURE charge alternance kokoro=\(turns.kokoro) parakeet=\(turns.parakeet) \(copies) \(LoadReadings.process()) \(LoadReadings.machine())")
        print("MESURE charge calculs=\(counter.entered) simultanes_max=\(counter.most) attendus=\(2 * (rounds + 1) + turns.kokoro + turns.parakeet)")
        XCTAssertEqual(counter.entered, 2 * (rounds + 1) + turns.kokoro + turns.parakeet, "un calcul n'est pas passé par la sonde")
        print("MESURE charge fin \(LoadReadings.machine())")
        #else
        throw XCTSkip("moteur sans la feature mlx (FEATURES=ocr,mlx ./scripts/build-ffi.sh)")
        #endif
    }

    /// R15, à part de la porte (après elle, dans l'ordre de XCTest) : ses
    /// relevés ne font ni passer ni échouer la porte (`scripts/mlx-charge.sh`
    /// ne juge que `testBothMLXCopiesUnderLoad`). Rien d'affirmé, sauf les
    /// échéances.
    /// - Les compteurs de la copie Swift lus hors du verrou, toutes les 5 ms,
    ///   comme `Measure.compteursMLX`, pendant 20 tours simultanés.
    /// - `clearCache` de mlx-swift juste après un calcul, sous le verrou,
    ///   comme `SpeechModels.release` : le cache relu aussitôt, puis après que
    ///   le verrou a attendu les flux.
    func testR15RacesMeasuredApart() throws {
        #if DYSFLOW_MLX_RUST
        guard let setting = try prepare() else {
            throw XCTSkip("DYSFLOW_KOKORO_DIR ou DYSFLOW_STT_MODELS absent : scripts/mlx-charge.sh")
        }
        let (kokoro, dictation) = (setting.kokoro, setting.dictation)
        let outside = LoadOutsideReader()
        outside.start()
        for round in 1...20 {
            _ = try Self.concurrentRound(round, kokoro: kokoro, dictation: dictation)
        }
        let readings = outside.stop()
        print("MESURE r15 lectures_hors_verrou=\(readings.count) actif_max_mo=\(readings.maxActive >> 20) (20 tours)")

        var rightAway: [Int] = [], later: [Int] = []
        for _ in 0..<20 {
            let cache = try Self.within("R15 clearCache") { try dictation.clearCacheRightAfterAPass() }
            rightAway.append(cache.rightAway)
            later.append(cache.later)
        }
        print("MESURE r15 clear_cache fois=20 cache_aussitot_non_nul=\(rightAway.filter { $0 > 0 }.count) "
            + "max_aussitot_ko=\((rightAway.max() ?? 0) >> 10) cache_ensuite_non_nul=\(later.filter { $0 > 0 }.count) "
            + "max_ensuite_ko=\((later.max() ?? 0) >> 10)")
        #else
        throw XCTSkip("moteur sans la feature mlx (FEATURES=ocr,mlx ./scripts/build-ffi.sh)")
        #endif
    }

    #if DYSFLOW_MLX_RUST
    /// Kokoro (copie Rust) et Parakeet (copie Swift), chacun sur son fil, en
    /// même temps ; les deux finis avant `roundTimeout`.
    static func concurrentRound(_ round: Int, kokoro: LoadKokoro, dictation: LoadDictation) throws -> MlxRustKokoroPass {
        let deadline = DispatchTime.now() + roundTimeout
        let rust = LoadThread("charge-kokoro") { try kokoro.pass(round: round) }
        let swift = LoadThread("charge-parakeet") { try dictation.pass(round: round) }
        let rustPass = try XCTUnwrap(rust.result(before: deadline), "tour \(round) : Kokoro bloqué plus de 30 s").get()
        _ = try XCTUnwrap(swift.result(before: deadline), "tour \(round) : Parakeet bloqué plus de 30 s").get()
        return rustPass
    }
    #endif

    static var configuration: String {
        #if DEBUG
        "Debug"
        #else
        "Release"
        #endif
    }

    /// `HOME` temporaire sans `~/.mlx`, modèles hors de la bibliothèque de
    /// l'utilisateur, noyaux de la copie Rust à côté de l'exécutable.
    private func checkTheSetting(kokoroDir: String, sttRoot: String) throws {
        let env = ProcessInfo.processInfo.environment
        let home = try XCTUnwrap(env["HOME"])
        let realHome = String(cString: try XCTUnwrap(getpwuid(getuid())).pointee.pw_dir)
        XCTAssertNotEqual(URL(fileURLWithPath: home).standardized, URL(fileURLWithPath: realHome).standardized,
                          "HOME doit être temporaire")
        XCTAssertFalse(FileManager.default.fileExists(atPath: home + "/.mlx"), "~/.mlx présent")
        for dir in [kokoroDir, sttRoot] {
            XCTAssertFalse(dir.hasPrefix(realHome + "/Library/"), "modèles pris dans la bibliothèque : \(dir)")
        }
        XCTAssertNil(env["DYSFLOW_MLX_METALLIB"], "les noyaux doivent venir de l'app, pas de la variable")
        let kernels = try XCTUnwrap(Bundle.main.executableURL).deletingLastPathComponent()
            .appendingPathComponent("mlx.metallib")
        XCTAssertTrue(FileManager.default.fileExists(atPath: kernels.path), "\(kernels.path) absent")
    }

    /// Les échantillons d'un WAV PCM 16 bits mono 16 kHz, entre -1 et 1.
    static func pcm16Samples(_ wav: Data) throws -> [Float] {
        let bytes = [UInt8](wav)
        func u32(_ at: Int) -> Int { Int(bytes[at]) | Int(bytes[at + 1]) << 8 | Int(bytes[at + 2]) << 16 | Int(bytes[at + 3]) << 24 }
        func u16(_ at: Int) -> Int { Int(bytes[at]) | Int(bytes[at + 1]) << 8 }
        XCTAssertEqual(Array(bytes[0..<4]), Array("RIFF".utf8))
        var at = 12
        var format: (channels: Int, rate: Int, bits: Int)?
        while at + 8 <= bytes.count {
            let id = String(decoding: bytes[at..<at + 4], as: UTF8.self), size = u32(at + 4)
            if id == "fmt " {
                format = (u16(at + 10), u32(at + 12), u16(at + 22))
            } else if id == "data" {
                let f = try XCTUnwrap(format, "fmt absent")
                XCTAssertEqual(f.channels, 1)
                XCTAssertEqual(f.rate, 16_000)
                XCTAssertEqual(f.bits, 16)
                return stride(from: at + 8, to: at + 8 + size - 1, by: 2).map {
                    Float(Int16(bitPattern: UInt16(u16($0)))) / 32768
                }
            }
            at += 8 + size + size % 2
        }
        throw GateFailure("WAV sans données")
    }
}

#if DYSFLOW_MLX_RUST
/// « Jamais deux calculs à la fois », vu de l'intérieur : `enter` au début
/// de chaque calcul et `leave` à sa fin, verrou du GPU tenu. Côté Rust,
/// `mlx_rust_kokoro_check` les appelle sur le fil MLX, dans le travail
/// (`MlxBackend::infer_seeded`) ; côté Swift, `LoadDictation.pass` les appelle
/// dans le corps de `MLXGate.shared.run`.
final class LoadComputeCounter: MlxComputeProbe, @unchecked Sendable {
    private let state = OSAllocatedUnfairLock(initialState: (inside: 0, most: 0, entered: 0))

    func enter() {
        state.withLock {
            $0.inside += 1
            $0.most = max($0.most, $0.inside)
            $0.entered += 1
        }
    }

    func leave() {
        state.withLock { $0.inside -= 1 }
    }

    var most: Int { state.withLock { $0.most } }
    var entered: Int { state.withLock { $0.entered } }
}

/// Kokoro sur la copie Rust : chaque passage rend la fixture.
final class LoadKokoro: @unchecked Sendable {
    let dir: String
    let ids: [Int64]
    let style: [Float]
    let durations: [UInt32]
    let expected: [Float]
    let counter: LoadComputeCounter

    init(dir: String, reference: MLXLoadTests.Reference, expected: [Float], counter: LoadComputeCounter) {
        self.dir = dir
        ids = reference.ids.map(Int64.init)
        style = reference.style
        durations = reference.durations.map(UInt32.init)
        self.expected = expected
        self.counter = counter
    }

    /// `round` : -1 pendant l'alternance.
    func pass(round: Int) throws -> MlxRustKokoroPass {
        let out = try mlxRustKokoroCheck(modelDir: dir, ids: ids, style: style, seed: 7, probe: counter)
        guard out.durations == durations else {
            throw GateFailure("tour \(round) : durées \(out.durations), attendu \(durations)")
        }
        guard out.samples.count == expected.count else {
            throw GateFailure("tour \(round) : \(out.samples.count) échantillons, attendu \(expected.count)")
        }
        let meanError = zip(out.samples, expected).map { abs($0 - $1) }.reduce(0, +) / Float(expected.count)
        guard meanError < 0.01 else { throw GateFailure("tour \(round) : écart moyen \(meanError)") }
        return out
    }
}

/// Parakeet v3 sur la copie Swift, par le chemin de l'app.
final class LoadDictation: @unchecked Sendable {
    let model: LoadedSpeechModel
    let samples: [Float]
    let counter: LoadComputeCounter

    init(dir: URL, samples: [Float], counter: LoadComputeCounter) throws {
        model = try MLXGate.shared.run {
            Self.stallIfAsked("init")
            return try SpeechModels.load(dir)
        }
        self.samples = samples
        self.counter = counter
    }

    @discardableResult
    func pass(round: Int) throws -> String {
        let text = try MLXGate.shared.run {
            counter.enter()
            defer { counter.leave() }
            if round == 0 { Self.stallIfAsked("tour0") }
            return try model.transcribe(samples, language: "fr")
        }
        if round == 0 { print("MESURE charge texte «\(text)»") }
        guard text == MLXLoadTests.dictationText else {
            throw GateFailure("tour \(round) : texte «\(text)», attendu «\(MLXLoadTests.dictationText)»")
        }
        return text
    }

    /// L'essai de l'échéance : `DYSFLOW_CHARGE_BLOCAGE` = `init` ou `tour0`
    /// fait dormir cette étape `DYSFLOW_CHARGE_BLOCAGE_S` secondes (45),
    /// verrou du GPU tenu, comme un calcul bloqué. Jamais posé par la porte.
    static func stallIfAsked(_ step: String) {
        let env = ProcessInfo.processInfo.environment
        guard env["DYSFLOW_CHARGE_BLOCAGE"] == step else { return }
        let seconds = Double(env["DYSFLOW_CHARGE_BLOCAGE_S"] ?? "") ?? 45
        print("MESURE charge blocage_voulu etape=\(step) s=\(Int(seconds))")
        Thread.sleep(forTimeInterval: seconds)
    }

    /// Une transcription, `clearCache` aussitôt, le cache relu aussitôt ;
    /// puis relu sous un nouveau passage du verrou (le premier a attendu les
    /// flux). R15.
    func clearCacheRightAfterAPass() throws -> (rightAway: Int, later: Int) {
        let rightAway = try MLXGate.shared.run {
            _ = try model.transcribe(samples, language: "fr")
            DictationMLX.clearCache()
            return DictationMLX.counters().cache
        }
        return (rightAway, MLXGate.shared.run { DictationMLX.counters().cache })
    }
}
#endif

/// Un écart de la porte, levé sur le fil du calcul et rendu au test.
struct GateFailure: Error, CustomStringConvertible {
    let description: String
    init(_ description: String) { self.description = description }
}

/// Un corps sur un fil à lui (pile de 8 Mo), et son résultat attendu avec
/// une échéance : un blocage est un échec, pas un test sans fin.
final class LoadThread<T>: @unchecked Sendable {
    private let done = DispatchSemaphore(value: 0)
    private let outcome = OSAllocatedUnfairLock<Result<T, Error>?>(uncheckedState: nil)

    init(_ name: String, _ body: @escaping @Sendable () throws -> T) {
        let thread = Thread { [self] in
            let result = Result { try body() }
            outcome.withLockUnchecked { $0 = result }
            done.signal()
        }
        thread.name = name
        thread.stackSize = 8 << 20
        thread.qualityOfService = .userInitiated
        thread.start()
    }

    /// `nil` : pas fini avant `deadline`.
    func result(before deadline: DispatchTime) -> Result<T, Error>? {
        guard done.wait(timeout: deadline) == .success else { return nil }
        return outcome.withLockUnchecked { $0 }
    }
}

/// Lit les compteurs de la copie Swift sans `MLXGate`, toutes les 5 ms,
/// comme `Measure.compteursMLX` (R15).
final class LoadOutsideReader: @unchecked Sendable {
    private let state = OSAllocatedUnfairLock(initialState: (running: true, finished: false, count: 0, maxActive: 0))

    func start() {
        Thread.detachNewThread { [self] in
            while state.withLock({ $0.running }) {
                let active = DictationMLX.counters().active
                state.withLock {
                    $0.count += 1
                    $0.maxActive = max($0.maxActive, active)
                }
                usleep(5_000)
            }
            state.withLock { $0.finished = true }
        }
    }

    /// Attend la fin du fil sans sémaphore : le fil du test, plus prioritaire,
    /// n'attend pas un fil de priorité par défaut.
    func stop() -> (count: Int, maxActive: Int) {
        state.withLock { $0.running = false }
        while !state.withLock({ $0.finished }) { usleep(1_000) }
        return state.withLock { ($0.count, $0.maxActive) }
    }
}

/// Les relevés : mémoire des deux copies, du processus, de la machine.
enum LoadReadings {
    #if DYSFLOW_MLX_RUST
    static func copies(rust: MlxRustKokoroPass) -> String {
        let swift = MLXGate.shared.run { (Memory.activeMemory, Memory.peakMemory, Memory.cacheMemory) }
        return "rust_actif_mo=\(rust.mlxActive >> 20) rust_pic_mo=\(rust.mlxPeak >> 20) rust_cache_mo=\(rust.mlxCache >> 20) "
            + "swift_actif_mo=\(swift.0 >> 20) swift_pic_mo=\(swift.1 >> 20) swift_cache_mo=\(swift.2 >> 20)"
    }
    #endif

    static func process() -> String {
        "residente_mo=\(resident() >> 20) empreinte_mo=\(footprint() >> 20)"
    }

    static func machine() -> String {
        var loads = [Double](repeating: 0, count: 3)
        getloadavg(&loads, 3)
        var swap = xsw_usage(), size = MemoryLayout<xsw_usage>.size
        sysctlbyname("vm.swapusage", &swap, &size, nil, 0)
        return String(format: "charge=%.2f,%.2f,%.2f swap_mo=%llu", loads[0], loads[1], loads[2], swap.xsu_used >> 20)
    }

    /// Mémoire résidente du processus, en octets.
    static func resident() -> Int {
        var info = mach_task_basic_info()
        var count = mach_msg_type_number_t(MemoryLayout<mach_task_basic_info>.size / MemoryLayout<natural_t>.size)
        let kr = withUnsafeMutablePointer(to: &info) {
            $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
                task_info(mach_task_self_, task_flavor_t(MACH_TASK_BASIC_INFO), $0, &count)
            }
        }
        return kr == KERN_SUCCESS ? Int(info.resident_size) : 0
    }

    /// L'empreinte physique (ce que le Moniteur d'activité appelle mémoire),
    /// mémoire Metal comprise.
    static func footprint() -> Int {
        var info = task_vm_info_data_t()
        var count = mach_msg_type_number_t(MemoryLayout<task_vm_info_data_t>.size / MemoryLayout<natural_t>.size)
        let kr = withUnsafeMutablePointer(to: &info) {
            $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
                task_info(mach_task_self_, task_flavor_t(TASK_VM_INFO), $0, &count)
            }
        }
        return kr == KERN_SUCCESS ? Int(info.phys_footprint) : 0
    }
}
