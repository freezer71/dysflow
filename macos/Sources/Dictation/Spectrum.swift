import Accelerate
import Foundation

/// Le spectre de la voix, tranche par tranche : chaque tranche (une colonne
/// du spectrogramme) donne l'énergie de `bands` bandes de fréquence, de
/// 90 Hz à 7 kHz en échelle logarithmique (là où vit la voix), entre 0 et 1.
///
/// Appelé sur le fil audio. Le gain suit la voix : le crête récent fixe le
/// haut de l'échelle, si bien qu'un micro discret dessine autant qu'un fort.
final class SpectrumAnalyzer {
    static let size = 1024
    /// Tranches par seconde.
    static let rate: Double = 40

    let bands: Int
    private let hop: Int
    private let fft: vDSP.FFT<DSPSplitComplex>
    private let window: [Float]
    private let edges: [Int]
    private var pending: [Float] = []
    /// Les tableaux de travail d'une tranche, alloués une fois : `column`
    /// tourne sur le fil audio, 40 fois par seconde.
    private var frame = [Float](repeating: 0, count: SpectrumAnalyzer.size)
    private var windowed = [Float](repeating: 0, count: SpectrumAnalyzer.size)
    private var real = [Float](repeating: 0, count: SpectrumAnalyzer.size / 2)
    private var imaginary = [Float](repeating: 0, count: SpectrumAnalyzer.size / 2)
    private var power = [Float](repeating: 0, count: SpectrumAnalyzer.size / 2)
    private var decibels: [Float]
    /// Le haut de l'échelle, en dB : monte d'un coup, redescend doucement.
    private var peak: Float = -60
    /// L'étendue affichée sous le crête.
    static let range: Float = 42
    /// En dessous, c'est le silence de la pièce : le crête n'y descend pas.
    static let quietestPeak: Float = -62

    init(sampleRate: Double, bands: Int = 12) {
        self.bands = bands
        hop = max(1, Int(sampleRate / Self.rate))
        let log2n = vDSP_Length(log2(Float(Self.size)))
        fft = vDSP.FFT(log2n: log2n, radix: .radix2, ofType: DSPSplitComplex.self)!
        window = vDSP.window(ofType: Float.self, usingSequence: .hanningDenormalized,
                             count: Self.size, isHalfWindow: false)
        edges = Self.bandEdges(bands: bands, sampleRate: sampleRate, size: Self.size)
        decibels = [Float](repeating: -120, count: bands)
    }

    /// Les limites des bandes, en indices de raies de la FFT.
    static func bandEdges(bands: Int, sampleRate: Double, size: Int,
                          low: Double = 90, high: Double = 7000) -> [Int] {
        let binWidth = sampleRate / Double(size)
        let top = min(high, sampleRate / 2 * 0.95)
        return (0...bands).map { i in
            let f = low * pow(top / low, Double(i) / Double(bands))
            return min(size / 2 - 1, max(1, Int((f / binWidth).rounded())))
        }
    }

    /// Ajoute des échantillons ; rend les tranches qu'ils complètent.
    func push(_ samples: UnsafeBufferPointer<Float>) -> [[Float]] {
        pending.append(contentsOf: samples)
        var columns: [[Float]] = []
        while pending.count >= Self.size {
            frame.withUnsafeMutableBufferPointer { out in
                pending.withUnsafeBufferPointer { out.baseAddress!.update(from: $0.baseAddress!, count: Self.size) }
            }
            columns.append(column())
            pending.removeFirst(min(hop, pending.count))
        }
        // Le retard ne s'accumule jamais : au plus une fenêtre en attente.
        if pending.count > Self.size * 2 { pending.removeFirst(pending.count - Self.size) }
        return columns
    }

    /// La tranche de `frame`.
    private func column() -> [Float] {
        let half = Self.size / 2
        vDSP.multiply(frame, window, result: &windowed)
        real.withUnsafeMutableBufferPointer { re in
            imaginary.withUnsafeMutableBufferPointer { im in
                var split = DSPSplitComplex(realp: re.baseAddress!, imagp: im.baseAddress!)
                windowed.withUnsafeBufferPointer { input in
                    input.baseAddress!.withMemoryRebound(to: DSPComplex.self, capacity: half) {
                        vDSP_ctoz($0, 2, &split, 1, vDSP_Length(half))
                    }
                }
                fft.forward(input: split, output: &split)
                vDSP.squareMagnitudes(split, result: &power)
            }
        }
        // L'énergie moyenne de chaque bande, en dB.
        let scale = 1 / Float(Self.size * Self.size)
        for b in 0..<bands {
            let lo = edges[b], hi = max(edges[b] + 1, edges[b + 1])
            let mean = power[lo..<hi].reduce(0, +) / Float(hi - lo) * scale
            decibels[b] = 10 * log10(max(mean, 1e-12))
        }
        let loudest = decibels.max() ?? -120
        peak = max(Self.quietestPeak, loudest > peak ? loudest : peak - 0.25)
        return decibels.map { Self.normalized($0, peak: peak) }
    }

    /// 0 à 1, sous le crête ; la courbe adoucit le bas pour que le souffle
    /// reste discret et que la voix ressorte.
    static func normalized(_ decibels: Float, peak: Float) -> Float {
        let x = min(1, max(0, (decibels - (peak - range)) / range))
        return x * x
    }
}

/// Les tranches du spectrogramme, du plus ancien au plus récent, lues par
/// l'affichage à chaque image : elles arrivent par paquets (le micro livre
/// ~100 ms à la fois) et en sortent à cadence régulière, pour un défilement
/// sans à-coups. Hors de l'observation : seul le dessin les lit.
@MainActor
final class SpectrumFeed {
    let capacity: Int
    let bands: Int
    private(set) var columns: [[Float]]
    private var queue: [[Float]] = []
    private var clock: TimeInterval?
    /// La part de colonne déjà glissée (0 à 1), pour un défilement continu.
    private(set) var phase: Double = 0

    init(capacity: Int = 8, bands: Int = 12) {
        self.capacity = capacity
        self.bands = bands
        columns = Array(repeating: Array(repeating: 0, count: bands), count: capacity)
    }

    func reset() {
        columns = Array(repeating: Array(repeating: 0, count: bands), count: capacity)
        queue = []
        clock = nil
        phase = 0
    }

    func push(_ new: [[Float]]) {
        queue.append(contentsOf: new.filter { $0.count == bands })
        // Trop de retard (fenêtre cachée, machine chargée) : on rattrape.
        if queue.count > 12 { queue.removeFirst(queue.count - 4) }
    }

    /// Avance jusqu'à `now` (secondes) : les colonnes en attente entrent au
    /// rythme où elles ont été enregistrées.
    func advance(to now: TimeInterval) {
        defer { clock = now }
        // Rien en attente : l'image reste, sans glisser vers le vide.
        guard let last = clock, !queue.isEmpty else { return }
        // Un peu plus vite quand la file s'allonge : elle ne déborde pas.
        let speed = SpectrumAnalyzer.rate * (1 + Double(max(0, queue.count - 3)) * 0.15)
        phase += (now - last) * speed
        while phase >= 1, !queue.isEmpty {
            columns.removeFirst()
            columns.append(queue.removeFirst())
            phase -= 1
        }
        phase = min(phase, 0.999)
    }
}
