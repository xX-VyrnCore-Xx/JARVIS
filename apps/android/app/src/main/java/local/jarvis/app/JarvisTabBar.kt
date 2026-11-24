package local.jarvis.app

import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * Barra a schede di JARVIS (ADR-0129): icone disegnate (nessun glifo di font che possa mancare), pillola che
 * si allarga con una molla sotto la scheda attiva, icona che «salta» e pallino con il numero di notifiche.
 * [badges] associa l'indice di una scheda al numero da mostrare.
 */
@Composable
fun JarvisTabBar(
    labels: List<String>,
    selected: Int,
    enabled: Boolean,
    badges: Map<Int, Int>,
    onSelect: (Int) -> Unit,
) {
    val scheme = MaterialTheme.colorScheme
    Row(
        Modifier
            .fillMaxWidth()
            .background(scheme.surface)
            .windowInsetsPadding(WindowInsets.navigationBars)
            .padding(horizontal = 8.dp, vertical = 6.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        labels.forEachIndexed { index, label ->
            val active = index == selected
            val progress by animateFloatAsState(
                if (active) 1f else 0f,
                spring(dampingRatio = Spring.DampingRatioMediumBouncy, stiffness = Spring.StiffnessMedium),
                label = "tab-$index",
            )
            val tint = lerp(scheme.onSurfaceVariant, scheme.primary, progress.coerceIn(0f, 1f))
            val pill = scheme.primary
            Column(
                Modifier
                    .weight(1f)
                    .clip(RoundedCornerShape(16.dp))
                    .clickable(enabled = enabled, role = Role.Tab) { onSelect(index) }
                    .semantics { this.selected = active }
                    .padding(vertical = 4.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Box(
                    Modifier
                        .height(32.dp)
                        .width(60.dp)
                        .drawBehind {
                            val fraction = progress.coerceIn(0f, 1f)
                            drawRoundRect(
                                pill.copy(alpha = 0.16f * fraction),
                                topLeft = Offset(size.width * (1f - fraction) / 2f, 0f),
                                size = Size(size.width * fraction, size.height),
                                cornerRadius = CornerRadius(size.height / 2f),
                            )
                        },
                    contentAlignment = Alignment.Center,
                ) {
                    Canvas(
                        Modifier.size(24.dp).graphicsLayer {
                            val grow = 1f + 0.14f * progress
                            scaleX = grow
                            scaleY = grow
                        },
                    ) { drawTabIcon(index, tint) }
                    val count = badges[index] ?: 0
                    if (count > 0) {
                        Box(
                            Modifier
                                .align(Alignment.TopEnd)
                                .offset(x = (-4).dp, y = 0.dp)
                                .defaultMinSize(minWidth = 16.dp, minHeight = 16.dp)
                                .background(scheme.error, CircleShape),
                            contentAlignment = Alignment.Center,
                        ) {
                            Text(
                                if (count > 9) "9+" else count.toString(),
                                color = scheme.onError,
                                fontSize = 10.sp,
                                fontWeight = FontWeight.Bold,
                                modifier = Modifier.padding(horizontal = 3.dp),
                            )
                        }
                    }
                }
                Text(
                    label,
                    style = MaterialTheme.typography.labelSmall,
                    fontWeight = if (active) FontWeight.SemiBold else FontWeight.Normal,
                    color = tint,
                    maxLines = 1,
                )
            }
        }
    }
}

/** Icone delle cinque schede: Plancia, Chat, Attività, Notifiche, Core (griglia di 24 unità). */
private fun DrawScope.drawTabIcon(index: Int, color: Color) {
    val u = size.minDimension / 24f
    fun p(x: Float, y: Float) = Offset(x * u, y * u)
    val style = Stroke(width = 1.8f * u, cap = StrokeCap.Round, join = StrokeJoin.Round)
    when (index) {
        0 -> {
            val roof = Path().apply {
                moveTo(3f * u, 11f * u); lineTo(12f * u, 3.5f * u); lineTo(21f * u, 11f * u)
            }
            val body = Path().apply {
                moveTo(5.5f * u, 9.5f * u); lineTo(5.5f * u, 20f * u); lineTo(18.5f * u, 20f * u); lineTo(18.5f * u, 9.5f * u)
            }
            val door = Path().apply {
                moveTo(10f * u, 20f * u); lineTo(10f * u, 14f * u); lineTo(14f * u, 14f * u); lineTo(14f * u, 20f * u)
            }
            drawPath(roof, color, style = style)
            drawPath(body, color, style = style)
            drawPath(door, color, style = style)
        }
        1 -> {
            drawRoundRect(color, p(3f, 4f), Size(18f * u, 12f * u), CornerRadius(4f * u), style = style)
            val tail = Path().apply {
                moveTo(8f * u, 16f * u); lineTo(7f * u, 20.5f * u); lineTo(13f * u, 16f * u)
            }
            drawPath(tail, color, style = style)
        }
        2 -> {
            for (row in 0 until 3) {
                val y = 6f + row * 6f
                drawCircle(color, radius = 1.3f * u, center = p(5f, y))
                drawLine(color, p(9f, y), p(20f, y), strokeWidth = 1.8f * u, cap = StrokeCap.Round)
            }
        }
        3 -> {
            drawArc(color, 180f, 180f, false, p(6f, 4f), Size(12f * u, 12f * u), style = style)
            drawLine(color, p(6f, 10f), p(6f, 17f), strokeWidth = 1.8f * u, cap = StrokeCap.Round)
            drawLine(color, p(18f, 10f), p(18f, 17f), strokeWidth = 1.8f * u, cap = StrokeCap.Round)
            drawLine(color, p(4f, 17f), p(20f, 17f), strokeWidth = 1.8f * u, cap = StrokeCap.Round)
            drawArc(color, 0f, 180f, false, p(10f, 18.5f), Size(4f * u, 3f * u), style = style)
        }
        else -> {
            drawCircle(color, radius = 9f * u, center = p(12f, 12f), style = style)
            drawCircle(color, radius = 4.5f * u, center = p(12f, 12f), style = style)
            drawCircle(color, radius = 1.4f * u, center = p(12f, 12f))
        }
    }
}
