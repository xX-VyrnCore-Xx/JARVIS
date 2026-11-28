import AppKit
import SceneKit
import SwiftUI
import simd

// Yggdrasil (ADR-0232): l'albero dei nove regni come menu in 3D.
// Ogni regno raccoglie un gruppo di funzioni dell'app. Questo file non conosce le pagine dell'app: riceve dal
// chiamante l'elenco delle funzioni di ciascun regno e restituisce quella scelta.

// MARK: - Regni

/// Un regno: nome, epiteto, colore, simbolo e posizione sull'anello davanti all'albero.
struct Realm: Identifiable {
    let id: String
    let name: String
    /// Titolo breve, come sotto il nome nel quadro dei regni.
    let epithet: String
    /// Cosa contiene il regno, in una riga.
    let summary: String
    let icon: String
    let rgb: SIMD3<Double>
    /// Posizione sul grande anello frontale, in gradi (90 = in alto, senso orario verso destra).
    let angle: Float

    var color: Color { Color(red: rgb.x, green: rgb.y, blue: rgb.z) }
    var nsColor: NSColor { NSColor(red: rgb.x, green: rgb.y, blue: rgb.z, alpha: 1) }

    /// Centro e semiassi dell'anello su cui stanno i regni, davanti all'albero (come un quadro frontale).
    static let ringCenter: Float = 7.8
    static let ringX: Float = 9.2
    static let ringY: Float = 6.9

    /// I nove regni sull'anello, in senso orario dall'alto: Asgard al vertice, Midgard in basso.
    static let all: [Realm] = [
        Realm(id: "asgard", name: "Asgard", epithet: "Controllo & Comando",
              summary: "Plancia, chat, attività e conferme: dai gli ordini e approvi", icon: "crown",
              rgb: SIMD3(1.00, 0.80, 0.32), angle: 90),
        Realm(id: "alfheim", name: "Alfheim", epithet: "Intelligenza & Conoscenza",
              summary: "Modelli AI, agenti e dataset", icon: "sparkles",
              rgb: SIMD3(0.72, 0.60, 1.00), angle: 50),
        Realm(id: "vanaheim", name: "Vanaheim", epithet: "Creatività & Contenuti",
              summary: "Voce, mappe, competenze e pacchetti", icon: "leaf",
              rgb: SIMD3(0.30, 0.92, 0.55), angle: 10),
        Realm(id: "muspelheim", name: "Muspelheim", epithet: "Sicurezza & Difesa",
              summary: "Sicurezza e privacy", icon: "flame",
              rgb: SIMD3(1.00, 0.34, 0.18), angle: -30),
        Realm(id: "svartalfheim", name: "Svartalfheim", epithet: "Strumenti & Infrastruttura",
              summary: "Automazioni, connettori, dispositivi e sistema (include Nidavellir)", icon: "gearshape.2",
              rgb: SIMD3(0.96, 0.60, 0.26), angle: -70),
        Realm(id: "midgard", name: "Midgard", epithet: "Utente & Progetti",
              summary: "Il tuo profilo, gli utenti e i workspace", icon: "person.2",
              rgb: SIMD3(0.45, 0.80, 0.40), angle: -110),
        Realm(id: "helheim", name: "Helheim", epithet: "Archiviazione & Memoria",
              summary: "Memoria e ricerche salvate", icon: "externaldrive",
              rgb: SIMD3(0.52, 0.72, 0.74), angle: -150),
        Realm(id: "niflheim", name: "Niflheim", epithet: "Monitoraggio & Analisi",
              summary: "Registro di audit e notifiche", icon: "chart.xyaxis.line",
              rgb: SIMD3(0.42, 0.86, 1.00), angle: 170),
        Realm(id: "jotunheim", name: "Jotunheim", epithet: "Sviluppo & Costruzione",
              summary: "Missioni, piani, strumenti e aggiornamenti", icon: "hammer",
              rgb: SIMD3(0.46, 0.60, 1.00), angle: 130)
    ]

    static func named(_ key: String) -> Realm? {
        let lowered = key.lowercased()
        return all.first { $0.id == lowered || $0.name.lowercased() == lowered }
    }

    /// Posizione del regno nello spazio della scena: sull'anello frontale.
    var position: SIMD3<Float> {
        let radians = angle * .pi / 180
        return SIMD3(Realm.ringX * cos(radians), Realm.ringCenter + Realm.ringY * sin(radians), 0)
    }
}

/// Una funzione dell'app dentro un regno.
struct YggFunction: Identifiable {
    let id: String
    let title: String
    let icon: String
    let subtitle: String
}

// MARK: - Immagini procedurali

private enum Texture {
    /// Punto morbido per particelle e aloni.
    static func softDot(_ size: CGFloat = 64) -> NSImage {
        NSImage(size: NSSize(width: size, height: size), flipped: false) { rect in
            let gradient = NSGradient(colors: [NSColor.white, NSColor.white.withAlphaComponent(0)])
            gradient?.draw(in: NSBezierPath(ovalIn: rect), relativeCenterPosition: .zero)
            return true
        }
    }

    /// Cielo notturno: dal blu profondo in alto al verde scuro all'orizzonte.
    static func sky() -> NSImage {
        NSImage(size: NSSize(width: 4, height: 512), flipped: false) { rect in
            let gradient = NSGradient(colors: [
                NSColor(red: 0.010, green: 0.030, blue: 0.040, alpha: 1),
                NSColor(red: 0.020, green: 0.075, blue: 0.095, alpha: 1),
                NSColor(red: 0.030, green: 0.040, blue: 0.110, alpha: 1),
                NSColor(red: 0.012, green: 0.012, blue: 0.045, alpha: 1)
            ], atLocations: [0, 0.32, 0.7, 1], colorSpace: .deviceRGB)
            gradient?.draw(in: rect, angle: 90)
            return true
        }
    }

    /// Nastro d'aurora: trasparente ai bordi, verde e viola al centro.
    static func aurora() -> NSImage {
        NSImage(size: NSSize(width: 8, height: 256), flipped: false) { rect in
            let gradient = NSGradient(colors: [
                NSColor(red: 0.2, green: 0.9, blue: 0.7, alpha: 0),
                NSColor(red: 0.25, green: 0.95, blue: 0.7, alpha: 0.55),
                NSColor(red: 0.45, green: 0.75, blue: 1.0, alpha: 0.45),
                NSColor(red: 0.65, green: 0.45, blue: 1.0, alpha: 0.30),
                NSColor(red: 0.65, green: 0.45, blue: 1.0, alpha: 0)
            ], atLocations: [0, 0.22, 0.5, 0.78, 1], colorSpace: .deviceRGB)
            gradient?.draw(in: rect, angle: 90)
            return true
        }
    }

    /// Foglia: sagoma con nervatura, bianca su trasparente (il colore arriva dai vertici).
    static func leaf() -> NSImage {
        NSImage(size: NSSize(width: 64, height: 64), flipped: false) { rect in
            let path = NSBezierPath()
            path.move(to: NSPoint(x: 32, y: 3))
            path.curve(to: NSPoint(x: 32, y: 61), controlPoint1: NSPoint(x: 62, y: 22), controlPoint2: NSPoint(x: 56, y: 48))
            path.curve(to: NSPoint(x: 32, y: 3), controlPoint1: NSPoint(x: 8, y: 48), controlPoint2: NSPoint(x: 2, y: 22))
            NSGradient(colors: [NSColor.white, NSColor.white.withAlphaComponent(0.55)])?.draw(in: path, angle: 90)
            NSColor.white.withAlphaComponent(0.25).setStroke()
            let vein = NSBezierPath()
            vein.move(to: NSPoint(x: 32, y: 6))
            vein.line(to: NSPoint(x: 32, y: 58))
            vein.lineWidth = 1.2
            vein.stroke()
            return true
        }
    }

    // MARK: Rumore periodico

    private static func hash(_ x: Int, _ y: Int, _ seed: Int) -> Float {
        var h = UInt32(truncatingIfNeeded: x &* 374761393 &+ y &* 668265263 &+ seed &* 2147483647)
        h = (h ^ (h >> 13)) &* 1274126177
        h ^= h >> 16
        return Float(h & 0xFFFF) / 65535
    }

    /// Rumore di valore che si ripete ogni `px` × `py` celle: la texture si accosta senza giunture.
    private static func noise(_ x: Float, _ y: Float, _ px: Int, _ py: Int, _ seed: Int) -> Float {
        let xi = Int(floor(x)), yi = Int(floor(y))
        let xf = x - floor(x), yf = y - floor(y)
        func value(_ a: Int, _ b: Int) -> Float {
            hash(((a % px) + px) % px, ((b % py) + py) % py, seed)
        }
        let u = xf * xf * (3 - 2 * xf), w = yf * yf * (3 - 2 * yf)
        return value(xi, yi) * (1 - u) * (1 - w) + value(xi + 1, yi) * u * (1 - w)
            + value(xi, yi + 1) * (1 - u) * w + value(xi + 1, yi + 1) * u * w
    }

    private static func image(_ bytes: [UInt8], _ side: Int) -> NSImage {
        let provider = CGDataProvider(data: Data(bytes) as CFData)
        guard let provider,
              let cg = CGImage(width: side, height: side, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: side * 4,
                               space: CGColorSpaceCreateDeviceRGB(),
                               bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
                               provider: provider, decode: nil, shouldInterpolate: true, intent: .defaultIntent)
        else { return NSImage(size: NSSize(width: side, height: side)) }
        return NSImage(cgImage: cg, size: NSSize(width: side, height: side))
    }

    /// Corteccia: colore e mappa delle normali (rilievo) dallo stesso campo di altezze, con crepe verticali.
    static func barkMaps() -> (diffuse: NSImage, normal: NSImage) {
        let side = 256
        var height = [Float](repeating: 0, count: side * side)
        for y in 0..<side {
            for x in 0..<side {
                let u = Float(x) / Float(side), v = Float(y) / Float(side)
                var total: Float = 0, amplitude: Float = 0.55
                for octave in 0..<4 {
                    let px = 6 << octave, py = 2 << octave
                    let n = noise(u * Float(px), v * Float(py), px, py, octave + 3)
                    total += (1 - abs(2 * n - 1)) * amplitude
                    amplitude *= 0.5
                }
                height[y * side + x] = min(max(total, 0), 1)
            }
        }
        var color = [UInt8](repeating: 255, count: side * side * 4)
        var normal = [UInt8](repeating: 255, count: side * side * 4)
        for y in 0..<side {
            for x in 0..<side {
                let h = height[y * side + x]
                let index = (y * side + x) * 4
                let warm = 0.55 + 0.45 * h
                color[index] = UInt8(min(255, 38 + 118 * warm * h + 20))
                color[index + 1] = UInt8(min(255, 24 + 80 * warm * h + 12))
                color[index + 2] = UInt8(min(255, 14 + 48 * warm * h + 8))
                func at(_ dx: Int, _ dy: Int) -> Float {
                    height[((y + dy + side) % side) * side + (x + dx + side) % side]
                }
                let nx = (at(-1, 0) - at(1, 0)) * 3.2
                let ny = (at(0, -1) - at(0, 1)) * 3.2
                let length = sqrt(nx * nx + ny * ny + 1)
                normal[index] = UInt8((nx / length * 0.5 + 0.5) * 255)
                normal[index + 1] = UInt8((ny / length * 0.5 + 0.5) * 255)
                normal[index + 2] = UInt8((1 / length * 0.5 + 0.5) * 255)
            }
        }
        return (image(color, side), image(normal, side))
    }

    /// Rune incise sul tronco: nero con segni luminosi (emissione). Alcune lettere del futhark antico, disegnate a tratti.
    static func runes() -> NSImage {
        // Ogni runa è un elenco di segmenti (x1, y1, x2, y2) in un quadrato 0…1.
        let glyphs: [[(CGFloat, CGFloat, CGFloat, CGFloat)]] = [
            [(0.5, 0, 0.5, 1), (0.5, 0.65, 0.85, 0.9), (0.5, 0.4, 0.85, 0.65)],
            [(0.25, 0, 0.25, 1), (0.25, 1, 0.7, 0.75), (0.7, 0.75, 0.7, 0)],
            [(0.4, 0, 0.4, 1), (0.4, 0.8, 0.78, 0.55), (0.78, 0.55, 0.4, 0.3)],
            [(0.4, 0, 0.4, 1), (0.4, 1, 0.8, 0.78), (0.4, 0.7, 0.8, 0.48)],
            [(0.3, 0, 0.3, 1), (0.3, 1, 0.75, 0.82), (0.75, 0.82, 0.3, 0.55), (0.3, 0.55, 0.78, 0)],
            [(0.7, 0.9, 0.3, 0.5), (0.3, 0.5, 0.7, 0.1)],
            [(0.25, 0.85, 0.75, 0.15), (0.75, 0.85, 0.25, 0.15)],
            [(0.3, 0, 0.3, 1), (0.7, 0, 0.7, 1), (0.3, 0.7, 0.7, 0.4)],
            [(0.5, 0, 0.5, 1)],
            [(0.5, 0, 0.5, 1), (0.25, 0.7, 0.5, 0.45), (0.75, 0.7, 0.5, 0.45)],
            [(0.3, 0, 0.3, 1), (0.3, 0.55, 0.75, 1), (0.3, 0.55, 0.75, 0)],
            [(0.5, 0.05, 0.5, 0.95), (0.5, 0.95, 0.8, 0.7), (0.5, 0.05, 0.2, 0.3)]
        ]
        return NSImage(size: NSSize(width: 1024, height: 1024), flipped: false) { rect in
            NSColor.black.setFill()
            rect.fill()
            var generator = SeededRandom(seed: 5)
            let columns = 6
            let rows = 11
            for column in 0..<columns {
                for row in 0..<rows where Int.random(in: 0...9, using: &generator) < 7 {
                    let glyph = glyphs[Int.random(in: 0..<glyphs.count, using: &generator)]
                    let x0 = (CGFloat(column) + 0.5) * rect.width / CGFloat(columns) - 22
                    let y0 = (CGFloat(row) + 0.25) * rect.height / CGFloat(rows)
                    for (width, alpha) in [(CGFloat(11), CGFloat(0.22)), (CGFloat(5), CGFloat(0.95))] {
                        NSColor(red: 0.30, green: 0.85, blue: 1.0, alpha: alpha).setStroke()
                        let path = NSBezierPath()
                        path.lineWidth = width
                        path.lineCapStyle = .round
                        for segment in glyph {
                            path.move(to: NSPoint(x: x0 + segment.0 * 44, y: y0 + segment.1 * 62))
                            path.line(to: NSPoint(x: x0 + segment.2 * 44, y: y0 + segment.3 * 62))
                        }
                        path.stroke()
                    }
                }
            }
            return true
        }
    }
}

/// Generatore deterministico: l'albero ha sempre lo stesso aspetto.
struct SeededRandom: RandomNumberGenerator {
    private var state: UInt64
    init(seed: UInt64) { state = seed &* 6364136223846793005 &+ 1442695040888963407 }
    mutating func next() -> UInt64 {
        state = state &* 6364136223846793005 &+ 1442695040888963407
        var x = state
        x ^= x >> 33
        x = x &* 0xff51afd7ed558ccd
        x ^= x >> 33
        return x
    }
}

// MARK: - Geometrie procedurali

private typealias V3 = SIMD3<Float>

private enum Mesh {
    /// Superficie continua lungo una curva: sezioni circolari con raggio dipendente dalla posizione e dall'angolo
    /// (creste, rastremazione, allargamento alla base). Le sezioni seguono la curva senza torcersi.
    static func tube(spine: [V3], sides: Int, vRepeat: Float, radius: (Float, Float) -> Float) -> SCNGeometry {
        let count = spine.count
        var positions: [V3] = []
        var uvs: [SIMD2<Float>] = []
        positions.reserveCapacity(count * (sides + 1))
        var tangents: [V3] = []
        for index in 0..<count {
            let before = spine[max(index - 1, 0)], after = spine[min(index + 1, count - 1)]
            tangents.append(simd_normalize(after - before))
        }
        let first = tangents[0]
        let reference: V3 = abs(first.y) < 0.9 ? V3(0, 1, 0) : V3(1, 0, 0)
        var frame = simd_normalize(simd_cross(first, reference))
        for index in 0..<count {
            let tangent = tangents[index]
            frame = simd_normalize(frame - simd_dot(frame, tangent) * tangent)
            let binormal = simd_cross(tangent, frame)
            let along = Float(index) / Float(max(count - 1, 1))
            for side in 0...sides {
                let angle = Float(side) / Float(sides) * 2 * .pi
                let direction = cos(angle) * frame + sin(angle) * binormal
                positions.append(spine[index] + direction * radius(along, angle))
                uvs.append(SIMD2(Float(side) / Float(sides), along * vRepeat))
            }
        }
        var indices: [UInt32] = []
        let stride = sides + 1
        for index in 0..<(count - 1) {
            for side in 0..<sides {
                let a = UInt32(index * stride + side), b = a + 1
                let c = UInt32((index + 1) * stride + side), d = c + 1
                indices += [a, b, c, b, d, c]
            }
        }
        var normals = [V3](repeating: .zero, count: positions.count)
        for triangle in stride_triangles(indices) {
            let normal = simd_cross(positions[Int(triangle.1)] - positions[Int(triangle.0)],
                                    positions[Int(triangle.2)] - positions[Int(triangle.0)])
            normals[Int(triangle.0)] += normal
            normals[Int(triangle.1)] += normal
            normals[Int(triangle.2)] += normal
        }
        for index in 0..<count {
            let seam = normals[index * stride] + normals[index * stride + sides]
            normals[index * stride] = seam
            normals[index * stride + sides] = seam
        }
        normals = normals.map { simd_length($0) > 0 ? simd_normalize($0) : V3(0, 1, 0) }
        return geometry(positions: positions, normals: normals, uvs: uvs, indices: indices)
    }

    private static func stride_triangles(_ indices: [UInt32]) -> [(UInt32, UInt32, UInt32)] {
        var out: [(UInt32, UInt32, UInt32)] = []
        out.reserveCapacity(indices.count / 3)
        var index = 0
        while index + 2 < indices.count {
            out.append((indices[index], indices[index + 1], indices[index + 2]))
            index += 3
        }
        return out
    }

    /// Foglie: molti quadrati orientati a caso, in un'unica geometria (una sola chiamata di disegno).
    static func leaves(centres: [V3], sizes: [Float], colors: [SIMD4<Float>], generator: inout SeededRandom) -> SCNGeometry {
        var positions: [V3] = []
        var normals: [V3] = []
        var uvs: [SIMD2<Float>] = []
        var tint: [SIMD4<Float>] = []
        var indices: [UInt32] = []
        for (index, centre) in centres.enumerated() {
            let axis = simd_normalize(V3(Float.random(in: -1...1, using: &generator),
                                         Float.random(in: -1...1, using: &generator),
                                         Float.random(in: -1...1, using: &generator)) + V3(0, 0.001, 0))
            let rotation = simd_quatf(angle: Float.random(in: 0...(2 * .pi), using: &generator), axis: axis)
            let right = rotation.act(V3(1, 0, 0)) * sizes[index]
            let up = rotation.act(V3(0, 1, 0)) * sizes[index]
            let base = UInt32(positions.count)
            positions += [centre - right - up, centre + right - up, centre + right + up, centre - right + up]
            let normal = simd_normalize(simd_cross(right, up))
            normals += [normal, normal, normal, normal]
            uvs += [SIMD2(0, 0), SIMD2(1, 0), SIMD2(1, 1), SIMD2(0, 1)]
            tint += [colors[index], colors[index], colors[index], colors[index]]
            indices += [base, base + 1, base + 2, base, base + 2, base + 3]
        }
        let shape = geometry(positions: positions, normals: normals, uvs: uvs, indices: indices)
        let colorData = Data(bytes: tint, count: tint.count * MemoryLayout<SIMD4<Float>>.stride)
        let colorSource = SCNGeometrySource(
            data: colorData, semantic: .color, vectorCount: tint.count, usesFloatComponents: true,
            componentsPerVector: 4, bytesPerComponent: MemoryLayout<Float>.size, dataOffset: 0,
            dataStride: MemoryLayout<SIMD4<Float>>.stride)
        return SCNGeometry(sources: shape.sources + [colorSource], elements: shape.elements)
    }

    private static func geometry(positions: [V3], normals: [V3], uvs: [SIMD2<Float>], indices: [UInt32]) -> SCNGeometry {
        let vertexSource = SCNGeometrySource(
            data: Data(bytes: positions, count: positions.count * MemoryLayout<V3>.stride), semantic: .vertex,
            vectorCount: positions.count, usesFloatComponents: true, componentsPerVector: 3,
            bytesPerComponent: MemoryLayout<Float>.size, dataOffset: 0, dataStride: MemoryLayout<V3>.stride)
        let normalSource = SCNGeometrySource(
            data: Data(bytes: normals, count: normals.count * MemoryLayout<V3>.stride), semantic: .normal,
            vectorCount: normals.count, usesFloatComponents: true, componentsPerVector: 3,
            bytesPerComponent: MemoryLayout<Float>.size, dataOffset: 0, dataStride: MemoryLayout<V3>.stride)
        let uvSource = SCNGeometrySource(
            data: Data(bytes: uvs, count: uvs.count * MemoryLayout<SIMD2<Float>>.stride), semantic: .texcoord,
            vectorCount: uvs.count, usesFloatComponents: true, componentsPerVector: 2,
            bytesPerComponent: MemoryLayout<Float>.size, dataOffset: 0, dataStride: MemoryLayout<SIMD2<Float>>.stride)
        let element = SCNGeometryElement(
            data: Data(bytes: indices, count: indices.count * MemoryLayout<UInt32>.size),
            primitiveType: .triangles, primitiveCount: indices.count / 3, bytesPerIndex: MemoryLayout<UInt32>.size)
        return SCNGeometry(sources: [vertexSource, normalSource, uvSource], elements: [element])
    }
}

// MARK: - Scena

/// La scena 3D: costruisce l'albero, gestisce orbita della camera, passaggio del mouse e volo verso un regno.
final class YggdrasilStage: NSObject, ObservableObject, SCNSceneRendererDelegate {
    let scene = SCNScene()
    let camera = SCNNode()
    private let focus = SCNNode()
    private var realmNodes: [String: SCNNode] = [:]
    private var orbiting = true
    /// Fase dell'ondeggiamento: la camera resta davanti all'albero e dondola piano a destra e a sinistra.
    private var swayPhase: Double = 0
    private var lastTime: TimeInterval = 0
    private let orbitRadius: Float = 31
    private let orbitHeight: Float = 8.4
    private let treeFocus = SIMD3<Float>(0, Realm.ringCenter, 0)
    /// Scala delle isole dei regni (più grandi dell'originale per essere leggibili dall'anello).
    private let realmScale: CGFloat = 1.45
    /// Curve dei rami verso i regni, ricordate per mettere le foglie lungo di esse.
    private var branchCurves: [String: (V3, V3, V3, V3)] = [:]
    /// Con «riduci movimento» l'orbita resta ferma e i voli sono quasi immediati.
    var reduceMotion = false

    override init() {
        super.init()
        buildEnvironment()
        buildTree()
        buildFoliage()
        Realm.all.forEach { buildRealm($0) }
        buildRing()
        buildCamera()
    }

    // MARK: Ambiente

    private func buildEnvironment() {
        scene.background.contents = Texture.sky()
        scene.fogColor = NSColor(red: 0.02, green: 0.05, blue: 0.07, alpha: 1)
        scene.fogStartDistance = 30
        scene.fogEndDistance = 100
        scene.fogDensityExponent = 1.2

        let ambient = SCNNode()
        ambient.light = SCNLight()
        ambient.light?.type = .ambient
        ambient.light?.color = NSColor(red: 0.20, green: 0.26, blue: 0.36, alpha: 1)
        ambient.light?.intensity = 520
        scene.rootNode.addChildNode(ambient)

        let moon = SCNNode()
        moon.light = SCNLight()
        moon.light?.type = .directional
        moon.light?.color = NSColor(red: 0.62, green: 0.74, blue: 1.0, alpha: 1)
        moon.light?.intensity = 900
        moon.light?.castsShadow = true
        moon.light?.shadowMode = .deferred
        moon.light?.shadowRadius = 8
        moon.light?.shadowColor = NSColor(white: 0, alpha: 0.55)
        moon.eulerAngles = SCNVector3(-0.9, 0.7, 0)
        scene.rootNode.addChildNode(moon)

        let crown = SCNNode()
        crown.simdPosition = SIMD3(0, 13.5, 0)
        crown.light = SCNLight()
        crown.light?.type = .omni
        crown.light?.color = NSColor(red: 1.0, green: 0.82, blue: 0.5, alpha: 1)
        crown.light?.intensity = 1100
        crown.light?.attenuationStartDistance = 2
        crown.light?.attenuationEndDistance = 26
        scene.rootNode.addChildNode(crown)

        let roots = SCNNode()
        roots.simdPosition = SIMD3(0, 0.5, 0)
        roots.light = SCNLight()
        roots.light?.type = .omni
        roots.light?.color = NSColor(red: 0.30, green: 0.80, blue: 1.0, alpha: 1)
        roots.light?.intensity = 700
        roots.light?.attenuationStartDistance = 1
        roots.light?.attenuationEndDistance = 14
        scene.rootNode.addChildNode(roots)

        scene.rootNode.addChildNode(stars())
        scene.rootNode.addChildNode(water())
        scene.rootNode.addChildNode(mist())
        scene.rootNode.addChildNode(auroras())
    }

    private func stars() -> SCNNode {
        var generator = SeededRandom(seed: 99)
        var vertices: [SIMD3<Float>] = []
        for _ in 0..<1400 {
            let theta = Float.random(in: 0...(2 * .pi), using: &generator)
            let phi = acos(Float.random(in: -0.1...1, using: &generator))
            let r: Float = 85
            vertices.append(SIMD3(r * sin(phi) * cos(theta), r * cos(phi) + 10, r * sin(phi) * sin(theta)))
        }
        let data = Data(bytes: vertices, count: vertices.count * MemoryLayout<SIMD3<Float>>.stride)
        let source = SCNGeometrySource(
            data: data, semantic: .vertex, vectorCount: vertices.count, usesFloatComponents: true,
            componentsPerVector: 3, bytesPerComponent: MemoryLayout<Float>.size, dataOffset: 0,
            dataStride: MemoryLayout<SIMD3<Float>>.stride)
        let indices = (0..<vertices.count).map { UInt32($0) }
        let element = SCNGeometryElement(
            data: Data(bytes: indices, count: indices.count * MemoryLayout<UInt32>.size),
            primitiveType: .point, primitiveCount: indices.count, bytesPerIndex: MemoryLayout<UInt32>.size)
        element.pointSize = 1.2
        element.minimumPointScreenSpaceRadius = 0.6
        element.maximumPointScreenSpaceRadius = 1.9
        let geometry = SCNGeometry(sources: [source], elements: [element])
        let material = SCNMaterial()
        material.lightingModel = .constant
        material.diffuse.contents = NSColor(red: 0.85, green: 0.92, blue: 1.0, alpha: 1)
        geometry.materials = [material]
        return SCNNode(geometry: geometry)
    }

    /// Mímisbrunnr: acqua scura che riflette l'albero, con anelli luminosi.
    private func water() -> SCNNode {
        let node = SCNNode()
        let floor = SCNFloor()
        floor.reflectivity = 0.34
        floor.reflectionFalloffStart = 0
        floor.reflectionFalloffEnd = 10
        floor.reflectionResolutionScaleFactor = 0.6
        let surface = SCNMaterial()
        surface.lightingModel = .blinn
        surface.diffuse.contents = NSColor(red: 0.015, green: 0.055, blue: 0.095, alpha: 1)
        surface.specular.contents = NSColor.white
        surface.shininess = 0.9
        floor.materials = [surface]
        let floorNode = SCNNode(geometry: floor)
        floorNode.simdPosition = SIMD3(0, -0.9, 0)
        node.addChildNode(floorNode)
        for (index, radius) in [3.4, 5.0, 6.8, 9.0].enumerated() {
            let ring = SCNTorus(ringRadius: CGFloat(radius), pipeRadius: 0.025)
            let glow = SCNMaterial()
            glow.lightingModel = .constant
            glow.diffuse.contents = NSColor(red: 0.35, green: 0.85, blue: 1.0, alpha: 1)
            glow.emission.contents = NSColor(red: 0.30, green: 0.80, blue: 1.0, alpha: 1)
            ring.materials = [glow]
            let ringNode = SCNNode(geometry: ring)
            ringNode.simdPosition = SIMD3(0, -0.86, 0)
            ringNode.opacity = 0.6 - CGFloat(index) * 0.12
            node.addChildNode(ringNode)
        }
        return node
    }

    private func mist() -> SCNNode {
        let plane = SCNPlane(width: 80, height: 80)
        let material = SCNMaterial()
        material.lightingModel = .constant
        material.diffuse.contents = Texture.softDot(128)
        material.emission.contents = NSColor(red: 0.10, green: 0.30, blue: 0.40, alpha: 1)
        material.blendMode = .add
        material.isDoubleSided = true
        material.writesToDepthBuffer = false
        plane.materials = [material]
        let node = SCNNode(geometry: plane)
        node.eulerAngles = SCNVector3(-Double.pi / 2, 0, 0)
        node.simdPosition = SIMD3(0, -0.6, 0)
        node.opacity = 0.5
        return node
    }

    /// Tre nastri d'aurora dietro l'albero, che ondeggiano piano.
    private func auroras() -> SCNNode {
        let group = SCNNode()
        let material = SCNMaterial()
        material.lightingModel = .constant
        material.diffuse.contents = Texture.aurora()
        material.blendMode = .add
        material.isDoubleSided = true
        material.writesToDepthBuffer = false
        for index in 0..<3 {
            let plane = SCNPlane(width: 90, height: 26)
            plane.materials = [material]
            let node = SCNNode(geometry: plane)
            let angle = Float(index) * 2.1 + 0.4
            node.simdPosition = SIMD3(58 * cos(angle), 30 + Float(index) * 3, 58 * sin(angle))
            node.eulerAngles = SCNVector3(0.35, -Double(angle) + .pi / 2, 0)
            node.opacity = 0.55
            node.runAction(.repeatForever(.sequence([
                .fadeOpacity(to: 0.25, duration: 7 + Double(index) * 2),
                .fadeOpacity(to: 0.6, duration: 7 + Double(index) * 2)
            ])))
            group.addChildNode(node)
        }
        return group
    }

    // MARK: Albero

    private func bezier(_ p0: V3, _ p1: V3, _ p2: V3, _ p3: V3, _ t: Float) -> V3 {
        let u = 1 - t
        return (u * u * u) * p0 + (3 * u * u * t) * p1 + (3 * u * t * t) * p2 + (t * t * t) * p3
    }

    private func barkMaterial() -> SCNMaterial {
        let maps = Texture.barkMaps()
        let material = SCNMaterial()
        material.lightingModel = .physicallyBased
        material.diffuse.contents = maps.diffuse
        material.normal.contents = maps.normal
        material.normal.intensity = 1.4
        for property in [material.diffuse, material.normal] {
            property.wrapS = .repeat
            property.wrapT = .repeat
        }
        material.roughness.contents = 0.9
        material.metalness.contents = 0.04
        material.isDoubleSided = true
        return material
    }

    private func buildTree() {
        let bark = barkMaterial()
        // Il tronco porta le rune incise, che brillano.
        let trunkBark = bark.copy() as? SCNMaterial ?? bark
        trunkBark.emission.contents = Texture.runes()
        trunkBark.emission.wrapS = .repeat
        trunkBark.emission.wrapT = .repeat
        trunkBark.emission.intensity = 1.6
        let tree = SCNNode()
        tree.name = "yggdrasil"

        // Tronco: una colonna con creste che si attorcigliano e un grande allargamento alla base.
        var spine: [V3] = []
        for index in 0...56 {
            let t = Float(index) / 56
            let y = -0.9 + t * 17.0
            spine.append(V3(0.45 * sin(t * 5.2), y, 0.40 * cos(t * 4.1)))
        }
        let trunk = Mesh.tube(spine: spine, sides: 56, vRepeat: 6) { t, angle in
            let flare: Float = 1.35 * exp(-t * 11)
            let taper: Float = 1.35 - 0.95 * t
            let wave1: Float = 0.16 * sin(5 * angle + 7 * t)
            let wave2: Float = 0.08 * sin(9 * angle - 11 * t)
            let damping: Float = 1 - 0.5 * t
            return max(0.3, taper + flare + (wave1 + wave2) * damping)
        }
        trunk.materials = [trunkBark]
        tree.addChildNode(SCNNode(geometry: trunk))

        // Tre fusti secondari che si avvolgono attorno al principale e vi si fondono.
        for strand in 0..<3 {
            let phase = Float(strand) * 2 * .pi / 3
            var helix: [V3] = []
            for index in 0...44 {
                let t = Float(index) / 44
                let around = 1.55 * (1 - 0.72 * t) + 0.15
                let turn = phase + t * 2 * .pi * 1.35
                helix.append(V3(around * cos(turn), -0.7 + t * 9.5, around * sin(turn)))
            }
            let rope = Mesh.tube(spine: helix, sides: 20, vRepeat: 3) { t, angle in
                let base: Float = 0.62 * (1 - 0.6 * t) + 0.12
                let bump: Float = 1 + 0.1 * sin(6 * angle + 5 * t)
                return base * bump
            }
            rope.materials = [bark]
            tree.addChildNode(SCNNode(geometry: rope))
        }

        // Radici che affondano nella sorgente.
        var generator = SeededRandom(seed: 21)
        for index in 0..<9 {
            let angle = Float(index) * (2 * .pi / 9) + Float.random(in: -0.2...0.2, using: &generator)
            let reach = Float.random(in: 4.4...7.0, using: &generator)
            let end = V3(reach * cos(angle), -0.95, reach * sin(angle))
            let c1 = V3(1.0 * cos(angle), 0.4, 1.0 * sin(angle))
            let c2 = V3(reach * 0.62 * cos(angle), -1.0, reach * 0.62 * sin(angle))
            var path: [V3] = []
            for step in 0...20 { path.append(bezier(V3(0, 0.9, 0), c1, c2, end, Float(step) / 20)) }
            let root = Mesh.tube(spine: path, sides: 14, vRepeat: 2) { t, angle in
                let base: Float = 0.95 * (1 - t) + 0.07
                let bump: Float = 1 + 0.12 * sin(4 * angle + 9 * t)
                return base * bump
            }
            root.materials = [bark]
            tree.addChildNode(SCNNode(geometry: root))
        }

        // Un ramo per ogni regno: sale dal tronco, si apre sull'anello e arriva sotto l'isola.
        for realm in Realm.all {
            let target = realm.position
            if abs(target.x) < 1 { continue } // Asgard sta sulla cima del tronco
            let side: Float = target.x < 0 ? -1 : 1
            let start = V3(0, min(max(target.y * 0.5 + 1.6, 2.6), 11.0), 0)
            let end = target - V3(0, 1.5, 0)
            let rising = end.y > start.y
            let wobble: Float = realm.angle.truncatingRemainder(dividingBy: 80) > 0 ? 0.9 : -0.9
            let c1 = start + V3(target.x * 0.32, rising ? 1.2 : 0.5, wobble)
            let c2 = V3(target.x * 0.86, end.y + (rising ? -1.8 : 1.1), -wobble * 0.6)
            branchCurves[realm.id] = (start, c1, c2, end)
            var path: [V3] = []
            for step in 0...30 { path.append(bezier(start, c1, c2, end, Float(step) / 30)) }
            let branch = Mesh.tube(spine: path, sides: 14, vRepeat: 3) { t, angle in
                let base: Float = 0.46 * (1 - t) + 0.09
                let bump: Float = 1 + 0.1 * sin(5 * angle + 7 * t)
                return base * bump
            }
            branch.materials = [bark]
            tree.addChildNode(SCNNode(geometry: branch))
            for twig in 0..<5 {
                let t = Float(twig + 2) / 8
                let origin = bezier(start, c1, c2, end, t)
                let tip = origin + V3(side * Float.random(in: 0.4...1.6, using: &generator),
                                      Float.random(in: 0.6...1.8, using: &generator),
                                      Float.random(in: -1.2...1.2, using: &generator))
                let reach: V3 = tip - origin
                let control1: V3 = origin + reach * 0.35 + V3(0, 0.4, 0)
                let control2: V3 = origin + reach * 0.75
                var small: [V3] = []
                for step in 0...8 { small.append(bezier(origin, control1, control2, tip, Float(step) / 8)) }
                let twigMesh = Mesh.tube(spine: small, sides: 8, vRepeat: 1) { t, _ in
                    let radius: Float = 0.08 * (1 - t) + 0.015
                    return radius
                }
                twigMesh.materials = [bark]
                tree.addChildNode(SCNNode(geometry: twigMesh))
            }
        }
        scene.rootNode.addChildNode(tree)

        // Particelle: foglie che scendono piano e lucciole attorno al tronco.
        let canopy = SCNNode()
        canopy.simdPosition = SIMD3(0, 12.0, 0)
        canopy.addParticleSystem(drifting(color: NSColor(red: 1.0, green: 0.82, blue: 0.40, alpha: 0.9), radius: 7.5, rate: 60, size: 0.13))
        canopy.addParticleSystem(drifting(color: NSColor(red: 0.45, green: 0.95, blue: 0.60, alpha: 0.8), radius: 6.5, rate: 34, size: 0.10))
        scene.rootNode.addChildNode(canopy)
        let fireflies = SCNNode()
        fireflies.simdPosition = SIMD3(0, 4, 0)
        fireflies.addParticleSystem(drifting(color: NSColor(red: 0.62, green: 0.95, blue: 1.0, alpha: 0.9), radius: 9, rate: 30, size: 0.08))
        scene.rootNode.addChildNode(fireflies)
    }

    /// Chioma: migliaia di foglie luminose in una sola geometria, più fitte attorno ai regni.
    private func buildFoliage() {
        var generator = SeededRandom(seed: 314)
        var centres: [V3] = []
        var sizes: [Float] = []
        var colors: [SIMD4<Float>] = []
        func add(_ point: V3, _ size: Float, _ tint: SIMD3<Float>) {
            centres.append(point)
            sizes.append(size)
            colors.append(SIMD4(tint * Float.random(in: 0.55...1.0, using: &generator), 1))
        }
        let gold = SIMD3<Float>(0.55, 0.42, 0.12), green = SIMD3<Float>(0.14, 0.45, 0.22), teal = SIMD3<Float>(0.12, 0.40, 0.42)
        // Nuvola della chioma, attorno alla cima del tronco.
        for _ in 0..<1900 {
            let direction = simd_normalize(V3(Float.random(in: -1...1, using: &generator),
                                              Float.random(in: -0.5...1, using: &generator),
                                              Float.random(in: -1...1, using: &generator)) + V3(0, 0.001, 0))
            let radius = pow(Float.random(in: 0.25...1, using: &generator), 0.6)
            let point = V3(0, 13.2, 0) + V3(direction.x * 5.4, direction.y * 3.2, direction.z * 3.2) * radius
            add(point, Float.random(in: 0.16...0.34, using: &generator), [gold, green, teal][Int.random(in: 0..<3, using: &generator)])
        }
        // Foglie lungo il ramo di ogni regno, tinte del suo colore.
        for realm in Realm.all {
            guard let curve = branchCurves[realm.id] else { continue }
            let tint = SIMD3<Float>(Float(realm.rgb.x), Float(realm.rgb.y), Float(realm.rgb.z)) * 0.5
            for _ in 0..<170 {
                let t = Float.random(in: 0.25...0.97, using: &generator)
                let along = bezier(curve.0, curve.1, curve.2, curve.3, t)
                let spread = V3(Float.random(in: -1...1, using: &generator), Float.random(in: -0.5...1, using: &generator),
                                Float.random(in: -1...1, using: &generator))
                add(along + spread * 1.3 + V3(0, 0.4, 0), Float.random(in: 0.13...0.26, using: &generator),
                    Float.random(in: 0...1, using: &generator) < 0.5 ? tint : green)
            }
        }
        let geometry = Mesh.leaves(centres: centres, sizes: sizes, colors: colors, generator: &generator)
        let material = SCNMaterial()
        material.lightingModel = .constant
        material.diffuse.contents = Texture.leaf()
        material.transparent.contents = Texture.leaf()
        material.blendMode = .add
        material.isDoubleSided = true
        material.writesToDepthBuffer = false
        geometry.materials = [material]
        let node = SCNNode(geometry: geometry)
        node.name = "foliage"
        scene.rootNode.addChildNode(node)
        // La chioma ondeggia appena.
        node.runAction(.repeatForever(.sequence([
            .rotateBy(x: 0, y: 0.05, z: 0, duration: 9),
            .rotateBy(x: 0, y: -0.05, z: 0, duration: 9)
        ])))
    }

    private func drifting(color: NSColor, radius: CGFloat, rate: CGFloat, size: CGFloat) -> SCNParticleSystem {
        let system = SCNParticleSystem()
        system.birthRate = rate
        system.particleLifeSpan = 7
        system.particleLifeSpanVariation = 3
        system.emitterShape = SCNSphere(radius: radius)
        system.birthLocation = .volume
        system.particleSize = size
        system.particleSizeVariation = size * 0.6
        system.particleColor = color
        system.particleVelocity = 0.35
        system.particleVelocityVariation = 0.3
        system.acceleration = SCNVector3(0, -0.12, 0)
        system.blendMode = .additive
        system.particleImage = Texture.softDot()
        system.isLightingEnabled = false
        system.isAffectedByGravity = false
        return system
    }

    /// Il grande anello dei nove regni: una fascia luminosa ellittica davanti all'albero, con perline di luce.
    private func buildRing() {
        let ring = SCNTorus(ringRadius: CGFloat(Realm.ringX), pipeRadius: 0.05)
        let material = SCNMaterial()
        material.lightingModel = .constant
        material.diffuse.contents = NSColor(red: 0.30, green: 0.80, blue: 1.0, alpha: 1)
        material.emission.contents = NSColor(red: 0.30, green: 0.80, blue: 1.0, alpha: 1)
        material.emission.intensity = 1.3
        ring.materials = [material]
        let node = SCNNode(geometry: ring)
        node.simdPosition = SIMD3(0, Realm.ringCenter, 0)
        node.eulerAngles = SCNVector3(Double.pi / 2, 0, 0)
        node.simdScale = SIMD3(1, 1, Realm.ringY / Realm.ringX)
        node.opacity = 0.5
        scene.rootNode.addChildNode(node)
        // Perline che scorrono lungo l'anello.
        let beads = SCNNode()
        beads.simdPosition = SIMD3(0, Realm.ringCenter, 0)
        for index in 0..<72 {
            let angle = Float(index) / 72 * 2 * .pi
            let bead = SCNSphere(radius: index % 8 == 0 ? 0.16 : 0.07)
            bead.materials = [glow(NSColor(red: 0.6, green: 0.92, blue: 1.0, alpha: 1), intensity: 1.4)]
            let beadNode = SCNNode(geometry: bead)
            beadNode.simdPosition = SIMD3(Realm.ringX * cos(angle), Realm.ringY * sin(angle), 0)
            beadNode.opacity = 0.75
            beads.addChildNode(beadNode)
        }
        scene.rootNode.addChildNode(beads)
    }

    // MARK: Regni

    private func part(_ geometry: SCNGeometry, _ color: NSColor, glow: CGFloat = 0.4,
                      at position: V3 = .zero, euler: V3 = .zero) -> SCNNode {
        let material = SCNMaterial()
        material.lightingModel = .physicallyBased
        material.diffuse.contents = color
        material.metalness.contents = 0.15
        material.roughness.contents = 0.55
        material.emission.contents = color
        material.emission.intensity = glow
        geometry.materials = [material]
        let node = SCNNode(geometry: geometry)
        node.simdPosition = position
        node.simdEulerAngles = euler
        return node
    }

    /// Paesaggio in miniatura di ogni regno, dentro la cupola: la base della scena è y = 0,04.
    private func miniature(_ id: String, color: NSColor) -> SCNNode {
        let group = SCNNode()
        let ground: Float = 0.04
        switch id {
        case "asgard":
            let gold = NSColor(red: 1.0, green: 0.80, blue: 0.32, alpha: 1)
            group.addChildNode(part(SCNCone(topRadius: 0, bottomRadius: 0.26, height: 1.5), gold, glow: 0.7, at: V3(0, ground + 0.75, 0)))
            for index in 0..<5 {
                let angle = Float(index) * 2 * .pi / 5
                group.addChildNode(part(SCNCone(topRadius: 0, bottomRadius: 0.14, height: 0.8), gold, glow: 0.6,
                                        at: V3(0.58 * cos(angle), ground + 0.4, 0.58 * sin(angle))))
            }
        case "alfheim":
            for index in 0..<7 {
                let angle = Float(index) * 2 * .pi / 7
                let height = 0.5 + 0.12 * Float(index % 4)
                group.addChildNode(part(SCNBox(width: 0.17, height: CGFloat(height), length: 0.17, chamferRadius: 0.04), color, glow: 1.0,
                                        at: V3(0.45 * cos(angle), ground + height / 2, 0.45 * sin(angle)),
                                        euler: V3(0.18 * cos(angle), 0, 0.18 * sin(angle))))
            }
            group.addChildNode(part(SCNBox(width: 0.24, height: 1.3, length: 0.24, chamferRadius: 0.05), color, glow: 1.1,
                                    at: V3(0, ground + 0.65, 0)))
        case "vanaheim":
            let leaf = NSColor(red: 0.30, green: 0.88, blue: 0.50, alpha: 1)
            group.addChildNode(part(SCNCylinder(radius: 0.09, height: 0.7), NSColor(red: 0.45, green: 0.30, blue: 0.16, alpha: 1), glow: 0.1,
                                    at: V3(0, ground + 0.35, 0)))
            for (offset, radius) in [(V3(0, 0.95, 0), 0.42), (V3(0.28, 0.75, 0.1), 0.3), (V3(-0.25, 0.8, -0.12), 0.3)] {
                group.addChildNode(part(SCNSphere(radius: CGFloat(radius)), leaf, glow: 0.55, at: offset))
            }
        case "midgard":
            let grass = NSColor(red: 0.40, green: 0.80, blue: 0.38, alpha: 1)
            let hill = part(SCNSphere(radius: 0.9), grass, glow: 0.25, at: V3(0, ground - 0.05, 0))
            hill.simdScale = V3(1, 0.3, 1)
            group.addChildNode(hill)
            group.addChildNode(part(SCNBox(width: 0.4, height: 0.3, length: 0.34, chamferRadius: 0.02), NSColor(red: 0.85, green: 0.7, blue: 0.5, alpha: 1), glow: 0.2,
                                    at: V3(0.1, ground + 0.35, 0)))
            group.addChildNode(part(SCNPyramid(width: 0.52, height: 0.3, length: 0.46), NSColor(red: 0.7, green: 0.25, blue: 0.2, alpha: 1), glow: 0.3,
                                    at: V3(0.1, ground + 0.5, 0)))
            group.addChildNode(part(SCNSphere(radius: 0.2), grass, glow: 0.5, at: V3(-0.45, ground + 0.4, 0.1)))
        case "jotunheim":
            let rock = NSColor(red: 0.42, green: 0.55, blue: 0.95, alpha: 1)
            for (x, height, radius) in [(-0.3, 1.4, 0.5), (0.3, 1.0, 0.4), (0.0, 0.7, 0.35)] {
                group.addChildNode(part(SCNCone(topRadius: 0, bottomRadius: CGFloat(radius), height: CGFloat(height)), rock, glow: 0.35,
                                        at: V3(Float(x), ground + Float(height) / 2, Float(x) * 0.4)))
                group.addChildNode(part(SCNCone(topRadius: 0, bottomRadius: CGFloat(radius) * 0.45, height: CGFloat(height) * 0.35), NSColor.white, glow: 0.6,
                                        at: V3(Float(x), ground + Float(height) * 0.83, Float(x) * 0.4)))
            }
        case "muspelheim":
            let lava = NSColor(red: 1.0, green: 0.34, blue: 0.10, alpha: 1)
            group.addChildNode(part(SCNCone(topRadius: 0.2, bottomRadius: 0.75, height: 1.0), NSColor(red: 0.20, green: 0.10, blue: 0.08, alpha: 1), glow: 0.1,
                                    at: V3(0, ground + 0.5, 0)))
            group.addChildNode(part(SCNSphere(radius: 0.22), lava, glow: 1.6, at: V3(0, ground + 1.0, 0)))
            for index in 0..<4 {
                let angle = Float(index) * .pi / 2 + 0.4
                group.addChildNode(part(SCNBox(width: 0.05, height: 0.5, length: 0.05, chamferRadius: 0), lava, glow: 1.2,
                                        at: V3(0.3 * cos(angle), ground + 0.55, 0.3 * sin(angle)), euler: V3(0.5 * sin(angle), 0, -0.5 * cos(angle))))
            }
        case "svartalfheim":
            let iron = NSColor(red: 0.55, green: 0.55, blue: 0.62, alpha: 1)
            let fire = NSColor(red: 1.0, green: 0.55, blue: 0.20, alpha: 1)
            group.addChildNode(part(SCNBox(width: 0.7, height: 0.18, length: 0.3, chamferRadius: 0.03), iron, glow: 0.2, at: V3(-0.2, ground + 0.34, 0)))
            group.addChildNode(part(SCNBox(width: 0.3, height: 0.3, length: 0.2, chamferRadius: 0.02), iron, glow: 0.2, at: V3(-0.2, ground + 0.15, 0)))
            group.addChildNode(part(SCNCylinder(radius: 0.26, height: 0.55), NSColor(red: 0.28, green: 0.2, blue: 0.2, alpha: 1), glow: 0.1, at: V3(0.45, ground + 0.28, 0)))
            group.addChildNode(part(SCNSphere(radius: 0.2), fire, glow: 1.5, at: V3(0.45, ground + 0.6, 0)))
            group.addChildNode(part(SCNCylinder(radius: 0.06, height: 0.5), iron, glow: 0.1, at: V3(0.62, ground + 0.8, 0)))
        case "helheim":
            let stone = NSColor(red: 0.50, green: 0.64, blue: 0.66, alpha: 1)
            for x in [-0.38, 0.38] {
                group.addChildNode(part(SCNBox(width: 0.17, height: 1.0, length: 0.17, chamferRadius: 0.02), stone, glow: 0.2, at: V3(Float(x), ground + 0.5, 0)))
            }
            group.addChildNode(part(SCNBox(width: 1.0, height: 0.16, length: 0.2, chamferRadius: 0.02), stone, glow: 0.2, at: V3(0, ground + 1.05, 0)))
            let portal = part(SCNPlane(width: 0.6, height: 0.95), NSColor(red: 0.3, green: 0.9, blue: 0.85, alpha: 1), glow: 1.2, at: V3(0, ground + 0.52, 0))
            portal.opacity = 0.55
            group.addChildNode(portal)
        default: // niflheim
            for index in 0..<8 {
                let angle = Float(index) * 2 * .pi / 8 + 0.2
                let height = 0.5 + 0.18 * Float(index % 3)
                group.addChildNode(part(SCNCone(topRadius: 0, bottomRadius: 0.13, height: CGFloat(height)), color, glow: 1.0,
                                        at: V3(0.46 * cos(angle), ground + height / 2, 0.46 * sin(angle)),
                                        euler: V3(0.22 * cos(angle), 0, -0.22 * sin(angle))))
            }
            group.addChildNode(part(SCNCone(topRadius: 0, bottomRadius: 0.22, height: 1.5), color, glow: 1.2, at: V3(0, ground + 0.75, 0)))
        }
        return group
    }

    private func glow(_ color: NSColor, intensity: CGFloat = 1) -> SCNMaterial {
        let material = SCNMaterial()
        material.lightingModel = .constant
        material.diffuse.contents = color
        material.emission.contents = color
        material.emission.intensity = intensity
        return material
    }

    private func buildRealm(_ realm: Realm) {
        let node = SCNNode()
        node.name = "realm:\(realm.id)"
        node.simdPosition = realm.position
        node.scale = SCNVector3(realmScale, realmScale, realmScale)
        let color = realm.nsColor

        // Isola di roccia sospesa.
        let rock = SCNCone(topRadius: 0.98, bottomRadius: 0.12, height: 1.0)
        let rockMaterial = SCNMaterial()
        rockMaterial.lightingModel = .physicallyBased
        rockMaterial.diffuse.contents = NSColor(red: 0.11, green: 0.10, blue: 0.13, alpha: 1)
        rockMaterial.roughness.contents = 0.9
        rockMaterial.emission.contents = color
        rockMaterial.emission.intensity = 0.06
        rock.materials = [rockMaterial]
        let rockNode = SCNNode(geometry: rock)
        rockNode.simdPosition = SIMD3(0, -0.5, 0)
        node.addChildNode(rockNode)
        let top = SCNCylinder(radius: 0.98, height: 0.08)
        let topMaterial = SCNMaterial()
        topMaterial.lightingModel = .physicallyBased
        topMaterial.diffuse.contents = NSColor(red: Double(realm.rgb.x) * 0.35, green: Double(realm.rgb.y) * 0.35, blue: Double(realm.rgb.z) * 0.35, alpha: 1)
        topMaterial.emission.contents = color
        topMaterial.emission.intensity = 0.12
        top.materials = [topMaterial]
        node.addChildNode(SCNNode(geometry: top))

        // Il paesaggio ruota piano dentro la cupola.
        let scenery = miniature(realm.id, color: color)
        scenery.runAction(.repeatForever(.rotateBy(x: 0, y: .pi * 2, z: 0, duration: 40)))
        node.addChildNode(scenery)

        // Cupola di vetro e anello alla base.
        let dome = SCNSphere(radius: 1.12)
        dome.segmentCount = 48
        let glass = SCNMaterial()
        glass.lightingModel = .blinn
        glass.diffuse.contents = NSColor(red: Double(realm.rgb.x), green: Double(realm.rgb.y), blue: Double(realm.rgb.z), alpha: 1)
        glass.emission.contents = color
        glass.emission.intensity = 0.12
        glass.specular.contents = NSColor.white
        glass.transparency = 0.11
        glass.isDoubleSided = true
        glass.blendMode = .add
        glass.writesToDepthBuffer = false
        dome.materials = [glass]
        let domeNode = SCNNode(geometry: dome)
        domeNode.simdPosition = SIMD3(0, 0.55, 0)
        node.addChildNode(domeNode)
        let base = SCNTorus(ringRadius: 1.0, pipeRadius: 0.05)
        base.materials = [glow(color, intensity: 1.4)]
        let baseNode = SCNNode(geometry: base)
        baseNode.simdPosition = SIMD3(0, 0.05, 0)
        node.addChildNode(baseNode)

        // Anelli orbitanti, inclinati in modo diverso.
        for (index, tilt) in [0.35, -0.55].enumerated() {
            let ring = SCNTorus(ringRadius: CGFloat(1.45 + Double(index) * 0.22), pipeRadius: 0.02)
            ring.materials = [glow(color, intensity: 1.2)]
            let ringNode = SCNNode(geometry: ring)
            ringNode.simdPosition = SIMD3(0, 0.55, 0)
            ringNode.eulerAngles = SCNVector3(tilt, 0, tilt * 0.6)
            let spin: CGFloat = index == 0 ? 1 : -1
            ringNode.runAction(.repeatForever(.rotateBy(x: 0, y: spin * .pi * 2, z: 0, duration: 7 + Double(index) * 3)))
            node.addChildNode(ringNode)
        }

        // Scintille.
        let sparks = SCNParticleSystem()
        sparks.birthRate = 16
        sparks.particleLifeSpan = 2.4
        sparks.emitterShape = SCNSphere(radius: 1.2)
        sparks.birthLocation = .surface
        sparks.particleSize = 0.07
        sparks.particleColor = color
        sparks.particleVelocity = 0.5
        sparks.blendMode = .additive
        sparks.particleImage = Texture.softDot()
        sparks.isLightingEnabled = false
        let sparkNode = SCNNode()
        sparkNode.simdPosition = SIMD3(0, 0.55, 0)
        sparkNode.addParticleSystem(sparks)
        node.addChildNode(sparkNode)

        // Nome sempre rivolto alla camera.
        let text = SCNText(string: realm.name, extrusionDepth: 0.03)
        text.font = NSFont.systemFont(ofSize: 0.5, weight: .bold)
        text.flatness = 0.02
        text.materials = [glow(NSColor(red: 0.96, green: 0.97, blue: 1, alpha: 1), intensity: 0.9)]
        let label = SCNNode(geometry: text)
        let (minimum, maximum) = label.boundingBox
        label.pivot = SCNMatrix4MakeTranslation((maximum.x + minimum.x) / 2, minimum.y, 0)
        label.simdPosition = SIMD3(0, -1.55, 0)
        let billboard = SCNBillboardConstraint()
        billboard.freeAxes = .Y
        label.constraints = [billboard]
        node.addChildNode(label)

        realmNodes[realm.id] = node
        scene.rootNode.addChildNode(node)
    }

    // MARK: Camera

    private func buildCamera() {
        let lens = SCNCamera()
        lens.fieldOfView = 52
        lens.zNear = 0.1
        lens.zFar = 240
        lens.wantsHDR = true
        lens.bloomIntensity = 1.25
        lens.bloomThreshold = 0.55
        lens.bloomBlurRadius = 14
        camera.camera = lens
        focus.simdPosition = treeFocus
        scene.rootNode.addChildNode(focus)
        let look = SCNLookAtConstraint(target: focus)
        look.isGimbalLockEnabled = true
        camera.constraints = [look]
        camera.simdPosition = orbitPosition(angle: swayAngle())
        scene.rootNode.addChildNode(camera)
    }

    /// Angolo della camera: davanti all'albero (+Z), con un dondolio di circa ±26°.
    private func swayAngle() -> Double { Double.pi / 2 + 0.45 * sin(swayPhase) }

    private func orbitPosition(angle: Double) -> SIMD3<Float> {
        SIMD3(orbitRadius * Float(cos(angle)), orbitHeight, orbitRadius * Float(sin(angle)))
    }

    func renderer(_ renderer: SCNSceneRenderer, updateAtTime time: TimeInterval) {
        defer { lastTime = time }
        guard orbiting, !reduceMotion, lastTime > 0 else { return }
        swayPhase += min(time - lastTime, 0.1) * 0.16
        camera.simdPosition = orbitPosition(angle: swayAngle())
    }

    // MARK: Interazione

    func setHover(_ id: String?) {
        for (key, node) in realmNodes {
            let wanted: CGFloat = key == id ? realmScale * 1.18 : realmScale
            if abs(node.scale.x - wanted) > 0.01 {
                node.runAction(.scale(to: wanted, duration: 0.18), forKey: "hover")
            }
        }
    }

    /// La camera vola davanti al regno scelto.
    func fly(to id: String) {
        guard let realm = Realm.named(id) else { return }
        orbiting = false
        let target = realm.position + SIMD3(0, 0.3, 0)
        let side: Float = target.x < -1 ? -0.7 : (target.x > 1 ? 0.7 : 0)
        let destination = target + SIMD3(side, 0.9, 7.4)
        SCNTransaction.begin()
        SCNTransaction.animationDuration = reduceMotion ? 0.2 : 1.7
        SCNTransaction.animationTimingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
        camera.simdPosition = destination
        focus.simdPosition = target
        SCNTransaction.commit()
    }

    /// Ritorno alla vista d'insieme dell'albero.
    func returnToTree() {
        SCNTransaction.begin()
        SCNTransaction.animationDuration = reduceMotion ? 0.2 : 1.6
        SCNTransaction.animationTimingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
        camera.simdPosition = orbitPosition(angle: swayAngle())
        focus.simdPosition = treeFocus
        SCNTransaction.completionBlock = { [weak self] in self?.orbiting = true }
        SCNTransaction.commit()
    }
}

// MARK: - Vista AppKit

/// SCNView che riconosce il regno sotto il puntatore.
final class RealmSCNView: SCNView {
    var onHover: ((String?) -> Void)?
    var onClick: ((String) -> Void)?

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        for area in trackingAreas { removeTrackingArea(area) }
        addTrackingArea(NSTrackingArea(
            rect: bounds, options: [.activeInKeyWindow, .mouseMoved, .mouseEnteredAndExited, .inVisibleRect],
            owner: self, userInfo: nil))
    }

    override func mouseMoved(with event: NSEvent) {
        super.mouseMoved(with: event)
        onHover?(realmID(at: convert(event.locationInWindow, from: nil)))
    }

    override func mouseExited(with event: NSEvent) {
        super.mouseExited(with: event)
        onHover?(nil)
    }

    override func mouseDown(with event: NSEvent) {
        super.mouseDown(with: event)
        if let id = realmID(at: convert(event.locationInWindow, from: nil)) { onClick?(id) }
    }

    private func realmID(at point: CGPoint) -> String? {
        let options: [SCNHitTestOption: Any] = [.searchMode: SCNHitTestSearchMode.all.rawValue]
        for hit in hitTest(point, options: options) {
            var node: SCNNode? = hit.node
            while let current = node {
                if let name = current.name, name.hasPrefix("realm:") { return String(name.dropFirst(6)) }
                node = current.parent
            }
        }
        return nil
    }
}

private struct YggdrasilSceneView: NSViewRepresentable {
    let stage: YggdrasilStage
    let onHover: (String?) -> Void
    let onSelect: (String) -> Void

    func makeNSView(context: Context) -> RealmSCNView {
        let view = RealmSCNView(frame: .zero, options: nil)
        view.scene = stage.scene
        view.pointOfView = stage.camera
        view.delegate = stage
        view.antialiasingMode = .multisampling4X
        view.preferredFramesPerSecond = 60
        view.isPlaying = true
        view.rendersContinuously = true
        view.allowsCameraControl = false
        view.onHover = onHover
        view.onClick = onSelect
        return view
    }

    func updateNSView(_ view: RealmSCNView, context: Context) {
        view.onHover = onHover
        view.onClick = onSelect
    }
}

// MARK: - Schermata Yggdrasil

/// L'albero come menu: passa il mouse su un regno per leggerne il nome, toccalo per entrare e scegliere una funzione.
struct YggdrasilHome: View {
    let functions: (String) -> [YggFunction]
    let open: (String) -> Void
    /// Regno aperto: si può cambiare anche da fuori (il quadro dei regni).
    @Binding var selected: String?
    var minHeight: CGFloat = 560
    @StateObject private var stage = YggdrasilStage()
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var hovered: String?

    var body: some View {
        ZStack {
            YggdrasilSceneView(stage: stage, onHover: hover, onSelect: choose)
                .accessibilityHidden(true)
            // Vignettatura: scurisce i bordi e fa risaltare l'albero.
            RadialGradient(colors: [.clear, .black.opacity(0.55)], center: .center, startRadius: 180, endRadius: 560)
                .allowsHitTesting(false)
            VStack(alignment: .leading, spacing: 0) {
                header
                Spacer(minLength: 0)
                realmBar
            }
            .padding(18)
            if let id = selected, let realm = Realm.named(id) {
                HStack {
                    Spacer()
                    panel(realm)
                }
                .padding(.vertical, 76).padding(.trailing, 14)
                .transition(.move(edge: .trailing).combined(with: .opacity))
            }
        }
        .frame(minHeight: minHeight)
        .clipShape(RoundedRectangle(cornerRadius: 20, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 20, style: .continuous).strokeBorder(
                LinearGradient(colors: [Color(red: 1, green: 0.82, blue: 0.4).opacity(0.55), .white.opacity(0.08),
                                        Color(red: 0.4, green: 0.85, blue: 1).opacity(0.45)],
                               startPoint: .topLeading, endPoint: .bottomTrailing), lineWidth: 1.2)
        }
        .shadow(color: Color(red: 0.3, green: 0.7, blue: 1).opacity(0.12), radius: 16, y: 6)
        .onAppear { stage.reduceMotion = reduceMotion }
        .onChange(of: reduceMotion) { _, value in stage.reduceMotion = value }
        .onChange(of: selected) { _, value in
            if let value { stage.fly(to: value) } else { stage.returnToTree() }
        }
        .animation(reduceMotion ? nil : .smooth(duration: 0.35), value: selected)
    }

    private func hover(_ id: String?) {
        guard hovered != id else { return }
        hovered = id
        stage.setHover(id)
    }

    private func choose(_ id: String) { selected = id }

    private func close() { selected = nil }

    private var header: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text("YGGDRASILL")
                .font(.system(size: 30, weight: .heavy, design: .rounded))
                .tracking(6)
                .foregroundStyle(LinearGradient(colors: [Color(red: 1, green: 0.86, blue: 0.5), Color(red: 0.5, green: 0.95, blue: 0.7)],
                                                startPoint: .leading, endPoint: .trailing))
            if let id = hovered ?? selected, let realm = Realm.named(id) {
                HStack(spacing: 8) {
                    Circle().fill(realm.color).frame(width: 9, height: 9)
                    Text(realm.name).font(.headline).foregroundStyle(realm.color)
                    Text("· \(realm.epithet) · \(functions(realm.id).count) funzioni")
                        .font(.subheadline).foregroundStyle(.secondary)
                }
                .transition(.opacity)
            } else {
                Text("Il cuore di JARVIS · tocca un regno per entrare")
                    .font(.subheadline).foregroundStyle(.secondary)
            }
        }
        .shadow(color: .black.opacity(0.6), radius: 6)
    }

    /// Elenco dei nove regni: un'alternativa al clic sulla scena, utile con la tastiera e VoiceOver.
    private var realmBar: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(Realm.all) { realm in
                    Button { choose(realm.id) } label: {
                        HStack(spacing: 6) {
                            Image(systemName: realm.icon).font(.caption)
                            Text(realm.name).font(.caption.weight(.semibold))
                        }
                        .padding(.horizontal, 11).padding(.vertical, 7)
                        .background(selected == realm.id ? realm.color.opacity(0.30) : Color.black.opacity(0.42), in: Capsule())
                        .overlay { Capsule().strokeBorder(realm.color.opacity(selected == realm.id ? 0.9 : 0.35)) }
                        .foregroundStyle(realm.color)
                    }
                    .buttonStyle(PressableStyle())
                    .onHover { inside in hover(inside ? realm.id : (hovered == realm.id ? nil : hovered)) }
                    .accessibilityLabel("\(realm.name): \(realm.epithet)")
                }
            }
            .padding(.vertical, 2)
        }
    }

    private func panel(_ realm: Realm) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 10) {
                Image(systemName: realm.icon).font(.title2).foregroundStyle(realm.color)
                VStack(alignment: .leading, spacing: 0) {
                    Text(realm.name).font(.title3.weight(.bold))
                    Text(realm.epithet).font(.caption).foregroundStyle(.secondary)
                }
                Spacer()
                Button { close() } label: { Image(systemName: "xmark.circle.fill").font(.title3) }
                    .buttonStyle(.plain).foregroundStyle(.secondary)
                    .help("Torna all'albero").accessibilityLabel("Torna all'albero")
            }
            Divider().overlay(realm.color.opacity(0.4))
            ScrollView {
                VStack(spacing: 8) {
                    ForEach(functions(realm.id)) { function in
                        Button { open(function.id) } label: {
                            HStack(spacing: 11) {
                                Image(systemName: function.icon).frame(width: 24).foregroundStyle(realm.color)
                                VStack(alignment: .leading, spacing: 1) {
                                    Text(function.title).font(.callout.weight(.semibold))
                                    Text(function.subtitle).font(.caption).foregroundStyle(.secondary).lineLimit(2)
                                        .multilineTextAlignment(.leading)
                                }
                                Spacer(minLength: 0)
                                Image(systemName: "chevron.right").font(.caption2).foregroundStyle(.tertiary)
                            }
                            .padding(10)
                            .background(.white.opacity(0.06), in: RoundedRectangle(cornerRadius: 11, style: .continuous))
                            .overlay { RoundedRectangle(cornerRadius: 11, style: .continuous).strokeBorder(realm.color.opacity(0.25)) }
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(PressableStyle())
                    }
                }
            }
        }
        .padding(16)
        .frame(width: 290)
        .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
        .overlay { RoundedRectangle(cornerRadius: 18, style: .continuous).strokeBorder(realm.color.opacity(0.45)) }
        .shadow(color: realm.color.opacity(0.25), radius: 18)
    }
}

// MARK: - Illustrazioni dei regni (vettoriali, disegnate con Canvas: nessun file immagine)

/// Piccolo paesaggio stilizzato di ogni regno: cielo sfumato, sagome e un tocco di luce nel colore del regno.
struct RealmArt: View {
    let realm: Realm

    var body: some View {
        Canvas { context, size in
            let w = size.width, h = size.height
            let c = realm.color
            // Cielo.
            context.fill(Path(CGRect(origin: .zero, size: size)),
                         with: .linearGradient(Gradient(colors: [c.opacity(0.10), c.opacity(0.42)]),
                                               startPoint: .zero, endPoint: CGPoint(x: 0, y: h)))
            // Astro o luce principale.
            let sun = CGRect(x: w * 0.68, y: h * 0.14, width: h * 0.34, height: h * 0.34)
            context.fill(Path(ellipseIn: sun.insetBy(dx: -h * 0.10, dy: -h * 0.10)), with: .color(c.opacity(0.18)))
            context.fill(Path(ellipseIn: sun), with: .color(.white.opacity(0.80)))
            // Sagome di fondo e di primo piano, diverse per regno.
            func ridge(_ points: [(Double, Double)], _ color: Color) {
                var p = Path()
                p.move(to: CGPoint(x: 0, y: h))
                for (x, y) in points { p.addLine(to: CGPoint(x: w * x, y: h * y)) }
                p.addLine(to: CGPoint(x: w, y: h))
                p.closeSubpath()
                context.fill(p, with: .color(color))
            }
            let dark = Color.black
            switch realm.id {
            case "asgard":      // torri dorate
                ridge([(0, 0.78), (0.2, 0.55), (0.25, 0.30), (0.30, 0.55), (0.5, 0.62), (0.62, 0.36), (0.68, 0.62), (1, 0.72)], dark.opacity(0.45))
            case "midgard":     // colline
                ridge([(0, 0.70), (0.25, 0.52), (0.5, 0.68), (0.75, 0.50), (1, 0.66)], dark.opacity(0.38))
                ridge([(0, 0.88), (0.4, 0.74), (1, 0.86)], dark.opacity(0.55))
            case "alfheim":     // cupole di luce
                ridge([(0, 0.80), (0.15, 0.50), (0.30, 0.80), (0.5, 0.58), (0.70, 0.80), (0.85, 0.52), (1, 0.80)], dark.opacity(0.35))
            case "vanaheim":    // foresta
                ridge([(0, 0.8), (0.1, 0.45), (0.2, 0.8), (0.32, 0.40), (0.44, 0.8), (0.58, 0.46), (0.70, 0.8), (0.84, 0.42), (1, 0.8)], dark.opacity(0.45))
            case "jotunheim":   // montagne
                ridge([(0, 0.85), (0.22, 0.28), (0.45, 0.80), (0.62, 0.38), (0.85, 0.82), (1, 0.62)], dark.opacity(0.48))
            case "svartalfheim": // forge e ingranaggi
                ridge([(0, 0.84), (0.1, 0.60), (0.3, 0.60), (0.34, 0.84), (0.5, 0.52), (0.7, 0.52), (0.75, 0.84), (1, 0.66)], dark.opacity(0.52))
            case "muspelheim":  // vulcano
                ridge([(0, 0.86), (0.35, 0.60), (0.5, 0.34), (0.58, 0.42), (0.66, 0.34), (0.82, 0.62), (1, 0.86)], dark.opacity(0.55))
            case "helheim":     // nebbia e lapidi
                ridge([(0, 0.76), (0.2, 0.76), (0.2, 0.55), (0.3, 0.55), (0.3, 0.76), (0.6, 0.76), (0.6, 0.48), (0.72, 0.48), (0.72, 0.76), (1, 0.76)], dark.opacity(0.50))
            default:            // niflheim: ghiaccio
                ridge([(0, 0.84), (0.12, 0.50), (0.2, 0.84), (0.38, 0.40), (0.5, 0.84), (0.7, 0.46), (0.82, 0.84), (1, 0.60)], dark.opacity(0.40))
            }
            // Luce radente sul terreno.
            context.fill(Path(CGRect(x: 0, y: h * 0.90, width: w, height: h * 0.10)), with: .color(c.opacity(0.35)))
        }
        .accessibilityHidden(true)
    }
}
