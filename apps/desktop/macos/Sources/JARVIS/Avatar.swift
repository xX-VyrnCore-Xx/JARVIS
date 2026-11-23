import SwiftUI

struct Avatar: View {
    let thinking: Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        TimelineView(.animation(minimumInterval: 1.0 / 30, paused: reduceMotion)) { view in
            let time = view.date.timeIntervalSinceReferenceDate
            let lift = reduceMotion ? 0 : sin(time * 1.8) * 4
            let blink = !reduceMotion && time.truncatingRemainder(dividingBy: 5.7) < 0.14
            let mouth = thinking && !reduceMotion ? 2 + abs(sin(time * 10)) * 4 : 2
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
                    let jaw = CGRect(x: -12, y: 16, width: 24, height: mouth)
                    g.fill(Path(roundedRect: jaw, cornerRadius: 4), with: .color(.cyan.opacity(thinking ? 0.95 : 0.45)))
                    let core = Path(roundedRect: CGRect(x: -9, y: 86, width: 18, height: 13), cornerRadius: 3)
                    g.fill(core, with: .color(.cyan.opacity(0.9)))
                }
            }
        }
        .aspectRatio(1, contentMode: .fit)
        .accessibilityLabel(thinking ? "JARVIS sta elaborando" : "Avatar JARVIS")
    }
}
