import AppKit
import SceneKit
import SwiftUI
import simd

// Yggdrasil (ADR-0232): l'albero dei nove regni come menu in 3D.
// Ogni regno raccoglie un gruppo di funzioni dell'app. Questo file non conosce le pagine dell'app: riceve dal
// chiamante l'elenco delle funzioni di ciascun regno e restituisce quella scelta.

// MARK: - Regni

/// Un regno: nome, epiteto, colore, simbolo e posizione sull'albero (angolo in gradi, distanza dal tronco, altezza).
struct Realm: Identifiable {
    let id: String
    let name: String
    let epithet: String
    let icon: String
    let rgb: SIMD3<Double>
    let angle: Float
    let radius: Float
    let height: Float

    var color: Color { Color(red: rgb.x, green: rgb.y, blue: rgb.z) }
    var nsColor: NSColor { NSColor(red: rgb.x, green: rgb.y, blue: rgb.z, alpha: 1) }

    /// I nove regni: tre in alto (la chioma), tre al centro, tre in basso (le radici).
    static let all: [Realm] = [
        Realm(id: "asgard", name: "Asgard", epithet: "La sala del comando", icon: "crown",
              rgb: SIMD3(1.00, 0.80, 0.32), angle: 90, radius: 4.2, height: 10.6),
        Realm(id: "alfheim", name: "Alfheim", epithet: "Il regno dei collegamenti", icon: "link",
              rgb: SIMD3(0.62, 0.92, 1.00), angle: 210, radius: 4.2, height: 10.2),
        Realm(id: "vanaheim", name: "Vanaheim", epithet: "Memoria e crescita", icon: "leaf",
              rgb: SIMD3(0.30, 0.92, 0.62), angle: 330, radius: 4.2, height: 10.4),
        Realm(id: "midgard", name: "Midgard", epithet: "Il tuo mondo", icon: "globe.europe.africa",
              rgb: SIMD3(0.42, 0.78, 0.40), angle: 150, radius: 5.6, height: 6.3),
        Realm(id: "jotunheim", name: "Jotunheim", epithet: "Le grandi imprese", icon: "snowflake",
              rgb: SIMD3(0.46, 0.58, 1.00), angle: 270, radius: 5.6, height: 6.1),
        Realm(id: "muspelheim", name: "Muspelheim", epithet: "Fuoco, ricerca e avvisi", icon: "flame",
              rgb: SIMD3(1.00, 0.34, 0.18), angle: 30, radius: 5.6, height: 6.4),
        Realm(id: "nidavellir", name: "Nidavellir", epithet: "Le fucine dei modelli", icon: "hammer",
              rgb: SIMD3(0.96, 0.62, 0.26), angle: 90, radius: 4.8, height: 2.5),
        Realm(id: "svartalfheim", name: "Svartalfheim", epithet: "La forgia delle automazioni", icon: "gearshape.2",
              rgb: SIMD3(0.72, 0.42, 0.96), angle: 210, radius: 4.8, height: 2.3),
        Realm(id: "helheim", name: "Helheim", epithet: "Sicurezza e custodia", icon: "shield.lefthalf.filled",
              rgb: SIMD3(0.56, 0.74, 0.76), angle: 330, radius: 4.8, height: 2.4)
    ]

    static func named(_ key: String) -> Realm? {
        let lowered = key.lowercased()
        return all.first { $0.id == lowered || $0.name.lowercased() == lowered }
    }

    /// Posizione del regno nello spazio della scena.
    var position: SIMD3<Float> {
        let radians = angle * .pi / 180
        return SIMD3(radius * cos(radians), height, radius * sin(radians))
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

    /// Corteccia: fondo scuro con venature verticali e qualche bagliore dorato.
    static func bark() -> NSImage {
        return NSImage(size: NSSize(width: 256, height: 256), flipped: false) { rect in
            var generator = SeededRandom(seed: 7)
            NSColor(red: 0.13, green: 0.085, blue: 0.055, alpha: 1).setFill()
            rect.fill()
            for _ in 0..<420 {
                let x = CGFloat.random(in: 0...256, using: &generator)
                let width = CGFloat.random(in: 0.6...2.8, using: &generator)
                let shade = CGFloat.random(in: 0.02...0.16, using: &generator)
                NSColor(red: shade + 0.04, green: shade * 0.7 + 0.02, blue: shade * 0.45, alpha: 0.75).setFill()
                NSRect(x: x, y: 0, width: width, height: 256).fill()
            }
            for _ in 0..<46 {
                let x = CGFloat.random(in: 0...256, using: &generator)
                let y = CGFloat.random(in: 0...256, using: &generator)
                NSColor(red: 0.95, green: 0.68, blue: 0.22, alpha: CGFloat.random(in: 0.25...0.8, using: &generator)).setFill()
                NSRect(x: x, y: y, width: 1.4, height: CGFloat.random(in: 4...22, using: &generator)).fill()
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

// MARK: - Scena

/// La scena 3D: costruisce l'albero, gestisce orbita della camera, passaggio del mouse e volo verso un regno.
final class YggdrasilStage: NSObject, ObservableObject, SCNSceneRendererDelegate {
    let scene = SCNScene()
    let camera = SCNNode()
    private let focus = SCNNode()
    private var realmNodes: [String: SCNNode] = [:]
    private var orbiting = true
    private var orbitAngle: Double = 0.9
    private var lastTime: TimeInterval = 0
    private let orbitRadius: Float = 25
    private let orbitHeight: Float = 7.5
    private let treeFocus = SIMD3<Float>(0, 6.4, 0)
    /// Con «riduci movimento» l'orbita resta ferma e i voli sono quasi immediati.
    var reduceMotion = false

    override init() {
        super.init()
        buildEnvironment()
        buildTree()
        Realm.all.forEach { buildRealm($0) }
        buildCamera()
    }

    // MARK: Ambiente

    private func buildEnvironment() {
        scene.background.contents = Texture.sky()
        scene.fogColor = NSColor(red: 0.02, green: 0.05, blue: 0.07, alpha: 1)
        scene.fogStartDistance = 28
        scene.fogEndDistance = 95
        scene.fogDensityExponent = 1.2

        let ambient = SCNNode()
        ambient.light = SCNLight()
        ambient.light?.type = .ambient
        ambient.light?.color = NSColor(red: 0.20, green: 0.26, blue: 0.34, alpha: 1)
        ambient.light?.intensity = 420
        scene.rootNode.addChildNode(ambient)

        let moon = SCNNode()
        moon.light = SCNLight()
        moon.light?.type = .directional
        moon.light?.color = NSColor(red: 0.62, green: 0.74, blue: 1.0, alpha: 1)
        moon.light?.intensity = 700
        moon.eulerAngles = SCNVector3(-0.8, 0.6, 0)
        scene.rootNode.addChildNode(moon)

        let crown = SCNNode()
        crown.simdPosition = SIMD3(0, 12.5, 0)
        crown.light = SCNLight()
        crown.light?.type = .omni
        crown.light?.color = NSColor(red: 1.0, green: 0.82, blue: 0.5, alpha: 1)
        crown.light?.intensity = 900
        crown.light?.attenuationStartDistance = 2
        crown.light?.attenuationEndDistance = 24
        scene.rootNode.addChildNode(crown)

        scene.rootNode.addChildNode(stars())
        scene.rootNode.addChildNode(well())
        scene.rootNode.addChildNode(mist())
    }

    private func stars() -> SCNNode {
        var generator = SeededRandom(seed: 99)
        var vertices: [SIMD3<Float>] = []
        for _ in 0..<1100 {
            let theta = Float.random(in: 0...(2 * .pi), using: &generator)
            let phi = acos(Float.random(in: -0.15...1, using: &generator))
            let r: Float = 80
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
        element.maximumPointScreenSpaceRadius = 1.8
        let geometry = SCNGeometry(sources: [source], elements: [element])
        let material = SCNMaterial()
        material.lightingModel = .constant
        material.diffuse.contents = NSColor(red: 0.85, green: 0.92, blue: 1.0, alpha: 1)
        geometry.materials = [material]
        return SCNNode(geometry: geometry)
    }

    /// Mímisbrunnr: la sorgente ai piedi dell'albero.
    private func well() -> SCNNode {
        let node = SCNNode()
        let disc = SCNCylinder(radius: 6.2, height: 0.05)
        let water = SCNMaterial()
        water.lightingModel = .constant
        water.diffuse.contents = NSColor(red: 0.03, green: 0.10, blue: 0.16, alpha: 1)
        water.emission.contents = NSColor(red: 0.04, green: 0.20, blue: 0.32, alpha: 1)
        disc.materials = [water]
        let floor = SCNNode(geometry: disc)
        floor.simdPosition = SIMD3(0, -0.9, 0)
        node.addChildNode(floor)
        for (index, radius) in [3.2, 4.6, 6.0].enumerated() {
            let ring = SCNTorus(ringRadius: CGFloat(radius), pipeRadius: 0.025)
            let glow = SCNMaterial()
            glow.lightingModel = .constant
            glow.diffuse.contents = NSColor(red: 0.35, green: 0.85, blue: 1.0, alpha: 1)
            glow.emission.contents = NSColor(red: 0.30, green: 0.80, blue: 1.0, alpha: 1)
            ring.materials = [glow]
            let ringNode = SCNNode(geometry: ring)
            ringNode.simdPosition = SIMD3(0, -0.86, 0)
            ringNode.opacity = 0.55 - CGFloat(index) * 0.14
            node.addChildNode(ringNode)
        }
        return node
    }

    private func mist() -> SCNNode {
        let plane = SCNPlane(width: 70, height: 70)
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
        node.simdPosition = SIMD3(0, -0.7, 0)
        node.opacity = 0.55
        return node
    }

    // MARK: Albero

    private func segment(_ a: SIMD3<Float>, _ b: SIMD3<Float>, _ r1: Float, _ r2: Float, _ material: SCNMaterial) -> SCNNode {
        let direction = b - a
        let length = simd_length(direction)
        let cone = SCNCone(topRadius: CGFloat(r2), bottomRadius: CGFloat(r1), height: CGFloat(max(length, 0.001)))
        cone.radialSegmentCount = 14
        cone.materials = [material]
        let node = SCNNode(geometry: cone)
        node.simdPosition = (a + b) / 2
        node.simdOrientation = simd_quatf(from: SIMD3<Float>(0, 1, 0), to: simd_normalize(direction))
        return node
    }

    private func bezier(_ p0: SIMD3<Float>, _ p1: SIMD3<Float>, _ p2: SIMD3<Float>, _ p3: SIMD3<Float>, _ t: Float) -> SIMD3<Float> {
        let u = 1 - t
        return (u * u * u) * p0 + (3 * u * u * t) * p1 + (3 * u * t * t) * p2 + (t * t * t) * p3
    }

    /// Tubo rastremato lungo una curva di Bézier, con giunture sferiche.
    private func tube(_ p0: SIMD3<Float>, _ p1: SIMD3<Float>, _ p2: SIMD3<Float>, _ p3: SIMD3<Float>,
                      from startRadius: Float, to endRadius: Float, steps: Int, material: SCNMaterial) -> SCNNode {
        let group = SCNNode()
        var previous = p0
        for step in 1...steps {
            let t = Float(step) / Float(steps)
            let point = bezier(p0, p1, p2, p3, t)
            let r1 = startRadius + (endRadius - startRadius) * Float(step - 1) / Float(steps)
            let r2 = startRadius + (endRadius - startRadius) * t
            group.addChildNode(segment(previous, point, r1, r2, material))
            let joint = SCNSphere(radius: CGFloat(r2))
            joint.segmentCount = 12
            joint.materials = [material]
            let jointNode = SCNNode(geometry: joint)
            jointNode.simdPosition = point
            group.addChildNode(jointNode)
            previous = point
        }
        return group
    }

    private func barkMaterial() -> SCNMaterial {
        let material = SCNMaterial()
        material.lightingModel = .physicallyBased
        material.diffuse.contents = Texture.bark()
        material.diffuse.wrapS = .repeat
        material.diffuse.wrapT = .repeat
        material.diffuse.contentsTransform = SCNMatrix4MakeScale(2, 4, 1)
        material.roughness.contents = 0.92
        material.metalness.contents = 0.05
        material.emission.contents = NSColor(red: 0.20, green: 0.11, blue: 0.03, alpha: 1)
        return material
    }

    private func buildTree() {
        let bark = barkMaterial()
        let tree = SCNNode()
        tree.name = "yggdrasil"

        // Tronco: curva lieve, più sottile verso la chioma.
        let trunk = tube(SIMD3(0, -0.8, 0), SIMD3(0.5, 4, -0.3), SIMD3(-0.45, 8.5, 0.3), SIMD3(0, 13.2, 0),
                         from: 1.55, to: 0.5, steps: 22, material: bark)
        tree.addChildNode(trunk)

        // Radici che affondano nella sorgente.
        var generator = SeededRandom(seed: 21)
        for index in 0..<8 {
            let angle = Float(index) * (2 * .pi / 8) + Float.random(in: -0.2...0.2, using: &generator)
            let reach = Float.random(in: 4.2...6.2, using: &generator)
            let end = SIMD3(reach * cos(angle), -0.85, reach * sin(angle))
            let mid1 = SIMD3(0.8 * cos(angle), -0.1, 0.8 * sin(angle))
            let mid2 = SIMD3(reach * 0.65 * cos(angle), -0.9, reach * 0.65 * sin(angle))
            tree.addChildNode(tube(SIMD3(0, 0.6, 0), mid1, mid2, end, from: 0.85, to: 0.07, steps: 9, material: bark))
        }

        // Un ramo per ogni regno.
        for realm in Realm.all {
            let target = realm.position
            let angle = realm.angle * .pi / 180
            let start = SIMD3<Float>(0, max(realm.height - 2.6, 1.6), 0)
            let c1 = SIMD3<Float>(realm.radius * 0.30 * cos(angle), start.y + 0.9, realm.radius * 0.30 * sin(angle))
            let c2 = SIMD3<Float>(realm.radius * 0.74 * cos(angle), target.y - 0.5, realm.radius * 0.74 * sin(angle))
            tree.addChildNode(tube(start, c1, c2, target - SIMD3(0, 0.55, 0),
                                   from: 0.36, to: 0.06, steps: 14, material: bark))
            // Qualche rametto secondario, per un aspetto meno geometrico.
            for twig in 0..<3 {
                let t = Float(twig + 2) / 6
                let origin = bezier(start, c1, c2, target, t)
                let spread = Float.random(in: -1.2...1.2, using: &generator)
                let end = origin + SIMD3(1.4 * cos(angle + spread), Float.random(in: 0.5...1.5, using: &generator), 1.4 * sin(angle + spread))
                tree.addChildNode(tube(origin, origin + (end - origin) * 0.4 + SIMD3(0, 0.4, 0), origin + (end - origin) * 0.8, end,
                                       from: 0.07, to: 0.015, steps: 4, material: bark))
            }
        }
        scene.rootNode.addChildNode(tree)

        // Chioma: foglie luminose che scendono piano; lucciole attorno al tronco.
        let canopy = SCNNode()
        canopy.simdPosition = SIMD3(0, 10.5, 0)
        canopy.addParticleSystem(drifting(color: NSColor(red: 1.0, green: 0.82, blue: 0.40, alpha: 0.9), radius: 7.5, rate: 70, size: 0.13))
        canopy.addParticleSystem(drifting(color: NSColor(red: 0.45, green: 0.95, blue: 0.60, alpha: 0.8), radius: 6.5, rate: 40, size: 0.10))
        scene.rootNode.addChildNode(canopy)
        let fireflies = SCNNode()
        fireflies.simdPosition = SIMD3(0, 4, 0)
        fireflies.addParticleSystem(drifting(color: NSColor(red: 0.62, green: 0.95, blue: 1.0, alpha: 0.9), radius: 8, rate: 26, size: 0.08))
        scene.rootNode.addChildNode(fireflies)
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

    // MARK: Regni

    /// Forma distintiva al centro di ogni regno.
    private func emblem(_ id: String) -> SCNGeometry {
        switch id {
        case "asgard": return SCNPyramid(width: 0.9, height: 1.3, length: 0.9)
        case "midgard": return SCNSphere(radius: 0.46)
        case "alfheim": return SCNCapsule(capRadius: 0.26, height: 1.15)
        case "vanaheim": return SCNTorus(ringRadius: 0.44, pipeRadius: 0.17)
        case "jotunheim": return SCNBox(width: 0.8, height: 0.8, length: 0.8, chamferRadius: 0.06)
        case "svartalfheim": return SCNCylinder(radius: 0.34, height: 0.9)
        case "nidavellir": return SCNBox(width: 1.0, height: 0.5, length: 0.6, chamferRadius: 0.05)
        case "helheim": return SCNTube(innerRadius: 0.2, outerRadius: 0.44, height: 0.9)
        default: return SCNCone(topRadius: 0, bottomRadius: 0.5, height: 1.15)
        }
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
        let color = realm.nsColor

        // Emblema metallico, luminoso, che ruota piano.
        let core = SCNNode(geometry: emblem(realm.id))
        let metal = SCNMaterial()
        metal.lightingModel = .physicallyBased
        metal.diffuse.contents = color
        metal.metalness.contents = 0.6
        metal.roughness.contents = 0.3
        metal.emission.contents = color
        metal.emission.intensity = 0.8
        core.geometry?.materials = [metal]
        core.runAction(.repeatForever(.rotateBy(x: 0, y: .pi * 2, z: 0, duration: 14)))
        node.addChildNode(core)

        // Guscio di luce.
        let shellGeometry = SCNSphere(radius: 0.95)
        shellGeometry.segmentCount = 40
        let shellMaterial = glow(color, intensity: 0.5)
        shellMaterial.transparency = 0.16
        shellMaterial.isDoubleSided = true
        shellMaterial.blendMode = .add
        shellMaterial.writesToDepthBuffer = false
        shellGeometry.materials = [shellMaterial]
        node.addChildNode(SCNNode(geometry: shellGeometry))

        // Anelli orbitanti, inclinati in modo diverso.
        for (index, tilt) in [0.35, -0.55].enumerated() {
            let ring = SCNTorus(ringRadius: CGFloat(1.25 + Double(index) * 0.22), pipeRadius: 0.022)
            ring.materials = [glow(color, intensity: 1.2)]
            let ringNode = SCNNode(geometry: ring)
            ringNode.eulerAngles = SCNVector3(tilt, 0, tilt * 0.6)
            let spin: CGFloat = index == 0 ? 1 : -1
            ringNode.runAction(.repeatForever(.rotateBy(x: 0, y: spin * .pi * 2, z: 0, duration: 7 + Double(index) * 3)))
            node.addChildNode(ringNode)
        }

        // Luce colorata che tinge i rami vicini.
        let light = SCNNode()
        light.light = SCNLight()
        light.light?.type = .omni
        light.light?.color = color
        light.light?.intensity = 520
        light.light?.attenuationStartDistance = 0.5
        light.light?.attenuationEndDistance = 9
        node.addChildNode(light)

        // Scintille.
        let sparks = SCNParticleSystem()
        sparks.birthRate = 16
        sparks.particleLifeSpan = 2.4
        sparks.emitterShape = SCNSphere(radius: 1.0)
        sparks.birthLocation = .surface
        sparks.particleSize = 0.07
        sparks.particleColor = color
        sparks.particleVelocity = 0.5
        sparks.blendMode = .additive
        sparks.particleImage = Texture.softDot()
        sparks.isLightingEnabled = false
        node.addParticleSystem(sparks)

        // Nome sempre rivolto alla camera.
        let text = SCNText(string: realm.name, extrusionDepth: 0.03)
        text.font = NSFont.systemFont(ofSize: 0.5, weight: .bold)
        text.flatness = 0.02
        text.materials = [glow(NSColor(red: 0.96, green: 0.97, blue: 1, alpha: 1), intensity: 0.9)]
        let label = SCNNode(geometry: text)
        let (minimum, maximum) = label.boundingBox
        label.pivot = SCNMatrix4MakeTranslation((maximum.x + minimum.x) / 2, minimum.y, 0)
        label.simdPosition = SIMD3(0, -1.9, 0)
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
        lens.zFar = 220
        lens.wantsHDR = true
        lens.bloomIntensity = 1.3
        lens.bloomThreshold = 0.55
        lens.bloomBlurRadius = 14
        camera.camera = lens
        focus.simdPosition = treeFocus
        scene.rootNode.addChildNode(focus)
        let look = SCNLookAtConstraint(target: focus)
        look.isGimbalLockEnabled = true
        camera.constraints = [look]
        camera.simdPosition = orbitPosition(angle: orbitAngle)
        scene.rootNode.addChildNode(camera)
    }

    private func orbitPosition(angle: Double) -> SIMD3<Float> {
        SIMD3(orbitRadius * Float(cos(angle)), orbitHeight, orbitRadius * Float(sin(angle)))
    }

    func renderer(_ renderer: SCNSceneRenderer, updateAtTime time: TimeInterval) {
        defer { lastTime = time }
        guard orbiting, !reduceMotion, lastTime > 0 else { return }
        orbitAngle += min(time - lastTime, 0.1) * 0.05
        camera.simdPosition = orbitPosition(angle: orbitAngle)
    }

    // MARK: Interazione

    func setHover(_ id: String?) {
        for (key, node) in realmNodes {
            let wanted: CGFloat = key == id ? 1.22 : 1
            if abs(node.scale.x - wanted) > 0.01 {
                node.runAction(.scale(to: wanted, duration: 0.18), forKey: "hover")
            }
        }
    }

    /// La camera vola davanti al regno scelto.
    func fly(to id: String) {
        guard let realm = Realm.named(id) else { return }
        orbiting = false
        let target = realm.position
        let outward = simd_normalize(SIMD3(target.x, 0, target.z))
        let destination = target + outward * 5.6 + SIMD3(0, 0.7, 0)
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
        camera.simdPosition = orbitPosition(angle: orbitAngle)
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
    @StateObject private var stage = YggdrasilStage()
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var hovered: String?
    @State private var selected: String?

    var body: some View {
        ZStack {
            YggdrasilSceneView(stage: stage, onHover: hover, onSelect: choose)
                .accessibilityHidden(true)
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
                .padding(.vertical, 70).padding(.trailing, 18)
                .transition(.move(edge: .trailing).combined(with: .opacity))
            }
        }
        .frame(height: 700)
        .clipShape(RoundedRectangle(cornerRadius: 20, style: .continuous))
        .overlay { RoundedRectangle(cornerRadius: 20, style: .continuous).strokeBorder(.white.opacity(0.10)) }
        .onAppear { stage.reduceMotion = reduceMotion }
        .onChange(of: reduceMotion) { _, value in stage.reduceMotion = value }
        .animation(reduceMotion ? nil : .smooth(duration: 0.35), value: selected)
    }

    private func hover(_ id: String?) {
        guard hovered != id else { return }
        hovered = id
        stage.setHover(id)
    }

    private func choose(_ id: String) {
        selected = id
        stage.fly(to: id)
    }

    private func close() {
        selected = nil
        stage.returnToTree()
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text("YGGDRASIL")
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
                Text("L'albero dei nove regni · tocca un regno per entrare")
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
        .frame(width: 330)
        .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
        .overlay { RoundedRectangle(cornerRadius: 18, style: .continuous).strokeBorder(realm.color.opacity(0.45)) }
        .shadow(color: realm.color.opacity(0.25), radius: 18)
    }
}
