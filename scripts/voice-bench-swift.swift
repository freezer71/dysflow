// Banc de Kokoro MLX en Swift, le pendant de examples/voice_bench.rs :
// mêmes entrées (la fixture, phrases de 34 et 162 jetons), mêmes mesures,
// même ligne JSON. Lié au KokoroMLX.framework construit en Release par
// scripts/voice-bench-swift.sh ; rien n'est joué, aucune fenêtre.
//
//   voice-bench-swift <fixture.json> <dossier kokoro>

import Foundation
import KokoroMLX
import MLX

struct Fixture: Decodable {
    let ids: [Int32]
    let style: [Float]
}

let warmRuns = 10
let longRepeats = 5

func now() -> Double { Double(DispatchTime.now().uptimeNanoseconds) / 1e9 }

func loadAverage() -> [Double] {
    var loads = [Double](repeating: 0, count: 3)
    let n = getloadavg(&loads, 3)
    return Array(loads.prefix(Int(max(0, n))))
}

func maxRSS() -> Int {
    var usage = rusage()
    getrusage(RUSAGE_SELF, &usage)
    return Int(usage.ru_maxrss)
}

let arguments = CommandLine.arguments
guard arguments.count == 3 else {
    FileHandle.standardError.write("usage : voice-bench-swift <fixture.json> <dossier kokoro>\n".data(using: .utf8)!)
    exit(2)
}
let fixture = try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: URL(fileURLWithPath: arguments[1])))
let body = Array(fixture.ids[1..<(fixture.ids.count - 1)])
var long: [Int32] = [0]
for _ in 0..<longRepeats { long += body }
long.append(0)
let loadBefore = loadAverage()

// Les plafonds de l'app : ceux de Kokoro seul (KokoroMLXCompute).
var started = now()
let runner = try KokoroRunner.load(directory: URL(fileURLWithPath: arguments[2]),
                                   memoryLimit: 1 << 30, cacheLimit: 64 << 20)
let loadSeconds = now() - started

var sentences: [[String: Any]] = []
for input in [fixture.ids, long] {
    Memory.peakMemory = 0
    started = now()
    guard let out = runner.run(ids: input, style: fixture.style, shouldStop: nil) else { exit(1) }
    let first = now() - started
    var runs: [Double] = []
    for _ in 0..<warmRuns {
        started = now()
        _ = runner.run(ids: input, style: fixture.style, shouldStop: nil)
        runs.append(now() - started)
    }
    runs.sort()
    let median = (runs[warmRuns / 2 - 1] + runs[warmRuns / 2]) / 2
    let audio = Double(out.samples.count) / 24_000
    sentences.append([
        "tokens": input.count,
        "audio_s": audio,
        "first_s": first,
        "median_s": median,
        "min_s": runs[0],
        "max_s": runs[warmRuns - 1],
        "real_time_factor": audio / median,
        "mlx_peak_bytes": Memory.peakMemory,
    ])
}
let report: [String: Any] = [
    "engine": "swift mlx-swift",
    "load_s": loadSeconds,
    "sentences": sentences,
    "rss_max_bytes": maxRSS(),
    "load_average_before": loadBefore,
    "load_average_after": loadAverage(),
]
let json = try JSONSerialization.data(withJSONObject: report, options: [.sortedKeys])
print(String(data: json, encoding: .utf8)!)
