import SwiftUI

/// Avatar di JARVIS (ADR-0128): testa olografica interamente disegnata, con tre stati continui:
/// a riposo (respira, sbatte le palpebre, guarda intorno), in elaborazione (anelli veloci, arco di scansione,
/// occhi che cercano) e mentre parla (bocca a barre, onde che si propagano). Al passaggio del puntatore si
/// ingrandisce, al clic emette un'onda. Con «Riduci movimento» resta un disegno fermo.
struct Avatar: View {
    let thinking: Bool
    var speaking = false
    /// In ascolto dopo «Hey Jarvis» (ADR-0139): anelli che convergono e occhi attenti.
    var listening = false
    /// Livello del microfono (0–1): fa vibrare gli anelli in ascolto.
    var level = 0.0
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var hovering = false
    @State private var tapped: TimeInterval = -1_000

    var body: some View {
        AvatarCanvas(think: thinking ? 1 : 0, talk: speaking ? 1 : 0, listen: listening ? 1 : 0, level: level,
                     reduceMotion: reduceMotion, hovering: hovering, tapped: tapped)
            .animation(.easeInOut(duration: 0.7), value: thinking)
            .animation(.easeInOut(duration: 0.5), value: speaking)
            .animation(.easeInOut(duration: 0.4), value: listening)
            .scaleEffect(hovering && !reduceMotion ? 1.045 : 1)
            .animation(.spring(response: 0.45, dampingFraction: 0.55), value: hovering)
            .aspectRatio(1, contentMode: .fit)
            .contentShape(Circle())
            .onHover { hovering = $0 }
            .onTapGesture { tapped = Date().timeIntervalSinceReferenceDate }
            .accessibilityElement(children: .ignore)
            .accessibilityAddTraits(.isImage)
            .accessibilityLabel(speaking ? "JARVIS sta parlando" : (listening ? "JARVIS ti sta ascoltando" : (thinking ? "JARVIS sta elaborando" : "Avatar JARVIS")))
    }
}

/// Parte animabile: `think` e `talk` passano gradualmente da 0 a 1, così gli stati si fondono invece di scattare.
private struct AvatarCanvas: View, Animatable {
    var think: Double
    var talk: Double
    var listen: Double
    let level: Double
    let reduceMotion: Bool
    let hovering: Bool
    let tapped: TimeInterval

    var animatableData: AnimatablePair<AnimatablePair<Double, Double>, Double> {
        get { AnimatablePair(AnimatablePair(think, talk), listen) }
        set { think = newValue.first.first; talk = newValue.first.second; listen = newValue.second }
    }

    var body: some View {
        TimelineView(.animation(minimumInterval: 1.0 / 30, paused: reduceMotion)) { timeline in
            let now = timeline.date.timeIntervalSinceReferenceDate
            Canvas { context, size in
                AvatarArt.draw(context, size: size, now: now, think: think, talk: talk, listen: listen, level: level,
                               still: reduceMotion, hovering: hovering, tapped: tapped)
            }
        }
    }
}

private enum AvatarArt {
    static let cyan = Color(red: 0.25, green: 0.88, blue: 1.0)
    static let alloy = Color(red: 0.21, green: 0.36, blue: 0.49)
    static let visor = Color(red: 0.07, green: 0.18, blue: 0.27)
    static let night = Color(red: 0.02, green: 0.06, blue: 0.12)

    static func unit(_ value: Double) -> Double { value - floor(value) }
    static func hash(_ value: Double) -> Double { unit(sin(value * 127.1 + 311.7) * 43758.5453) }
    static func smooth(_ value: Double) -> Double { let c = min(max(value, 0), 1); return c * c * (3 - 2 * c) }

    static func point(_ x: Double, _ y: Double) -> CGPoint { CGPoint(x: x, y: y) }

    static func circle(_ radius: Double, at center: CGPoint = .zero) -> Path {
        Path(ellipseIn: CGRect(x: center.x - radius, y: center.y - radius, width: radius * 2, height: radius * 2))
    }

    static func arc(_ radius: Double, from start: Double, to end: Double) -> Path {
        var path = Path()
        path.addArc(center: .zero, radius: radius, startAngle: .degrees(start), endAngle: .degrees(end), clockwise: false)
        return path
    }

    static func polygon(_ points: [CGPoint]) -> Path {
        var path = Path()
        path.addLines(points)
        path.closeSubpath()
        return path
    }

    static let shoulders = [point(-101, 98), point(-91, 65), point(-43, 45), point(41, 45),
                            point(92, 65), point(101, 98), point(66, 110), point(-66, 110)]
    static let neck = [point(-30, 59), point(-23, 34), point(23, 34), point(30, 59), point(0, 78)]
    static let head = [point(-57, -43), point(-43, -74), point(-24, -87), point(24, -87),
                       point(43, -74), point(57, -43), point(48, 12), point(26, 42),
                       point(-26, 42), point(-48, 12)]
    static let eyeBand = [point(-52, -40), point(-34, -55), point(34, -55), point(52, -40),
                          point(41, -17), point(-41, -17)]

    // MARK: Disegno

    static func draw(_ context: GraphicsContext, size: CGSize, now: Double, think: Double, talk: Double,
                     listen: Double, level: Double, still: Bool, hovering: Bool, tapped: TimeInterval) {
        let t = still ? 0 : now
        let energy = 0.28 + 0.45 * think + 0.30 * talk + 0.22 * listen + (hovering ? 0.12 : 0)
        var g = context
        g.translateBy(x: size.width / 2, y: size.height / 2)
        let scale = min(size.width, size.height) / 270
        g.scaleBy(x: scale, y: scale)

        halo(&g, t: t, energy: energy)
        if !still { ripples(&g, t: t, now: now, energy: energy, talk: talk, think: think, tapped: tapped) }
        if listen > 0.02 { listeningRings(&g, t: still ? 0 : t, listen: listen, level: level) }
        rings(&g, t: t, energy: energy, think: think)
        particles(&g, t: t, energy: energy)

        // Testa: oscilla piano, si inclina appena, respira.
        let sway = still ? 0 : sin(t * 0.7) * 2.2
        let lift = still ? 0 : sin(t * 1.8) * 3.2 - talk * abs(sin(t * 9)) * 1.2
        let tilt = still ? 0 : sin(t * 0.5) * 1.6 + think * sin(t * 1.3) * 1.4
        var h = g
        h.translateBy(x: sway, y: lift)
        h.rotate(by: .degrees(tilt))

        let glitching = !still && unit(t / 9.3) < 0.0125
        if glitching {
            var ghost = h
            let shift = (hash(floor(t * 40)) - 0.5) * 14
            ghost.translateBy(x: shift, y: 0)
            ghost.opacity = 0.55
            ghost.stroke(polygon(head), with: .color(Color.pink.opacity(0.8)), lineWidth: 1.4)
            ghost.stroke(polygon(shoulders), with: .color(cyan.opacity(0.8)), lineWidth: 1.2)
        }

        body(&h)
        scan(&h, t: t, energy: energy)
        eyes(&h, t: t, think: think, talk: talk, listen: listen, hovering: hovering, still: still)
        mouth(&h, t: t, think: think, talk: talk)
        reactor(&h, t: t, energy: energy)
    }

    static func halo(_ g: inout GraphicsContext, t: Double, energy: Double) {
        let breath = 0.5 + 0.5 * sin(t * 1.4)
        g.fill(circle(140), with: .radialGradient(
            Gradient(colors: [cyan.opacity(0.10 + 0.18 * energy + 0.05 * breath), .clear]),
            center: .zero, startRadius: 8, endRadius: 140))
    }

    /// Onde che si propagano dal centro (più frequenti quando JARVIS lavora o parla) e onda al clic.
    static func ripples(_ g: inout GraphicsContext, t: Double, now: Double, energy: Double,
                        talk: Double, think: Double, tapped: TimeInterval) {
        for k in 0..<3 {
            let phase = unit(t * (0.18 + 0.5 * energy) + Double(k) / 3)
            let alpha = (1 - phase) * (0.05 + 0.20 * energy)
            g.stroke(circle(46 + 86 * phase), with: .color(cyan.opacity(alpha)), lineWidth: 1.2 + talk)
        }
        let age = now - tapped
        if age >= 0 && age < 1.4 {
            let p = age / 1.4
            g.stroke(circle(30 + 112 * smooth(p)), with: .color(Color.white.opacity((1 - p) * 0.8)), lineWidth: 3 * (1 - p) + 0.4)
            g.stroke(circle(20 + 100 * smooth(p)), with: .color(cyan.opacity((1 - p) * 0.9)), lineWidth: 2 * (1 - p) + 0.4)
        }
    }

    /// Anelli che convergono verso il volto mentre JARVIS ascolta; il livello del microfono ne aumenta lo spessore.
    static func listeningRings(_ g: inout GraphicsContext, t: Double, listen: Double, level: Double) {
        for k in 0..<4 {
            let phase = unit(t * 0.9 + Double(k) / 4)
            let radius = 132 - 84 * phase
            let alpha = (1 - abs(phase - 0.5) * 2) * 0.55 * listen
            g.stroke(circle(radius), with: .color(cyan.opacity(alpha)),
                     lineWidth: 1.2 + 3.5 * level * (1 - phase))
        }
        g.stroke(circle(46), with: .color(Color.white.opacity(0.35 * listen * (0.4 + level))), lineWidth: 1.2)
    }

    static func rings(_ g: inout GraphicsContext, t: Double, energy: Double, think: Double) {
        let spin = 1 + 2.2 * energy
        // Anello esterno tratteggiato.
        g.stroke(circle(128), with: .color(cyan.opacity(0.22 + 0.2 * energy)),
                 style: StrokeStyle(lineWidth: 1, dash: [2, 6], dashPhase: CGFloat(-t * 14 * spin)))
        // Archi che ruotano in senso opposto.
        for k in 0..<3 {
            let base = Double(k) * 120
            g.stroke(arc(120, from: base + t * 26 * spin, to: base + 46 + t * 26 * spin),
                     with: .color(cyan.opacity(0.5 + 0.4 * energy)),
                     style: StrokeStyle(lineWidth: 2.2, lineCap: .round))
        }
        for k in 0..<2 {
            let base = Double(k) * 180 + 30
            g.stroke(arc(111, from: base - t * 18 * spin, to: base + 70 - t * 18 * spin),
                     with: .color(Color.indigo.opacity(0.55 + 0.3 * energy)),
                     style: StrokeStyle(lineWidth: 2.6, lineCap: .round))
        }
        // Tacche con un'onda di luce che le percorre.
        let sweep = unit(t * 0.12 * spin) * 360
        let ticks = 72
        for i in 0..<ticks {
            let angle = Double(i) / Double(ticks) * 360
            var delta = abs(angle - sweep)
            if delta > 180 { delta = 360 - delta }
            let glow = max(0, 1 - delta / 50)
            let radians = angle * Double.pi / 180
            let inner = 101.0 - glow * 3
            let outer = 105.0 + glow * 4
            var tick = Path()
            tick.move(to: point(cos(radians) * inner, sin(radians) * inner))
            tick.addLine(to: point(cos(radians) * outer, sin(radians) * outer))
            g.stroke(tick, with: .color(cyan.opacity(0.18 + 0.75 * glow)), lineWidth: i % 6 == 0 ? 1.6 : 0.9)
        }
        // Arco di scansione veloce mentre elabora.
        if think > 0.02 {
            let start = t * 240
            g.stroke(arc(132, from: start, to: start + 64), with: .color(Color.white.opacity(0.85 * think)),
                     style: StrokeStyle(lineWidth: 3, lineCap: .round))
            g.stroke(arc(132, from: start + 180, to: start + 214), with: .color(cyan.opacity(0.9 * think)),
                     style: StrokeStyle(lineWidth: 2, lineCap: .round))
        }
    }

    /// Scintille in orbita: posizioni deterministiche, velocità e scintillio diversi per ognuna.
    static func particles(_ g: inout GraphicsContext, t: Double, energy: Double) {
        for i in 0..<42 {
            let seed = Double(i)
            let radius = 88 + 44 * hash(seed + 1)
            let speed = (0.04 + 0.10 * hash(seed + 2)) * (1 + 2 * energy) * (i % 2 == 0 ? 1 : -1)
            let angle = (hash(seed + 3) + t * speed) * 2 * Double.pi
            let twinkle = 0.25 + 0.75 * abs(sin(t * (0.8 + hash(seed + 4) * 1.6) + seed))
            let dot = 0.8 + 1.7 * hash(seed + 5)
            let center = point(cos(angle) * radius, sin(angle) * radius * 0.96)
            g.fill(circle(dot * 2.4, at: center), with: .color(cyan.opacity(0.07 * twinkle)))
            g.fill(circle(dot, at: center), with: .color(Color.white.opacity(0.35 + 0.5 * twinkle)))
        }
    }

    static func body(_ h: inout GraphicsContext) {
        let gradient = GraphicsContext.Shading.linearGradient(Gradient(colors: [alloy, night]),
                                                              startPoint: point(-58, -90), endPoint: point(75, 100))
        for outline in [shoulders, neck, head] {
            let path = polygon(outline)
            h.fill(path, with: gradient)
            h.stroke(path, with: .color(cyan.opacity(0.5)), lineWidth: 1)
        }
        let band = polygon(eyeBand)
        h.fill(band, with: .linearGradient(Gradient(colors: [visor, night]), startPoint: point(0, -55), endPoint: point(0, -17)))
        h.stroke(band, with: .color(cyan.opacity(0.35)), lineWidth: 1)
        // Nervature luminose sulla fronte e sulle guance.
        for side in [-1.0, 1.0] {
            var line = Path()
            line.move(to: point(side * 20, -86))
            line.addLine(to: point(side * 36, -66))
            line.addLine(to: point(side * 45, -58))
            h.stroke(line, with: .color(cyan.opacity(0.4)), lineWidth: 1)
            var cheek = Path()
            cheek.move(to: point(side * 44, -8))
            cheek.addLine(to: point(side * 30, 28))
            h.stroke(cheek, with: .color(cyan.opacity(0.3)), lineWidth: 1)
        }
    }

    /// Fascio di scansione che scende sul volto, ritagliato sulla sagoma della testa.
    static func scan(_ h: inout GraphicsContext, t: Double, energy: Double) {
        let sweep = unit(t * (0.16 + 0.35 * energy))
        let y = -92 + 140 * sweep
        h.drawLayer { layer in
            layer.clip(to: polygon(head))
            let glow = Path(CGRect(x: -70, y: y - 16, width: 140, height: 32))
            layer.fill(glow, with: .linearGradient(
                Gradient(colors: [.clear, cyan.opacity(0.08 + 0.26 * energy), .clear]),
                startPoint: point(0, y - 16), endPoint: point(0, y + 16)))
            layer.fill(Path(CGRect(x: -70, y: y - 0.6, width: 140, height: 1.2)),
                       with: .color(cyan.opacity(0.35 + 0.45 * energy)))
        }
    }

    static func eyes(_ h: inout GraphicsContext, t: Double, think: Double, talk: Double, listen: Double, hovering: Bool, still: Bool) {
        // Sguardo: piccoli scatti verso punti casuali; quando elabora cerca avanti e indietro.
        let interval = 1.7
        let segment = floor(t / interval)
        let ease = smooth((t / interval - segment) * interval / 0.18)
        var gx = (hash(segment - 1) - 0.5) * 7 * (1 - ease) + (hash(segment) - 0.5) * 7 * ease
        var gy = (hash(segment + 40) - 0.5) * 2.4
        if still { gx = 0; gy = 0 }
        gx = gx * (1 - think) + 7 * sin(t * 3.1) * think
        if hovering { gx *= 0.3; gy = -0.6 }
        // Palpebre.
        let phase = t.truncatingRemainder(dividingBy: 4.9)
        let blink = (!still && phase < 0.16) ? abs(phase - 0.08) / 0.08 : 1
        let open = 7 + hovering.asDouble * 1.5 + listen * 1.6 - think * 1.4
        let height = max(1.6, open * blink)
        let pulse = 0.82 + 0.18 * sin(t * (4 + 6 * think)) + talk * 0.05
        for x in [-27.0, 13.0] {
            let rect = CGRect(x: x + gx, y: -36 + gy + (open - height) / 2, width: 14, height: height)
            h.drawLayer { layer in
                layer.addFilter(.blur(radius: 5))
                layer.fill(Path(roundedRect: rect.insetBy(dx: -3, dy: -2), cornerRadius: 5), with: .color(cyan.opacity(0.75 * pulse)))
            }
            h.fill(Path(roundedRect: rect, cornerRadius: 3), with: .color(cyan.opacity(pulse)))
            h.fill(Path(roundedRect: rect.insetBy(dx: 3, dy: height * 0.28), cornerRadius: 1.5), with: .color(Color.white.opacity(0.85 * blink)))
        }
    }

    /// Bocca a barre: ferma a riposo, onda di luce quando elabora, sillabe quando parla.
    static func mouth(_ h: inout GraphicsContext, t: Double, think: Double, talk: Double) {
        let bars = 7
        for i in 0..<bars {
            let index = Double(i)
            let speech = abs(sin(t * 13 + index * 1.7)) * abs(sin(t * 5.3 + index * 0.9)) * 10
            let wave = (0.5 + 0.5 * sin(t * 6 - index * 0.9)) * 4
            let height = 2 + talk * speech + (1 - talk) * think * wave
            let envelope = 1 - abs(index - Double(bars - 1) / 2) / Double(bars) * 0.8
            let rect = CGRect(x: -13 + index * 4.2, y: 21 - height / 2 + 1, width: 2.8, height: height * envelope + 0.6)
            let alpha = 0.40 + 0.5 * max(think, talk)
            h.fill(Path(roundedRect: rect, cornerRadius: 1.4), with: .color(cyan.opacity(alpha)))
        }
    }

    /// Nucleo sul petto: pulsa, ruota e si accende con l'attività.
    static func reactor(_ h: inout GraphicsContext, t: Double, energy: Double) {
        let center = point(0, 92)
        let pulse = 0.5 + 0.5 * sin(t * (2 + 4 * energy))
        h.fill(circle(26, at: center), with: .radialGradient(
            Gradient(colors: [cyan.opacity(0.35 + 0.35 * pulse), .clear]), center: center, startRadius: 1, endRadius: 26))
        h.stroke(circle(10, at: center), with: .color(cyan.opacity(0.9)),
                 style: StrokeStyle(lineWidth: 1.4, dash: [3, 2.2], dashPhase: CGFloat(t * 6 * (1 + energy))))
        var triangle = Path()
        let spin = t * (1.4 + 2.6 * energy)
        for k in 0..<3 {
            let angle = spin + Double(k) * 2 * Double.pi / 3
            let p = point(center.x + cos(angle) * 6.5, center.y + sin(angle) * 6.5)
            if k == 0 { triangle.move(to: p) } else { triangle.addLine(to: p) }
        }
        triangle.closeSubpath()
        h.stroke(triangle, with: .color(Color.white.opacity(0.9)), lineWidth: 1.1)
        h.fill(circle(2.6 + pulse * 1.2, at: center), with: .color(Color.white))
    }
}

private extension Bool {
    var asDouble: Double { self ? 1 : 0 }
}
