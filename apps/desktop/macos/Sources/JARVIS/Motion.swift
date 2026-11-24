import Foundation
import SwiftUI

// Animazioni di JARVIS (ADR-0125). Con «Riduci movimento» attivo in macOS restano soltanto dissolvenze brevi:
// niente movimenti, rimbalzi o animazioni continue.

/// Pulsante che si abbassa leggermente quando viene premuto.
struct PressableStyle: ButtonStyle {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .scaleEffect(configuration.isPressed && !reduceMotion ? 0.97 : 1)
            .opacity(configuration.isPressed ? 0.88 : 1)
            .animation(.spring(response: 0.25, dampingFraction: 0.7), value: configuration.isPressed)
    }
}

/// Punto di stato con un'onda che si espande piano, per dire «attivo».
struct PulsingDot: View {
    let color: Color
    var size: CGFloat = 8
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var pulsing = false

    var body: some View {
        ZStack {
            Circle().fill(color.opacity(0.35))
                .frame(width: size * 2.4, height: size * 2.4)
                .scaleEffect(pulsing ? 1 : 0.4)
                .opacity(pulsing ? 0 : 0.8)
            Circle().fill(color).frame(width: size, height: size)
        }
        .frame(width: size * 2.4, height: size * 2.4)
        .onAppear {
            guard !reduceMotion else { return }
            withAnimation(.easeOut(duration: 1.7).repeatForever(autoreverses: false)) { pulsing = true }
        }
        .accessibilityHidden(true)
    }
}

/// Tre punti che rimbalzano mentre JARVIS elabora.
struct TypingIndicator: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        TimelineView(.animation(minimumInterval: 1.0 / 30, paused: reduceMotion)) { context in
            let time = context.date.timeIntervalSinceReferenceDate
            HStack(spacing: 5) {
                ForEach(0..<3, id: \.self) { index in
                    let wave = reduceMotion ? 0.5 : max(0, sin(time * 6 - Double(index) * 0.9))
                    Circle().fill(Color.cyan)
                        .frame(width: 7, height: 7)
                        .offset(y: -3.5 * CGFloat(wave))
                        .opacity(0.45 + 0.55 * wave)
                }
            }
        }
        .accessibilityLabel("JARVIS sta elaborando")
    }
}

/// Bagliore lento e morbido dietro l'intestazione della Plancia.
struct AuroraGlow: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        TimelineView(.animation(minimumInterval: 1.0 / 24, paused: reduceMotion)) { context in
            let t = reduceMotion ? 0 : context.date.timeIntervalSinceReferenceDate / 9
            GeometryReader { geometry in
                ZStack {
                    Circle().fill(Color.cyan.opacity(0.20))
                        .frame(width: geometry.size.width * 0.5)
                        .blur(radius: 60)
                        .offset(x: CGFloat(sin(t)) * geometry.size.width * 0.18,
                                y: CGFloat(cos(t * 0.8)) * geometry.size.height * 0.2)
                    Circle().fill(Color.indigo.opacity(0.18))
                        .frame(width: geometry.size.width * 0.42)
                        .blur(radius: 70)
                        .offset(x: CGFloat(cos(t * 0.7)) * geometry.size.width * 0.25,
                                y: CGFloat(sin(t * 1.1)) * geometry.size.height * 0.22)
                }
                .frame(width: geometry.size.width, height: geometry.size.height)
            }
        }
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}

/// Elemento che compare con una leggera salita e dissolvenza, ritardato in base all'ordine.
struct StaggeredAppear: ViewModifier {
    let index: Int
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var shown = false

    func body(content: Content) -> some View {
        content
            .opacity(shown ? 1 : 0)
            .offset(y: shown || reduceMotion ? 0 : 14)
            .onAppear {
                withAnimation(reduceMotion
                              ? .easeOut(duration: 0.15)
                              : .spring(response: 0.5, dampingFraction: 0.82).delay(Double(index) * 0.05)) {
                    shown = true
                }
            }
    }
}

/// Alza appena l'elemento e ne accende il bordo quando il puntatore è sopra.
struct HoverLift: ViewModifier {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var hovering = false

    func body(content: Content) -> some View {
        content
            .scaleEffect(hovering && !reduceMotion ? 1.02 : 1)
            .shadow(color: Color.cyan.opacity(hovering ? 0.28 : 0), radius: hovering ? 10 : 0, y: 3)
            .animation(.spring(response: 0.3, dampingFraction: 0.75), value: hovering)
            .onHover { hovering = $0 }
    }
}

extension View {
    func staggeredAppear(_ index: Int = 0) -> some View { modifier(StaggeredAppear(index: index)) }
    func hoverLift() -> some View { modifier(HoverLift()) }
}

/// Barre di un'onda sonora: lente a riposo, vivaci mentre JARVIS lavora.
struct WaveformView: View {
    var active: Bool
    var barCount = 24
    var color: Color = .cyan
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        TimelineView(.animation(minimumInterval: 1.0 / 24, paused: reduceMotion)) { context in
            let time = context.date.timeIntervalSinceReferenceDate
            HStack(alignment: .center, spacing: 3) {
                ForEach(0..<barCount, id: \.self) { index in
                    let envelope = sin(Double(index) / Double(max(barCount - 1, 1)) * Double.pi)
                    let wave = reduceMotion ? 0.35 : 0.5 + 0.5 * sin(time * (active ? 7 : 2.2) + Double(index) * 0.55)
                    let level = 0.15 + envelope * (active ? 0.85 : 0.45) * wave
                    Capsule().fill(color.opacity(0.35 + 0.65 * level))
                        .frame(width: 3, height: 4 + 30 * CGFloat(level))
                }
            }
            .frame(height: 38)
        }
        .animation(.easeInOut(duration: 0.4), value: active)
        .accessibilityHidden(true)
    }
}

/// Anelli che ruotano piano intorno all'avatar.
struct OrbitRings: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        TimelineView(.animation(minimumInterval: 1.0 / 30, paused: reduceMotion)) { context in
            let time = reduceMotion ? 0 : context.date.timeIntervalSinceReferenceDate
            ZStack {
                Circle().trim(from: 0.05, to: 0.30)
                    .stroke(LinearGradient(colors: [Color.cyan, Color.blue.opacity(0.1)], startPoint: .leading, endPoint: .trailing),
                            style: StrokeStyle(lineWidth: 3, lineCap: .round))
                    .rotationEffect(.degrees(time * 24))
                Circle().trim(from: 0.55, to: 0.80)
                    .stroke(Color.indigo.opacity(0.7), style: StrokeStyle(lineWidth: 3, lineCap: .round))
                    .rotationEffect(.degrees(-time * 16))
                    .padding(14)
                Circle().stroke(Color.cyan.opacity(0.18), lineWidth: 1).padding(6)
            }
        }
        .accessibilityHidden(true)
    }
}
