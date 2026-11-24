package local.jarvis.app

import android.provider.Settings
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.withTransform
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.floor
import kotlin.math.max
import kotlin.math.min
import kotlin.math.sin

private val Cyan = Color(0xFF40E0FF)
private val Alloy = Color(0xFF365C7D)
private val Deep = Color(0xFF0A1828)
private val Visor = Color(0xFF122E45)
private val Indigo = Color(0xFF5C60D6)

/** Funzioni pure della scena, separate dal disegno così si possono provare senza schermo. */
internal object AvatarMath {
    fun unit(value: Float): Float = value - floor(value)

    /** Rumore deterministico in [0, 1): stessa sequenza a ogni avvio. */
    fun hash(value: Float): Float {
        val raw = sin(value.toDouble() * 127.1 + 311.7) * 43758.5453
        return (raw - floor(raw)).toFloat()
    }

    fun smooth(value: Float): Float {
        val c = value.coerceIn(0f, 1f)
        return c * c * (3f - 2f * c)
    }

    /** Altezza relativa della palpebra: 1 aperta, vicino a 0 a metà ammiccamento (ogni 4,9 s). */
    fun blink(time: Float): Float {
        val phase = time % 4.9f
        return if (phase < 0.16f) abs(phase - 0.08f) / 0.08f else 1f
    }

    /** Luminosità di una tacca (0–1) in base alla distanza angolare dall'onda di luce. */
    fun tickGlow(angle: Float, sweep: Float): Float {
        var delta = abs(angle - sweep)
        if (delta > 180f) delta = 360f - delta
        return max(0f, 1f - delta / 50f)
    }

    /** Altezza di una barra della bocca: sillabe quando parla, onda di luce quando elabora. */
    fun mouthHeight(time: Float, index: Float, think: Float, talk: Float): Float {
        val speech = abs(sin(time * 13f + index * 1.7f)) * abs(sin(time * 5.3f + index * 0.9f)) * 10f
        val wave = (0.5f + 0.5f * sin(time * 6f - index * 0.9f)) * 4f
        return 2f + talk * speech + (1f - talk) * think * wave
    }

    private val profile = listOf(
        -87f to 24f, -74f to 43f, -43f to 57f, 12f to 48f, 42f to 26f, 43f to 0f, 200f to 0f,
    )

    /** Mezza larghezza della testa all'altezza [y] (spazio dell'avatar), per ritagliare la scansione. */
    fun headHalfWidth(y: Float): Float {
        if (y <= profile.first().first) return 0f
        for (i in 0 until profile.size - 1) {
            val (y0, w0) = profile[i]
            val (y1, w1) = profile[i + 1]
            if (y <= y1) return w0 + (w1 - w0) * ((y - y0) / (y1 - y0))
        }
        return 0f
    }
}

private val shoulders = listOf(
    -101f to 98f, -91f to 65f, -43f to 45f, 41f to 45f, 92f to 65f, 101f to 98f, 66f to 110f, -66f to 110f,
)
private val neck = listOf(-30f to 59f, -23f to 34f, 23f to 34f, 30f to 59f, 0f to 78f)
private val head = listOf(
    -57f to -43f, -43f to -74f, -24f to -87f, 24f to -87f, 43f to -74f,
    57f to -43f, 48f to 12f, 26f to 42f, -26f to 42f, -48f to 12f,
)
private val eyeBand = listOf(-52f to -40f, -34f to -55f, 34f to -55f, 52f to -40f, 41f to -17f, -41f to -17f)

/**
 * Avatar di JARVIS (ADR-0129): la stessa testa olografica di macOS e Windows, disegnata con Canvas.
 * Tre stati continui: riposo, elaborazione ([thinking]) e parlato ([speaking]); un tocco emette un'onda.
 * Se l'utente ha disattivato le animazioni di sistema resta un disegno fermo.
 */
@Composable
fun JarvisAvatar(modifier: Modifier = Modifier, thinking: Boolean = false, speaking: Boolean = false) {
    val context = LocalContext.current
    val still = remember {
        Settings.Global.getFloat(context.contentResolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f
    }
    var time by remember { mutableFloatStateOf(0f) }
    var tapped by remember { mutableFloatStateOf(-1000f) }
    val think by animateFloatAsState(if (thinking) 1f else 0f, tween(700), label = "avatar-think")
    val talk by animateFloatAsState(if (speaking) 1f else 0f, tween(500), label = "avatar-talk")

    LaunchedEffect(still) {
        if (still) return@LaunchedEffect
        val start = withFrameNanos { it }
        var last = 0f
        while (true) {
            withFrameNanos { now ->
                val seconds = (now - start) / 1_000_000_000f
                // 30 fotogrammi al secondo bastano e risparmiano batteria.
                if (seconds - last >= 1f / 30f) {
                    last = seconds
                    time = seconds
                }
            }
        }
    }

    val description = when {
        speaking -> "JARVIS sta parlando"
        thinking -> "JARVIS sta elaborando"
        else -> "Avatar di JARVIS"
    }
    Canvas(
        modifier
            .pointerInput(Unit) { detectTapGestures { tapped = time } }
            .semantics { contentDescription = description },
    ) {
        drawAvatar(time = if (still) 0f else time, tapped = tapped, think = think, talk = talk, still = still)
    }
}

private fun DrawScope.drawAvatar(time: Float, tapped: Float, think: Float, talk: Float, still: Boolean) {
    val s = min(size.width, size.height) / 276f
    val c = Offset(size.width / 2f, size.height / 2f)
    val energy = 0.28f + 0.45f * think + 0.30f * talk
    fun at(x: Float, y: Float) = Offset(c.x + x * s, c.y + y * s)
    fun cyan(alpha: Float) = Cyan.copy(alpha = alpha.coerceIn(0f, 1f))
    val unit = AvatarMath::unit

    // Fondo scuro: l'avatar resta leggibile anche con il tema chiaro.
    drawCircle(
        Brush.radialGradient(listOf(Color(0xFF0E2A3F), Color(0xFF07131F)), center = c, radius = 138f * s),
        radius = 138f * s,
        center = c,
    )
    val breath = 0.5f + 0.5f * sin(time * 1.4f)
    drawCircle(
        Brush.radialGradient(
            listOf(cyan(0.10f + 0.18f * energy + 0.05f * breath), Color.Transparent),
            center = c,
            radius = 140f * s,
        ),
        radius = 140f * s,
        center = c,
    )

    // Onde dal centro e onda al tocco.
    if (!still) {
        for (k in 0 until 3) {
            val phase = unit(time * (0.18f + 0.5f * energy) + k / 3f)
            drawCircle(
                cyan((1f - phase) * (0.05f + 0.20f * energy)),
                radius = (46f + 86f * phase) * s,
                center = c,
                style = Stroke(width = (1.2f + talk) * s),
            )
        }
        val age = time - tapped
        if (age in 0f..1.4f) {
            val p = age / 1.4f
            val grow = AvatarMath.smooth(p)
            drawCircle(
                Color.White.copy(alpha = (1f - p) * 0.8f),
                radius = (30f + 112f * grow) * s,
                center = c,
                style = Stroke(width = (3f * (1f - p) + 0.4f) * s),
            )
            drawCircle(
                cyan((1f - p) * 0.9f),
                radius = (20f + 100f * grow) * s,
                center = c,
                style = Stroke(width = (2f * (1f - p) + 0.4f) * s),
            )
        }
    }

    // Anelli.
    val spin = 1f + 2.2f * energy
    drawCircle(
        cyan(0.22f + 0.2f * energy),
        radius = 128f * s,
        center = c,
        style = Stroke(width = 1f * s, pathEffect = PathEffect.dashPathEffect(floatArrayOf(2f * s, 6f * s), -time * 14f * spin * s)),
    )
    fun ring(radius: Float, start: Float, sweep: Float, color: Color, width: Float) {
        drawArc(
            color,
            startAngle = start,
            sweepAngle = sweep,
            useCenter = false,
            topLeft = Offset(c.x - radius * s, c.y - radius * s),
            size = Size(radius * 2f * s, radius * 2f * s),
            style = Stroke(width = width * s, cap = StrokeCap.Round),
        )
    }
    for (k in 0 until 3) ring(120f, k * 120f + time * 26f * spin, 46f, cyan(0.5f + 0.4f * energy), 2.2f)
    for (k in 0 until 2) ring(111f, k * 180f + 30f - time * 18f * spin, 70f, Indigo.copy(alpha = 0.55f + 0.3f * energy), 2.6f)
    val sweepAngle = unit(time * 0.12f * spin) * 360f
    val ticks = 72
    for (i in 0 until ticks) {
        val angle = i.toFloat() / ticks * 360f
        val glow = AvatarMath.tickGlow(angle, sweepAngle)
        val radians = (angle * PI / 180.0).toFloat()
        val dx = cos(radians)
        val dy = sin(radians)
        drawLine(
            cyan(0.18f + 0.75f * glow),
            start = at(dx * (101f - glow * 3f), dy * (101f - glow * 3f)),
            end = at(dx * (105f + glow * 4f), dy * (105f + glow * 4f)),
            strokeWidth = (if (i % 6 == 0) 1.6f else 0.9f) * s,
        )
    }
    if (think > 0.02f) {
        val start = time * 240f
        ring(132f, start, 64f, Color.White.copy(alpha = 0.85f * think), 3f)
        ring(132f, start + 180f, 34f, cyan(0.9f * think), 2f)
    }

    // Scintille in orbita.
    for (i in 0 until 42) {
        val seed = i.toFloat()
        val radius = 88f + 44f * AvatarMath.hash(seed + 1f)
        val speed = (0.04f + 0.10f * AvatarMath.hash(seed + 2f)) * (1f + 2f * energy) * (if (i % 2 == 0) 1f else -1f)
        val angle = (AvatarMath.hash(seed + 3f) + time * speed) * 2f * PI.toFloat()
        val twinkle = 0.25f + 0.75f * abs(sin(time * (0.8f + AvatarMath.hash(seed + 4f) * 1.6f) + seed))
        val dot = 0.8f + 1.7f * AvatarMath.hash(seed + 5f)
        val spot = at(cos(angle) * radius, sin(angle) * radius * 0.96f)
        drawCircle(cyan(0.07f * twinkle), radius = dot * 2.4f * s, center = spot)
        drawCircle(Color.White.copy(alpha = 0.35f + 0.5f * twinkle), radius = max(dot * s, 0.8f), center = spot)
    }

    // Testa: oscilla, si inclina, respira; ogni ~9 s un breve glitch.
    val sway = if (still) 0f else sin(time * 0.7f) * 2.2f
    val lift = if (still) 0f else sin(time * 1.8f) * 3.2f - talk * abs(sin(time * 9f)) * 1.2f
    val tilt = if (still) 0f else sin(time * 0.5f) * 1.6f + think * sin(time * 1.3f) * 1.4f

    fun polygon(points: List<Pair<Float, Float>>, fill: Color, edge: Float, width: Float) {
        val path = Path().apply {
            points.forEachIndexed { index, (x, y) ->
                val p = at(x, y)
                if (index == 0) moveTo(p.x, p.y) else lineTo(p.x, p.y)
            }
            close()
        }
        drawPath(path, fill)
        drawPath(path, cyan(edge), style = Stroke(width = width * s))
    }

    withTransform({
        translate(left = sway * s, top = lift * s)
        rotate(tilt, pivot = c)
    }) {
        if (!still && unit(time / 9.3f) < 0.0125f) {
            val shift = (AvatarMath.hash(floor(time * 40f)) - 0.5f) * 14f
            for ((points, color) in listOf(head to Color(0xFFFF50AA), shoulders to Cyan)) {
                val ghost = Path().apply {
                    points.forEachIndexed { index, (x, y) ->
                        val p = at(x + shift, y)
                        if (index == 0) moveTo(p.x, p.y) else lineTo(p.x, p.y)
                    }
                    close()
                }
                drawPath(ghost, color.copy(alpha = 0.7f), style = Stroke(width = 1.3f * s))
            }
        }
        polygon(shoulders, lerpColor(Alloy, Deep, 0.55f), 0.5f, 1f)
        polygon(neck, lerpColor(Alloy, Deep, 0.6f), 0.5f, 1f)
        polygon(head, lerpColor(Alloy, Deep, 0.35f), 0.55f, 1.1f)
        polygon(eyeBand, Visor, 0.4f, 1f)
        for (sign in listOf(-1f, 1f)) {
            val brow = Path().apply {
                moveTo(at(sign * 20f, -86f).x, at(sign * 20f, -86f).y)
                lineTo(at(sign * 36f, -66f).x, at(sign * 36f, -66f).y)
                lineTo(at(sign * 45f, -58f).x, at(sign * 45f, -58f).y)
            }
            drawPath(brow, cyan(0.4f), style = Stroke(width = 1f * s))
            drawLine(cyan(0.3f), at(sign * 44f, -8f), at(sign * 30f, 28f), strokeWidth = 1f * s)
        }

        // Fascio di scansione ritagliato sulla testa.
        val sweepY = -92f + 140f * unit(time * (0.16f + 0.35f * energy))
        for ((offset, strength) in listOf(-10f to 0.25f, -5f to 0.55f, 0f to 1f, 5f to 0.55f, 10f to 0.25f)) {
            val y = sweepY + offset
            val half = AvatarMath.headHalfWidth(y) * 0.97f
            if (half > 1f) {
                val core = offset == 0f
                drawLine(
                    cyan(if (core) 0.35f + 0.45f * energy else (0.04f + 0.12f * energy) * strength),
                    start = at(-half, y),
                    end = at(half, y),
                    strokeWidth = (if (core) 1.2f else 4f) * s,
                )
            }
        }

        // Occhi: sguardo a scatti; quando elabora cerca avanti e indietro.
        val interval = 1.7f
        val segment = floor(time / interval)
        val ease = AvatarMath.smooth((time / interval - segment) * interval / 0.18f)
        var gazeX = (AvatarMath.hash(segment - 1f) - 0.5f) * 7f * (1f - ease) + (AvatarMath.hash(segment) - 0.5f) * 7f * ease
        var gazeY = (AvatarMath.hash(segment + 40f) - 0.5f) * 2.4f
        if (still) {
            gazeX = 0f
            gazeY = 0f
        }
        gazeX = gazeX * (1f - think) + 7f * sin(time * 3.1f) * think
        val open = 7f - think * 1.4f
        val lid = if (still) 1f else AvatarMath.blink(time)
        val eyeHeight = max(1.6f, open * lid)
        val pulse = 0.82f + 0.18f * sin(time * (4f + 6f * think)) + talk * 0.05f
        for (x in listOf(-27f, 13f)) {
            val top = -36f + gazeY + (open - eyeHeight) / 2f
            val corner = CornerRadius(2f * s)
            val glowAt = at(x + gazeX - 4f, top - 3f)
            drawRoundRect(cyan(0.12f * pulse), glowAt, Size(22f * s, (eyeHeight + 6f) * s), CornerRadius(6f * s))
            val glowInner = at(x + gazeX - 2f, top - 1.5f)
            drawRoundRect(cyan(0.25f * pulse), glowInner, Size(18f * s, (eyeHeight + 3f) * s), CornerRadius(4f * s))
            drawRoundRect(cyan(pulse), at(x + gazeX, top), Size(14f * s, eyeHeight * s), corner)
            drawRoundRect(
                Color.White.copy(alpha = 0.85f * lid),
                at(x + gazeX + 3f, top + eyeHeight * 0.28f),
                Size(8f * s, max(0.6f, eyeHeight * 0.44f) * s),
                CornerRadius(1f * s),
            )
        }

        // Bocca a barre.
        val bars = 7
        for (i in 0 until bars) {
            val index = i.toFloat()
            val envelope = 1f - abs(index - (bars - 1) / 2f) / bars * 0.8f
            val height = AvatarMath.mouthHeight(time, index, think, talk) * envelope + 0.6f
            drawRoundRect(
                cyan(0.40f + 0.5f * max(think, talk)),
                at(-13f + index * 4.2f, 22f - height / 2f),
                Size(2.8f * s, height * s),
                CornerRadius(1.4f * s),
            )
        }

        // Nucleo sul petto.
        val core = at(0f, 92f)
        val pulseCore = 0.5f + 0.5f * sin(time * (2f + 4f * energy))
        drawCircle(cyan(0.06f + 0.08f * pulseCore), radius = 24f * s, center = core)
        drawCircle(cyan(0.10f + 0.14f * pulseCore), radius = 15f * s, center = core)
        val turn = time * 72f * (1f + energy)
        for (k in 0 until 6) {
            drawArc(
                cyan(0.9f),
                startAngle = turn + k * 60f,
                sweepAngle = 34f,
                useCenter = false,
                topLeft = Offset(core.x - 10f * s, core.y - 10f * s),
                size = Size(20f * s, 20f * s),
                style = Stroke(width = 1.4f * s, cap = StrokeCap.Round),
            )
        }
        val spinCore = time * (1.4f + 2.6f * energy)
        val triangle = Path().apply {
            for (k in 0 until 3) {
                val angle = spinCore + k * 2f * PI.toFloat() / 3f
                val x = core.x + cos(angle) * 6.5f * s
                val y = core.y + sin(angle) * 6.5f * s
                if (k == 0) moveTo(x, y) else lineTo(x, y)
            }
            close()
        }
        drawPath(triangle, Color.White.copy(alpha = 0.9f), style = Stroke(width = 1.1f * s))
        drawCircle(Color.White, radius = (2.6f + pulseCore * 1.2f) * s, center = core)
    }
}

private fun lerpColor(a: Color, b: Color, amount: Float): Color = androidx.compose.ui.graphics.lerp(a, b, amount)
