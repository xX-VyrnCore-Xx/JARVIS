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
    primary = Color(0xFF78E0E7),
    onPrimary = Color(0xFF00232B),
    primaryContainer = Color(0xFF163E4B),
    onPrimaryContainer = Color(0xFFD4F7FF),
    secondary = Color(0xFFB7C9D0),
    background = Color(0xFF09111A),
    surface = Color(0xFF0F1B27),
    surfaceVariant = Color(0xFF162635),
    onSurface = Color(0xFFE4F0F3),
    onSurfaceVariant = Color(0xFFB0C0C6),
    outline = Color(0xFF53666F),
)

private val JarvisLight = lightColorScheme(
    primary = Color(0xFF006779),
    onPrimary = Color.White,
    primaryContainer = Color(0xFFC0F0FA),
    onPrimaryContainer = Color(0xFF001F26),
    secondary = Color(0xFF49616A),
    background = Color(0xFFF4F8FA),
    surface = Color(0xFFFFFFFF),
    surfaceVariant = Color(0xFFE5EEF1),
    onSurface = Color(0xFF131D22),
    onSurfaceVariant = Color(0xFF3F5057),
    outline = Color(0xFF73868D),
)

private val JarvisShapes = Shapes(
    small = RoundedCornerShape(10.dp),
    medium = RoundedCornerShape(14.dp),
    large = RoundedCornerShape(20.dp),
)

@Composable
fun JarvisTheme(dark: Boolean, content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = if (dark) JarvisDark else JarvisLight,
        shapes = JarvisShapes,
        content = content,
    )
}
