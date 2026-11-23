import AVFoundation
import SwiftUI

@MainActor final class Voice: ObservableObject {
    struct ShapeKey {
        let start: Double
        let end: Double
        let pose: String
    }

    @Published private(set) var playing = false
    @Published private(set) var recording = false
    private var player: AVAudioPlayer?
    private let speech = AVSpeechSynthesizer()
    private var shapes: [ShapeKey] = []
    private var capture: AVAudioRecorder?
    private var captureURL: URL?
    private var generation = 0

    func speak(_ text: String, through core: CoreSession) async {
        stopPlayback()
        let request = generation
        let text = String(text.prefix(2_000))
        do {
            let result = try await core.request("/api/voice/speak", method: "POST",
                                                body: ["text": text, "with_visemes": true], timeout: 25)
            guard generation == request else { return }
            guard let encoded = result["audio_b64"] as? String,
                  let bytes = Data(base64Encoded: encoded) else {
                throw CoreFailure(message: "Audio non disponibile.")
            }
            shapes = (result["visemes"] as? [[String: Any]] ?? []).compactMap { key in
                guard let t = key["t"] as? Double, let d = key["d"] as? Double,
                      let pose = key["v"] as? String else { return nil }
                return ShapeKey(start: t, end: t + d, pose: pose)
            }
            let playback = try AVAudioPlayer(data: bytes)
            playback.isMeteringEnabled = true
            guard playback.play() else { throw CoreFailure(message: "Riproduzione non disponibile.") }
            player = playback
        } catch {
            guard generation == request else { return }
            let utterance = AVSpeechUtterance(string: text)
            utterance.voice = AVSpeechSynthesisVoice(language: "it-IT")
            speech.speak(utterance)
        }
        playing = true
        while generation == request && (player?.isPlaying == true || speech.isSpeaking) {
            try? await Task.sleep(for: .milliseconds(70))
            if Task.isCancelled { break }
        }
        if generation == request { stopPlayback() }
    }

    func stopPlayback() {
        generation += 1
        player?.stop()
        player = nil
        speech.stopSpeaking(at: .immediate)
        playing = false
    }

    func mouth(at time: Double) -> (width: Double, opening: Double) {
        if let player, player.isPlaying {
            let cursor = player.currentTime * 1000
            let pose = shapes.first { $0.start <= cursor && cursor < $0.end }?.pose
            switch pose {
            case "A": return (28, 16)
            case "E", "I": return (32, 7)
            case "O", "U": return (17, 14)
            case "M", "X": return (24, 2)
            default:
                player.updateMeters()
                return (23, max(2, Double(42 + player.averagePower(forChannel: 0)) / 3))
            }
        }
        return speech.isSpeaking ? (25, 3 + abs(sin(time * 22)) * 9) : (25, 2)
    }

    func startRecording() async throws {
        guard capture == nil else { return }
        guard await AVCaptureDevice.requestAccess(for: .audio) else {
            throw CoreFailure(message: "Consenti a JARVIS di usare il microfono in Impostazioni di Sistema.")
        }
        stopPlayback()
        let path = FileManager.default.temporaryDirectory.appendingPathComponent("jarvis-\(UUID()).wav")
        let recorder = try AVAudioRecorder(url: path, settings: [
            AVFormatIDKey: kAudioFormatLinearPCM, AVSampleRateKey: 16000,
            AVNumberOfChannelsKey: 1, AVLinearPCMBitDepthKey: 16
        ])
        guard recorder.record(forDuration: 60) else {
            try? FileManager.default.removeItem(at: path)
            throw CoreFailure(message: "Registrazione non disponibile.")
        }
        capture = recorder
        captureURL = path
        recording = true
    }

    func transcribe(through core: CoreSession) async throws -> String {
        guard let path = captureURL else { return "" }
        capture?.stop()
        capture = nil
        captureURL = nil
        recording = false
        defer { try? FileManager.default.removeItem(at: path) }
        let audio = try Data(contentsOf: path)
        let result = try await core.bytes("/api/voice/transcribe", method: "POST", body: nil,
                                          timeout: 100, contentType: "audio/wav", raw: audio)
        let object = try JSONSerialization.jsonObject(with: result) as? [String: Any]
        return object?["text"] as? String ?? ""
    }

    func cancelRecording() {
        capture?.stop()
        capture = nil
        if let captureURL { try? FileManager.default.removeItem(at: captureURL) }
        captureURL = nil
        recording = false
    }
}

struct Avatar: View {
    @ObservedObject var voice: Voice
    let thinking: Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        TimelineView(.animation(minimumInterval: 1.0 / 30, paused: reduceMotion && !voice.playing)) { view in
            let time = view.date.timeIntervalSinceReferenceDate
            let lift = reduceMotion ? 0 : sin(time * 1.8) * 4
            let blink = !reduceMotion && time.truncatingRemainder(dividingBy: 5.7) < 0.14
            let mouth = voice.mouth(at: time)
            ZStack {
                Circle().fill(RadialGradient(colors: [.cyan.opacity(0.19), .clear], center: .center,
                                             startRadius: 12, endRadius: 135))
                Circle().stroke(.cyan.opacity(0.20), lineWidth: 1).padding(14)
                Circle().trim(from: 0.04, to: 0.27).stroke(.cyan.opacity(thinking ? 0.95 : 0.55),
                    style: StrokeStyle(lineWidth: 2, lineCap: .round))
                    .rotationEffect(.degrees(reduceMotion ? -70 : time.truncatingRemainder(dividingBy: 12) * 30))
                    .padding(10)
                Circle().stroke(.cyan.opacity(0.33), style: StrokeStyle(lineWidth: 1, dash: [1, 5])).padding(22)
                Canvas { context, size in
                    let scale = min(size.width, size.height) / 260
                    var g = context
                    g.translateBy(x: size.width / 2, y: size.height / 2 + lift)
                    g.scaleBy(x: scale, y: scale)
                    func plate(_ points: [CGPoint], _ color: Color) {
                        var path = Path()
                        path.addLines(points)
                        path.closeSubpath()
                        g.fill(path, with: .linearGradient(Gradient(colors: [color, Color(red: 0.02, green: 0.06, blue: 0.12)]),
                                                           startPoint: CGPoint(x: -58, y: -90), endPoint: CGPoint(x: 75, y: 100)))
                        g.stroke(path, with: .color(.cyan.opacity(0.45)), lineWidth: 1)
                    }
                    func point(_ x: Double, _ y: Double) -> CGPoint { CGPoint(x: x, y: y) }
                    let alloy = Color(red: 0.21, green: 0.36, blue: 0.49)
                    plate([point(-101, 98), point(-91, 65), point(-43, 45), point(41, 45),
                           point(92, 65), point(101, 98), point(66, 110), point(-66, 110)], alloy)
                    plate([point(-30, 59), point(-23, 34), point(23, 34), point(30, 59), point(0, 78)], alloy)
                    plate([point(-57, -43), point(-43, -74), point(-24, -87), point(24, -87),
                           point(43, -74), point(57, -43), point(48, 12), point(26, 42),
                           point(-26, 42), point(-48, 12)], alloy)
                    plate([point(-52, -40), point(-34, -55), point(34, -55), point(52, -40),
                           point(41, -17), point(-41, -17)], Color(red: 0.07, green: 0.18, blue: 0.27))
                    for x in [-27.0, 13.0] {
                        let eye = CGRect(x: x, y: -36, width: 14, height: blink ? 2 : 7)
                        g.fill(Path(roundedRect: eye, cornerRadius: 3), with: .color(.cyan))
                    }
                    let jaw = CGRect(x: -mouth.width / 2, y: 16, width: mouth.width, height: mouth.opening)
                    g.fill(Path(roundedRect: jaw, cornerRadius: 4), with: .color(.cyan.opacity(voice.playing ? 0.95 : 0.45)))
                    let core = Path(roundedRect: CGRect(x: -9, y: 86, width: 18, height: 13), cornerRadius: 3)
                    g.fill(core, with: .color(.cyan.opacity(0.9)))
                }
            }
        }
        .aspectRatio(1, contentMode: .fit)
        .accessibilityLabel(voice.playing ? "JARVIS sta parlando" : thinking ? "JARVIS sta elaborando" : "Avatar JARVIS")
    }
}
