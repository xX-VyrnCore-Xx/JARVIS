package local.jarvis.app

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp

private val JarvisDark = darkColorScheme(
    primary = Color(0xFF9CCAFF),
    onPrimary = Color(0xFF00232B),
    primaryContainer = Color(0xFF233F65),
    onPrimaryContainer = Color(0xFFD4F7FF),
    secondary = Color(0xFFB7C9D0),
    background = Color(0xFF111318),
    surface = Color(0xFF191D25),
    surfaceVariant = Color(0xFF222833),
    onSurface = Color(0xFFE4F0F3),
    onSurfaceVariant = Color(0xFFB0C0C6),
    outline = Color(0xFF53666F),
)

private val JarvisLight = lightColorScheme(
    primary = Color(0xFF145DB2),
    onPrimary = Color.White,
    primaryContainer = Color(0xFFD9E9FF),
    onPrimaryContainer = Color(0xFF092649),
    secondary = Color(0xFF49616A),
    background = Color(0xFFF3F4F8),
    surface = Color(0xFFFFFFFF),
    surfaceVariant = Color(0xFFE9EDF4),
    onSurface = Color(0xFF131D22),
    onSurfaceVariant = Color(0xFF3F5057),
    outline = Color(0xFF73868D),
)

private val JarvisShapes = Shapes(
    small = RoundedCornerShape(10.dp),
    medium = RoundedCornerShape(18.dp),
    large = RoundedCornerShape(26.dp),
)

@Composable
fun JarvisTheme(dark: Boolean, content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = if (dark) JarvisDark else JarvisLight,
        shapes = JarvisShapes,
        content = content,
    )
}
