import SwiftUI
import AppKit

/// Schede della Plancia (ADR-0181). Ogni scheda è autonoma: riceve i dati già letti e non conosce il resto
/// dell'app. Nessun dato personale è scritto qui: città, progetti e attività arrivano dal Core.
enum Mock {
    static let base = Color(red: 0.025, green: 0.047, blue: 0.083)
    static let panel = Color(red: 0.055, green: 0.10, blue: 0.16)
    static let stroke = Color(red: 0.25, green: 0.78, blue: 1.0)
    static let cyan = Color(red: 0.25, green: 0.88, blue: 1.0)
    static let violet = Color(red: 0.55, green: 0.42, blue: 1.0)
    static let amber = Color(red: 1.0, green: 0.72, blue: 0.25)
    static let green = Color(red: 0.30, green: 0.85, blue: 0.55)
}

private func text(_ value: Any?) -> String { value as? String ?? "" }
private func number(_ value: Any?) -> Double? {
    if let d = value as? Double { return d }
    if let i = value as? Int { return Double(i) }
    return nil
}
private func list(_ value: Any?) -> [[String: Any]] { value as? [[String: Any]] ?? [] }

/// Contenitore comune: pannello scuro con bordo luminoso e intestazione.
struct MockCard<Content: View>: View {
    let title: String
    let icon: String
    var trailing: AnyView? = nil
    @ViewBuilder var content: () -> Content

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 7) {
                Image(systemName: icon).font(.system(size: 12, weight: .semibold)).foregroundStyle(Mock.cyan)
                Text(title).font(.system(size: 13, weight: .semibold)).foregroundStyle(.white)
                Spacer(minLength: 4)
                if let trailing { trailing }
            }
            content()
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .topLeading)
        .background {
            RoundedRectangle(cornerRadius: 16)
                .fill(LinearGradient(colors: [Mock.panel.opacity(0.95), Mock.base.opacity(0.85)],
                                     startPoint: .topLeading, endPoint: .bottomTrailing))
        }
        .overlay { RoundedRectangle(cornerRadius: 16).strokeBorder(Mock.stroke.opacity(0.22)) }
        .shadow(color: Mock.cyan.opacity(0.06), radius: 10, y: 4)
    }
}

// MARK: - Dati

/// Legge i dati della Plancia dal Core e li tiene aggiornati.
@MainActor final class DashboardModel: ObservableObject {
    @Published var summary: [String: Any] = [:]
    @Published var weather: [String: Any] = [:]
    @Published var stats: [String: Any] = [:]
    @Published var cpuHistory: [Double] = []

    func refresh(_ core: CoreSession) async {
        if let value = try? await core.request("/api/dashboard/summary", timeout: 15) { summary = value }
        if let value = try? await core.request("/api/dashboard/weather", timeout: 20) { weather = value }
    }

    func sampleSystem(_ core: CoreSession) async {
        if let value = try? await core.request("/api/system/stats", timeout: 10) {
            stats = value
            if let cpu = number(value["cpu_percent"]) {
                cpuHistory.append(cpu)
                if cpuHistory.count > 48 { cpuHistory.removeFirst(cpuHistory.count - 48) }
            }
        }
    }

    func setWeatherPlace(_ core: CoreSession, _ place: String?) async {
        let value: Any = place ?? NSNull()
        _ = try? await core.request("/api/dashboard", method: "PATCH", body: ["weather_place": value], timeout: 15)
        await refresh(core)
    }
}

// MARK: - Specialisti attivi

struct ActiveAgentsCard: View {
    let items: [[String: Any]]

    private func title(_ name: String) -> (String, String) {
        switch name {
        case "general_assistant": return ("Generalista", "person.crop.circle")
        case "research": return ("Ricerca", "magnifyingglass")
        case "coding": return ("Codice", "chevron.left.forwardslash.chevron.right")
        case "system": return ("Sistema", "desktopcomputer")
        case "automation": return ("Automazioni", "bolt")
        case "memory": return ("Memoria", "brain")
        case "business": return ("Azienda", "briefcase")
        case "monitoring": return ("Sicurezza", "shield.lefthalf.filled")
        default: return (name, "circle")
        }
    }

    private var rows: [(String, String, Bool)] {
        var out: [(String, String, Bool)] = [("Generalista", "person.crop.circle", true)]
        for item in items {
            let pair = title(text(item["agent"]))
            out.append((pair.0, pair.1, text(item["state"]) == "attivo"))
        }
        return out
    }

    var body: some View {
        MockCard(title: "Agenti attivi", icon: "person.3.fill",
                 trailing: AnyView(PulsingDot(color: Mock.green, size: 6))) {
            VStack(spacing: 7) {
                ForEach(Array(rows.enumerated()), id: \.offset) { _, row in
                    HStack(spacing: 9) {
                        Image(systemName: row.1).font(.system(size: 12)).foregroundStyle(Mock.cyan).frame(width: 18)
                        Text(row.0).font(.system(size: 12.5)).foregroundStyle(.white.opacity(0.92))
                        Spacer(minLength: 4)
                        Image(systemName: row.2 ? "checkmark" : "pause.fill")
                            .font(.system(size: 10, weight: .bold))
                            .foregroundStyle(row.2 ? Mock.green : Mock.amber)
                    }
                    .accessibilityElement(children: .combine)
                    .accessibilityLabel("\(row.0): \(row.2 ? "attivo" : "in pausa")")
                }
            }
        }
    }
}

// MARK: - Meteo (facoltativo)

struct WeatherCard: View {
    let weather: [String: Any]
    let configured: Bool
    let setPlace: (String?) -> Void
    @State private var draft = ""

    private func symbol(_ sky: String) -> String {
        let s = sky.lowercased()
        if s.contains("temporal") { return "cloud.bolt.rain.fill" }
        if s.contains("neve") { return "snowflake" }
        if s.contains("piogg") || s.contains("acquazz") { return "cloud.rain.fill" }
        if s.contains("neb") { return "cloud.fog.fill" }
        if s.contains("nuvol") || s.contains("coperto") { return "cloud.sun.fill" }
        return "sun.max.fill"
    }

    var body: some View {
        MockCard(title: configured ? text(weather["place"]) : "Meteo", icon: "location.fill") {
            if configured, let temperature = number(weather["temperature_c"]) {
                HStack(alignment: .center, spacing: 10) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text("\(Int(temperature.rounded()))°C")
                            .font(.system(size: 30, weight: .bold, design: .rounded)).foregroundStyle(.white)
                        Text(text(weather["sky"]).capitalized).font(.system(size: 11)).foregroundStyle(.secondary)
                    }
                    Spacer()
                    VStack(alignment: .trailing, spacing: 4) {
                        Image(systemName: symbol(text(weather["sky"])))
                            .font(.system(size: 28)).symbolRenderingMode(.multicolor)
                        Text("↑\(Int((number(weather["max_c"]) ?? 0).rounded()))°  ↓\(Int((number(weather["min_c"]) ?? 0).rounded()))°")
                            .font(.system(size: 10.5)).foregroundStyle(.secondary)
                    }
                }
                Button("Cambia o togli la città") { setPlace(nil) }
                    .buttonStyle(.plain).font(.system(size: 10.5)).foregroundStyle(Mock.cyan.opacity(0.8))
            } else if configured {
                Text("Meteo non disponibile in questo momento.").font(.system(size: 11.5)).foregroundStyle(.secondary)
                Button("Togli la città") { setPlace(nil) }
                    .buttonStyle(.plain).font(.system(size: 10.5)).foregroundStyle(Mock.cyan.opacity(0.8))
            } else {
                Text("Facoltativo: la città scelta viene inviata a Open-Meteo per leggere il meteo.")
                    .font(.system(size: 11)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                HStack(spacing: 6) {
                    TextField("Città", text: $draft).textFieldStyle(.roundedBorder).font(.system(size: 12))
                        .onSubmit { commit() }
                    Button("Attiva") { commit() }
                        .disabled(draft.trimmingCharacters(in: .whitespaces).isEmpty)
                }
            }
        }
    }

    private func commit() {
        let value = draft.trimmingCharacters(in: .whitespaces)
        guard !value.isEmpty else { return }
        setPlace(value)
        draft = ""
    }
}

// MARK: - Progetti in corso

struct ProjectsCard: View {
    let projects: [[String: Any]]
    private let tints: [Color] = [Mock.cyan, Mock.violet, Mock.amber, Mock.green]

    var body: some View {
        MockCard(title: "Progetti in corso", icon: "folder.fill") {
            if projects.isEmpty {
                Text("Nessun piano in corso. Chiedi a Yggdrasil di organizzare un lavoro in passi.")
                    .font(.system(size: 11.5)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            }
            ForEach(Array(projects.enumerated()), id: \.offset) { index, project in
                let percent = number(project["percent"]) ?? 0
                let tint = tints[index % tints.count]
                VStack(alignment: .leading, spacing: 5) {
                    HStack(spacing: 8) {
                        Image(systemName: "square.stack.3d.up.fill").font(.system(size: 11)).foregroundStyle(tint)
                        Text(text(project["name"])).font(.system(size: 12, weight: .medium)).lineLimit(1)
                            .foregroundStyle(.white.opacity(0.95))
                        Spacer(minLength: 4)
                        Text("\(Int(percent))%").font(.system(size: 11.5, weight: .semibold).monospacedDigit())
                            .foregroundStyle(.white.opacity(0.9))
                    }
                    GeometryReader { geometry in
                        ZStack(alignment: .leading) {
                            Capsule().fill(Color.white.opacity(0.09))
                            Capsule().fill(LinearGradient(colors: [tint.opacity(0.7), tint],
                                                          startPoint: .leading, endPoint: .trailing))
                                .frame(width: geometry.size.width * CGFloat(min(max(percent / 100, 0), 1)))
                        }
                    }
                    .frame(height: 5)
                }
                .accessibilityElement(children: .combine)
                .accessibilityLabel("\(text(project["name"])), \(Int(percent)) per cento")
            }
        }
    }
}

// MARK: - Stato del sistema

struct RingGauge: View {
    let value: Double?
    let color: Color
    let label: String

    private var fraction: CGFloat { CGFloat(min(max((value ?? 0) / 100, 0), 1)) }

    var body: some View {
        ZStack {
            Circle().stroke(Color.white.opacity(0.10), lineWidth: 6)
            Circle().trim(from: 0, to: fraction)
                .stroke(color, style: StrokeStyle(lineWidth: 6, lineCap: .round))
                .rotationEffect(.degrees(-90))
                .shadow(color: color.opacity(0.6), radius: 4)
            VStack(spacing: 0) {
                Text(label).font(.system(size: 8.5, weight: .medium)).foregroundStyle(.secondary)
                Text(value.map { "\(Int($0.rounded()))%" } ?? "—")
                    .font(.system(size: 13, weight: .bold, design: .rounded)).foregroundStyle(.white)
            }
        }
        .frame(width: 62, height: 62)
        .animation(.easeOut(duration: 0.8), value: value)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(label) \(value.map { "\(Int($0.rounded())) per cento" } ?? "non disponibile")")
    }
}

struct SystemStatusCard: View {
    let stats: [String: Any]
    let coreUp: Bool
    let detail: String

    var body: some View {
        let cpu = number(stats["cpu_percent"])
        let memory = number(stats["memory_percent"])
        let disk = number(stats["disk_percent"])
        let busy = [cpu, memory, disk].compactMap { $0 }.contains { $0 >= 90 }
        let label = coreUp ? (busy ? "Sotto carico" : "Operativo") : "Non disponibile"
        let tint = coreUp ? (busy ? Mock.amber : Mock.green) : Color.red
        return MockCard(title: "Stato sistema", icon: "gauge.with.dots.needle.67percent",
                        trailing: AnyView(HStack(spacing: 5) {
                            PulsingDot(color: tint, size: 5)
                            Text(label).font(.system(size: 10.5, weight: .semibold)).foregroundStyle(tint)
                        })) {
            HStack(spacing: 12) {
                RingGauge(value: cpu, color: Mock.cyan, label: "CPU")
                RingGauge(value: memory, color: Mock.violet, label: "RAM")
                RingGauge(value: disk, color: Mock.green, label: "SSD")
            }
            .frame(maxWidth: .infinity)
            Text(detail).font(.system(size: 10.5)).foregroundStyle(.secondary).lineLimit(1)
        }
    }
}

// MARK: - Attività di oggi

struct TodayCard: View {
    let items: [[String: Any]]
    let add: () -> Void

    var body: some View {
        MockCard(title: "Attività di oggi", icon: "checklist",
                 trailing: AnyView(Button(action: add) {
                     Image(systemName: "plus").font(.system(size: 11, weight: .bold))
                 }.buttonStyle(.plain).foregroundStyle(Mock.cyan).accessibilityLabel("Aggiungi un'attività"))) {
            if items.isEmpty {
                Text("Nessuna cosa da fare. Dì «ricordami di…» per aggiungerne una.")
                    .font(.system(size: 11.5)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            }
            VStack(alignment: .leading, spacing: 8) {
                ForEach(Array(items.enumerated()), id: \.offset) { _, item in
                    let done = !(item["done_at"] is NSNull) && item["done_at"] != nil
                    HStack(spacing: 8) {
                        Image(systemName: done ? "checkmark.circle.fill" : "circle")
                            .font(.system(size: 14)).foregroundStyle(done ? Mock.green : Color.white.opacity(0.45))
                        Text(text(item["text"])).font(.system(size: 12)).lineLimit(1)
                            .foregroundStyle(.white.opacity(done ? 0.6 : 0.92))
                            .strikethrough(done, color: .white.opacity(0.4))
                    }
                    .accessibilityElement(children: .combine)
                    .accessibilityLabel("\(text(item["text"])), \(done ? "fatto" : "da fare")")
                }
            }
        }
    }
}

// MARK: - Notifiche

struct NotificationsCard: View {
    let unread: Int
    let items: [[String: Any]]

    private func tint(_ severity: String) -> Color {
        switch severity {
        case "success": return Mock.green
        case "warning": return Mock.amber
        case "error", "critical": return Color.red
        default: return Mock.violet
        }
    }

    private func time(_ iso: String) -> String {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        let plain = ISO8601DateFormatter()
        guard let date = formatter.date(from: iso) ?? plain.date(from: iso) else { return "" }
        let out = DateFormatter()
        out.dateFormat = "HH:mm"
        return out.string(from: date)
    }

    var body: some View {
        MockCard(title: "Notifiche", icon: "bell.fill",
                 trailing: AnyView(Group {
                     if unread > 0 {
                         Text("\(unread)").font(.system(size: 10.5, weight: .bold)).foregroundStyle(.white)
                             .padding(.horizontal, 7).padding(.vertical, 2)
                             .background(Color.red, in: Capsule())
                     }
                 })) {
            if items.isEmpty {
                Text("Nessuna notifica recente.").font(.system(size: 11.5)).foregroundStyle(.secondary)
            }
            VStack(alignment: .leading, spacing: 8) {
                ForEach(Array(items.enumerated()), id: \.offset) { _, item in
                    HStack(spacing: 8) {
                        RoundedRectangle(cornerRadius: 3).fill(tint(text(item["severity"]))).frame(width: 12, height: 12)
                        Text(text(item["title"])).font(.system(size: 11.5)).lineLimit(1).foregroundStyle(.white.opacity(0.92))
                        Spacer(minLength: 4)
                        Text(time(text(item["created_at"]))).font(.system(size: 10.5).monospacedDigit())
                            .foregroundStyle(.secondary)
                    }
                    .accessibilityElement(children: .combine)
                }
            }
        }
    }
}

// MARK: - Monitoraggio

struct Sparkline: View {
    let values: [Double]

    private func path(_ size: CGSize, closed: Bool) -> Path {
        var p = Path()
        guard values.count > 1 else { return p }
        let top = max(values.max() ?? 1, 10)
        let step = size.width / CGFloat(values.count - 1)
        for (i, v) in values.enumerated() {
            let point = CGPoint(x: CGFloat(i) * step, y: size.height * (1 - CGFloat(v / top)) * 0.9 + 2)
            if i == 0 { p.move(to: point) } else { p.addLine(to: point) }
        }
        if closed {
            p.addLine(to: CGPoint(x: size.width, y: size.height))
            p.addLine(to: CGPoint(x: 0, y: size.height))
            p.closeSubpath()
        }
        return p
    }

    var body: some View {
        GeometryReader { geometry in
            ZStack {
                path(geometry.size, closed: true)
                    .fill(LinearGradient(colors: [Mock.violet.opacity(0.45), Mock.violet.opacity(0.02)],
                                         startPoint: .top, endPoint: .bottom))
                path(geometry.size, closed: false)
                    .stroke(LinearGradient(colors: [Mock.cyan, Mock.violet], startPoint: .leading, endPoint: .trailing),
                            style: StrokeStyle(lineWidth: 1.6, lineCap: .round, lineJoin: .round))
            }
        }
        .accessibilityHidden(true)
    }
}

struct MonitoringCard: View {
    let history: [Double]
    let monitoring: [String: Any]

    private func uptime(_ seconds: Double?) -> String {
        guard let seconds else { return "—" }
        let s = Int(seconds)
        if s >= 86_400 { return "\(s / 86_400) g \((s % 86_400) / 3_600) h" }
        if s >= 3_600 { return "\(s / 3_600) h \((s % 3_600) / 60) min" }
        return "\(max(s / 60, 0)) min"
    }

    private func kpi(_ title: String, _ value: String, _ tint: Color) -> some View {
        VStack(alignment: .leading, spacing: 1) {
            Text(title).font(.system(size: 9.5)).foregroundStyle(.secondary)
            Text(value).font(.system(size: 14, weight: .bold, design: .rounded)).foregroundStyle(tint)
        }
    }

    var body: some View {
        let ops = number(monitoring["operations"]).map { Int($0) }
        let success = number(monitoring["tool_success_percent"])
        return MockCard(title: "Monitoraggio", icon: "chart.xyaxis.line") {
            Sparkline(values: history).frame(height: 44)
            HStack(alignment: .top) {
                kpi("Attivo da", uptime(number(monitoring["uptime_s"])), Mock.green)
                Spacer()
                kpi("Operazioni", ops.map { "\($0)" } ?? "—", Mock.cyan)
                Spacer()
                kpi("Esito strumenti", success.map { "\(Int($0.rounded()))%" } ?? "—", Mock.violet)
            }
        }
    }
}

// MARK: - Media (facoltativo, con consenso)

/// Legge e comanda Spotify con AppleScript: è l'unico servizio musicale collegato (ADR-0195). Parte solo dopo il
/// consenso dell'utente (macOS chiede a sua volta l'autorizzazione all'automazione): senza, nessuno script viene
/// eseguito, e Spotify non viene mai avviato da qui.
@MainActor final class NowPlaying: ObservableObject {
    @Published var title = ""
    @Published var artist = ""
    @Published var playing = false
    @Published var enabled: Bool = UserDefaults.standard.bool(forKey: "mediaControlEnabled")

    func enable() {
        UserDefaults.standard.set(true, forKey: "mediaControlEnabled")
        enabled = true
        refresh()
    }

    func disable() {
        UserDefaults.standard.set(false, forKey: "mediaControlEnabled")
        enabled = false
        title = ""
        artist = ""
        playing = false
    }

    private func apply(_ parts: [String]) {
        if parts.count >= 4 {
            playing = parts[1] == "playing"
            title = parts[2]
            artist = parts[3]
        } else {
            title = ""
            artist = ""
            playing = false
        }
    }

    func refresh() {
        guard enabled else { return }
        Task.detached(priority: .utility) {
            var error: NSDictionary?
            let value = NSAppleScript(source: nowPlayingReadScript)?.executeAndReturnError(&error).stringValue ?? ""
            let parts = value.components(separatedBy: "|")
            await MainActor.run { self.apply(parts) }
        }
    }

    func command(_ verb: String) {
        guard enabled else { return }
        // Solo i tre comandi della tavolozza fissa: nessun testo esterno entra nello script.
        guard ["previous track", "playpause", "next track"].contains(verb) else { return }
        Task.detached(priority: .userInitiated) {
            var error: NSDictionary?
            let script = "if application \"Spotify\" is running then tell application \"Spotify\" to \(verb)"
            _ = NSAppleScript(source: script)?.executeAndReturnError(&error)
            try? await Task.sleep(for: .milliseconds(400))
            await MainActor.run { self.refresh() }
        }
    }
}

/// Lettura del brano in ascolto: solo Spotify, e solo se è già aperto (non lo avvia).
private let nowPlayingReadScript = """
    set out to ""
    tell application "System Events" to set names to name of every application process
    if names contains "Spotify" then
        tell application "Spotify"
            if player state is playing or player state is paused then
                set out to "Spotify|" & (player state as string) & "|" & (name of current track) & "|" & (artist of current track)
            end if
        end tell
    end if
    return out
    """

struct MediaCard: View {
    @ObservedObject var media: NowPlaying

    var body: some View {
        MockCard(title: "Spotify", icon: "music.note") {
            if !media.enabled {
                Text("Facoltativo: Yggdrasil legge il brano in ascolto su Spotify, l'unico servizio musicale collegato. macOS chiederà l'autorizzazione.")
                    .font(.system(size: 11)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                Button("Attiva il controllo media") { media.enable() }
            } else {
                HStack(spacing: 10) {
                    Image(systemName: "waveform").font(.system(size: 20)).foregroundStyle(Mock.cyan)
                        .frame(width: 40, height: 40)
                        .background(Mock.cyan.opacity(0.10), in: RoundedRectangle(cornerRadius: 9))
                    VStack(alignment: .leading, spacing: 2) {
                        Text(media.title.isEmpty ? "Nessun brano" : media.title)
                            .font(.system(size: 12.5, weight: .semibold)).lineLimit(1).foregroundStyle(.white)
                        Text(media.artist.isEmpty ? "Apri Spotify" : media.artist)
                            .font(.system(size: 11)).lineLimit(1).foregroundStyle(.secondary)
                    }
                    Spacer(minLength: 0)
                }
                HStack(spacing: 22) {
                    Button { media.command("previous track") } label: { Image(systemName: "backward.fill") }
                        .accessibilityLabel("Brano precedente")
                    Button { media.command("playpause") } label: {
                        Image(systemName: media.playing ? "pause.fill" : "play.fill")
                            .font(.system(size: 15, weight: .bold))
                            .frame(width: 34, height: 34)
                            .background(Color.white.opacity(0.14), in: Circle())
                    }
                    .accessibilityLabel(media.playing ? "Pausa" : "Riproduci")
                    Button { media.command("next track") } label: { Image(systemName: "forward.fill") }
                        .accessibilityLabel("Brano successivo")
                    Spacer()
                    Button("Disattiva") { media.disable() }.font(.system(size: 10.5))
                }
                .buttonStyle(.plain).foregroundStyle(.white.opacity(0.9))
            }
        }
        .task {
            while !Task.isCancelled {
                media.refresh()
                try? await Task.sleep(for: .seconds(4))
            }
        }
    }
}
