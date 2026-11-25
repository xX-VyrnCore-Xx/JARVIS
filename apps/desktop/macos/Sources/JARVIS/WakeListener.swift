import AVFoundation
import AppKit
import Foundation
import Speech

/// Campioni a 16 kHz mono (16 bit) in un buffer circolare in memoria; scritto dal thread audio, letto dall'interfaccia.
final class SampleBuffer: @unchecked Sendable {
    static let rate = 16_000
    private let lock = NSLock()
    private var samples: [Int16] = []
    private var dropped = 0
    private let capacity = 16_000 * 40

    /// Numero totale di campioni ricevuti dall'avvio (indice assoluto del prossimo campione).
    var total: Int {
        lock.lock(); defer { lock.unlock() }
        return dropped + samples.count
    }

    func append(_ new: [Int16]) {
        lock.lock(); defer { lock.unlock() }
        samples.append(contentsOf: new)
        if samples.count > capacity {
            let extra = samples.count - capacity
            samples.removeFirst(extra)
            dropped += extra
        }
    }

    /// Campioni dall'indice assoluto `start` (o da quello più vecchio disponibile) fino alla fine.
    func slice(from start: Int) -> [Int16] {
        lock.lock(); defer { lock.unlock() }
        let offset = max(start - dropped, 0)
        return offset >= samples.count ? [] : Array(samples[offset...])
    }

    func last(seconds: Double) -> [Int16] {
        lock.lock(); defer { lock.unlock() }
        let count = min(Int(seconds * Double(Self.rate)), samples.count)
        return Array(samples.suffix(count))
    }

    func clear() {
        lock.lock(); defer { lock.unlock() }
        dropped += samples.count
        samples.removeAll(keepingCapacity: true)
    }
}

enum WakeAudio {
    /// Energia efficace di un blocco di campioni.
    static func rms(_ samples: [Int16]) -> Double {
        guard !samples.isEmpty else { return 0 }
        var sum = 0.0
        for value in samples { sum += Double(value) * Double(value) }
        return (sum / Double(samples.count)).squareRoot()
    }

    /// WAV PCM 16 bit mono a 16 kHz.
    static func wav(_ samples: [Int16]) -> Data {
        var data = Data()
        func put32(_ value: UInt32) { var v = value.littleEndian; withUnsafeBytes(of: &v) { data.append(contentsOf: $0) } }
        func put16(_ value: UInt16) { var v = value.littleEndian; withUnsafeBytes(of: &v) { data.append(contentsOf: $0) } }
        let bytes = UInt32(samples.count * 2)
        data.append(contentsOf: Array("RIFF".utf8)); put32(36 + bytes)
        data.append(contentsOf: Array("WAVEfmt ".utf8)); put32(16)
        put16(1); put16(1); put32(16_000); put32(32_000); put16(2); put16(16)
        data.append(contentsOf: Array("data".utf8)); put32(bytes)
        samples.withUnsafeBytes { data.append(contentsOf: $0) }
        return data
    }
}

/// Riceve i buffer audio del microfono e li passa alla richiesta di riconoscimento in corso (ADR-0183).
final class RecognitionBox: @unchecked Sendable {
    private let lock = NSLock()
    private var request: SFSpeechAudioBufferRecognitionRequest?

    func set(_ new: SFSpeechAudioBufferRecognitionRequest?) {
        lock.lock(); defer { lock.unlock() }
        request = new
    }

    func append(_ buffer: AVAudioPCMBuffer) {
        lock.lock(); defer { lock.unlock() }
        request?.append(buffer)
    }
}

/// Ultima trascrizione parziale di una sessione di riconoscimento, scritta dal sistema e letta dal ciclo di ascolto.
final class RecognitionState: @unchecked Sendable {
    private let lock = NSLock()
    private var text = ""
    private var changedAt = Date()
    private var finished = false
    private var failed = false

    func update(_ new: String, final: Bool) {
        lock.lock(); defer { lock.unlock() }
        if new != text {
            text = new
            changedAt = Date()
        }
        if final { finished = true }
    }

    func fail() {
        lock.lock(); defer { lock.unlock() }
        failed = true
    }

    func snapshot() -> (text: String, changedAt: Date, finished: Bool, failed: Bool) {
        lock.lock(); defer { lock.unlock() }
        return (text, changedAt, finished, failed)
    }
}

/// Riconoscimento della parola di attivazione nel testo trascritto dal sistema.
enum WakeText {
    private static let expression = try? NSRegularExpression(
        pattern: "(?:(?:ehi|ehy|hey|ok|ciao)\\s*,?\\s*)?(?:jarvis|giarvis|garvis|jervis|yarvis|charvis|giarvi|jarvi)\\b[\\s,.:!?-]*",
        options: [.caseInsensitive])

    /// `found` se compare «Hey Jarvis» (o una sua variante) e il testo che lo segue, già ripulito.
    static func split(_ text: String) -> (found: Bool, command: String) {
        let range = NSRange(text.startIndex..., in: text)
        guard let match = expression?.firstMatch(in: text, options: [], range: range),
              let swiftRange = Range(match.range, in: text) else { return (false, "") }
        let rest = String(text[swiftRange.upperBound...]).trimmingCharacters(in: .whitespacesAndNewlines)
        return (true, rest)
    }
}

/// Ascolto «Hey Jarvis» sempre attivo (ADR-0134).
///
/// In attesa il microfono alimenta solo un buffer in memoria: i blocchi con voce vanno al Core per un unico verdetto
/// (sì o no, nessun testo) e poi sono scartati. Solo dopo un sì il comando viene registrato, trascritto e inviato;
/// finito quello, si torna ad ascoltare soltanto la parola di attivazione. Nulla viene salvato su disco.
@MainActor
final class WakeListener: ObservableObject {
    enum Phase: Equatable { case off, denied, unavailable, waiting, capturing, working }

    @Published private(set) var phase: Phase = .off
    @Published private(set) var level: Double = 0
    @Published private(set) var detail = ""
    /// Motore in uso: `system` (riconoscimento del Mac, sul dispositivo) o `core` (modello del Core).
    @Published private(set) var engine = "core"

    /// Chiamato con il testo del comando dopo una parola di attivazione.
    var onCommand: ((String) -> Void)?
    /// Mentre JARVIS parla o elabora non ascolta: la sua voce non deve riattivarlo.
    var suspended = false

    private let engine = AVAudioEngine()
    private let buffer = SampleBuffer()
    private let recognition = RecognitionBox()
    private var loop: Task<Void, Never>?
    private weak var core: CoreSession?
    private var running = false

    private let windowSeconds = 2.0
    private let gate = 450.0
    private let silenceSeconds = 1.1

    var isActive: Bool { running }

    func start(core: CoreSession) {
        guard !running else { return }
        self.core = core
        running = true
        loop = Task { [weak self] in await self?.run() }
    }

    func stop() {
        running = false
        loop?.cancel()
        loop = nil
        if engine.isRunning {
            engine.inputNode.removeTap(onBus: 0)
            engine.stop()
        }
        buffer.clear()
        phase = .off
        level = 0
    }

    // MARK: Ciclo principale

    private func run() async {
        guard await ensurePermission() else {
            phase = .denied
            detail = "Consenti il microfono a JARVIS in Impostazioni di Sistema → Privacy e sicurezza."
            running = false
            return
        }
        do { try startEngine() } catch {
            phase = .unavailable
            detail = "Microfono non disponibile: \(error.localizedDescription)"
            running = false
            return
        }
        phase = .waiting
        detail = ""
        // Riconoscimento del Mac, sul dispositivo (ADR-0183): niente modelli da installare né audio fuori dal computer.
        if await runSystemRecognizer() { return }
        var lastChecked = buffer.total
        var retryAt = Date.distantPast
        while running, !Task.isCancelled {
            try? await Task.sleep(for: .milliseconds(400))
            guard running, let core else { break }
            level = min(WakeAudio.rms(buffer.last(seconds: 0.4)) / 6000, 1)
            if suspended {
                buffer.clear()
                lastChecked = buffer.total
                continue
            }
            guard phase == .waiting, Date() >= retryAt else { continue }
            let recent = buffer.last(seconds: windowSeconds)
            let fresh = buffer.total - lastChecked
            // Solo se c'è voce nuova: il silenzio non lascia mai il Mac.
            guard fresh >= SampleBuffer.rate / 2, WakeAudio.rms(buffer.last(seconds: 0.6)) > gate else { continue }
            lastChecked = buffer.total
            do {
                let verdict = try await core.postAudio("/api/voice/wake", wav: WakeAudio.wav(recent))
                detail = ""
                if verdict["detected"] as? Bool == true { await handleWake(core: core) }
            } catch {
                // Voce non installata o Core occupato: si riprova fra poco senza rumore.
                detail = error.localizedDescription
                retryAt = Date().addingTimeInterval(20)
            }
        }
    }

    // MARK: Riconoscimento del sistema (ADR-0183)

    private func ensureSpeechPermission() async -> Bool {
        switch SFSpeechRecognizer.authorizationStatus() {
        case .authorized: return true
        case .notDetermined:
            return await withCheckedContinuation { continuation in
                SFSpeechRecognizer.requestAuthorization { status in continuation.resume(returning: status == .authorized) }
            }
        default: return false
        }
    }

    /// Ascolta con il riconoscimento vocale del Mac, **solo sul dispositivo**: il testo vive in memoria per la durata
    /// della sessione (circa un minuto) e solo ciò che segue «Hey Jarvis» diventa un comando. Restituisce `false`
    /// (con il motivo in `detail`) se non è utilizzabile, e l'ascolto ripiega sul Core.
    private func runSystemRecognizer() async -> Bool {
        guard await ensureSpeechPermission() else {
            detail = "Consenti il riconoscimento vocale a JARVIS in Impostazioni di Sistema → Privacy e sicurezza → Riconoscimento vocale."
            return false
        }
        guard let recognizer = SFSpeechRecognizer(locale: Locale(identifier: "it-IT")), recognizer.isAvailable else {
            detail = "Il riconoscimento vocale italiano del Mac non è disponibile ora."
            return false
        }
        guard recognizer.supportsOnDeviceRecognition else {
            detail = "Per ascoltare senza inviare nulla ad Apple serve la dettatura in italiano sul Mac: attivala e scaricala in Impostazioni di Sistema → Tastiera → Dettatura."
            return false
        }
        detail = ""
        engine = "system"
        phase = .waiting
        while running, !Task.isCancelled {
            if suspended {
                recognition.set(nil)
                buffer.clear()
                try? await Task.sleep(for: .milliseconds(400))
                continue
            }
            await recognizeOnce(recognizer)
        }
        return true
    }

    /// Una sessione di riconoscimento: finisce con un comando, un errore, il limite di tempo o la sospensione.
    private func recognizeOnce(_ recognizer: SFSpeechRecognizer) async {
        let request = SFSpeechAudioBufferRecognitionRequest()
        request.shouldReportPartialResults = true
        request.requiresOnDeviceRecognition = true
        request.taskHint = .dictation
        let state = RecognitionState()
        let task = recognizer.recognitionTask(with: request) { result, error in
            if let result { state.update(result.bestTranscription.formattedString, final: result.isFinal) }
            if error != nil { state.fail() }
        }
        recognition.set(request)
        let began = Date()
        var wakeAt: Date?
        var command = ""
        while running, !Task.isCancelled, !suspended {
            try? await Task.sleep(for: .milliseconds(250))
            level = min(WakeAudio.rms(buffer.last(seconds: 0.3)) / 6000, 1)
            let snapshot = state.snapshot()
            if snapshot.failed || snapshot.finished || Date().timeIntervalSince(began) > 55 { break }
            let heard = WakeText.split(snapshot.text)
            guard heard.found else { continue }
            if wakeAt == nil {
                wakeAt = Date()
                phase = .capturing
                chime("Tink")
            }
            command = heard.command
            let quiet = Date().timeIntervalSince(snapshot.changedAt)
            let waited = Date().timeIntervalSince(wakeAt ?? Date())
            if (!command.isEmpty && quiet > 1.3) || (command.isEmpty && waited > 6) || waited > 15 { break }
        }
        recognition.set(nil)
        request.endAudio()
        task.cancel()
        buffer.clear()
        if wakeAt != nil, !command.isEmpty, running {
            phase = .working
            let lowered = command.lowercased().trimmingCharacters(in: .whitespacesAndNewlines.union(.punctuationCharacters))
            if ["stop", "fermati", "zitto", "basta"].contains(lowered) {
                _ = try? await core?.request("/api/voice/stop", method: "POST", body: [:])
            } else {
                onCommand?(command)
            }
        }
        if running { phase = .waiting }
        // Una breve pausa evita di riavviare di continuo una sessione che fallisce subito.
        try? await Task.sleep(for: .milliseconds(300))
    }

    private func handleWake(core: CoreSession) async {
        phase = .capturing
        chime("Tink")
        let start = max(buffer.total - Int(windowSeconds * Double(SampleBuffer.rate)), 0)
        await recordUntilSilence(from: start, minimum: 1.2, maximum: 12, waitForVoice: 0)
        phase = .working
        defer {
            if running { phase = .waiting }
            buffer.clear()
        }
        guard let result = try? await core.postAudio("/api/voice/transcribe?mode=wake_word", wav: WakeAudio.wav(buffer.slice(from: start))),
              let wake = result["wake"] as? [String: Any], wake["detected"] as? Bool == true else { return }
        var command = (result["normalized"] as? String) ?? (wake["command"] as? String) ?? ""
        var intent = result["intent"] as? String
        if command.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            // Solo «Hey Jarvis»: ora attende il comando vero e proprio.
            phase = .capturing
            chime("Pop")
            let next = buffer.total
            await recordUntilSilence(from: next, minimum: 0.8, maximum: 10, waitForVoice: 5)
            phase = .working
            guard let spoken = try? await core.postAudio("/api/voice/transcribe", wav: WakeAudio.wav(buffer.slice(from: next))) else { return }
            command = (spoken["normalized"] as? String) ?? (spoken["text"] as? String) ?? ""
            intent = spoken["intent"] as? String
        }
        let text = command.trimmingCharacters(in: .whitespacesAndNewlines)
        switch intent {
        case "stop": _ = try? await core.request("/api/voice/stop", method: "POST", body: [:])
        case "cancel": return
        default: if !text.isEmpty { onCommand?(text) }
        }
    }

    /// Registra finché non c'è silenzio dopo la voce, entro un massimo. `waitForVoice` è l'attesa dell'inizio della voce.
    private func recordUntilSilence(from start: Int, minimum: Double, maximum: Double, waitForVoice: Double) async {
        let began = Date()
        var lastVoice = Date()
        var heard = waitForVoice == 0
        while running, !Task.isCancelled {
            try? await Task.sleep(for: .milliseconds(200))
            let elapsed = Date().timeIntervalSince(began)
            level = min(WakeAudio.rms(buffer.last(seconds: 0.3)) / 6000, 1)
            if WakeAudio.rms(buffer.last(seconds: 0.3)) > gate {
                lastVoice = Date()
                heard = true
            }
            if elapsed >= maximum { return }
            if !heard, elapsed >= waitForVoice { return }
            if heard, elapsed >= minimum, Date().timeIntervalSince(lastVoice) >= silenceSeconds { return }
        }
    }

    // MARK: Microfono

    private func ensurePermission() async -> Bool {
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized: return true
        case .notDetermined: return await AVCaptureDevice.requestAccess(for: .audio)
        default: return false
        }
    }

    private func startEngine() throws {
        let input = engine.inputNode
        let format = input.outputFormat(forBus: 0)
        guard format.sampleRate > 0,
              let target = AVAudioFormat(commonFormat: .pcmFormatInt16, sampleRate: 16_000, channels: 1, interleaved: true),
              let converter = AVAudioConverter(from: format, to: target) else {
            throw CoreFailure(message: "formato audio non supportato")
        }
        let sink = buffer
        let speech = recognition
        input.installTap(onBus: 0, bufferSize: 4096, format: format) { pcm, _ in
            speech.append(pcm)
            let capacity = AVAudioFrameCount(Double(pcm.frameLength) * 16_000 / format.sampleRate) + 64
            guard let out = AVAudioPCMBuffer(pcmFormat: target, frameCapacity: capacity) else { return }
            var supplied = false
            var failure: NSError?
            converter.convert(to: out, error: &failure) { _, status in
                if supplied { status.pointee = .noDataNow; return nil }
                supplied = true
                status.pointee = .haveData
                return pcm
            }
            guard failure == nil, let channel = out.int16ChannelData else { return }
            sink.append(Array(UnsafeBufferPointer(start: channel[0], count: Int(out.frameLength))))
        }
        engine.prepare()
        try engine.start()
    }

    private func chime(_ name: String) {
        NSSound(named: NSSound.Name(name))?.play()
    }
}
