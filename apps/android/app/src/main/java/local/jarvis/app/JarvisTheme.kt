package local.jarvis.app

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBarItemColors
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.material3.Shapes
import androidx.compose.material3.SwitchColors
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

// Palette di sistema di iOS (ADR-0101): sfondo raggruppato, schede bianche o grigio scuro, blu di sistema,
// separatori sottili. Il verde dei controlli attivi è il «systemGreen» di iOS.
private val SystemBlueLight = Color(0xFF007AFF)
private val SystemBlueDark = Color(0xFF0A84FF)
private val SystemGreen = Color(0xFF34C759)

private val JarvisDark = darkColorScheme(
    primary = SystemBlueDark,
    onPrimary = Color.White,
    primaryContainer = Color(0xFF0A3A73),
    onPrimaryContainer = Color(0xFFD6E8FF),
    secondary = Color(0xFF8E8E93),
    background = Color(0xFF000000),
    onBackground = Color.White,
    surface = Color(0xFF1C1C1E),
    onSurface = Color.White,
    surfaceVariant = Color(0xFF1C1C1E),
    onSurfaceVariant = Color(0xFF8E8E93),
    outline = Color(0xFF48484A),
    outlineVariant = Color(0xFF38383A),
    error = Color(0xFFFF453A),
)

private val JarvisLight = lightColorScheme(
    primary = SystemBlueLight,
    onPrimary = Color.White,
    primaryContainer = Color(0xFFD6E8FF),
    onPrimaryContainer = Color(0xFF002E66),
    secondary = Color(0xFF8E8E93),
    background = Color(0xFFF2F2F7),
    onBackground = Color.Black,
    surface = Color.White,
    onSurface = Color.Black,
    surfaceVariant = Color.White,
    onSurfaceVariant = Color(0xFF6C6C70),
    outline = Color(0xFFC6C6C8),
    outlineVariant = Color(0xFFD1D1D6),
    error = Color(0xFFFF3B30),
)

// Angoli generosi come le schede raggruppate di iOS.
private val JarvisShapes = Shapes(
    extraSmall = RoundedCornerShape(8.dp),
    small = RoundedCornerShape(10.dp),
    medium = RoundedCornerShape(14.dp),
    large = RoundedCornerShape(20.dp),
    extraLarge = RoundedCornerShape(28.dp),
)

// Gerarchia tipografica di iOS (Large Title 34, Title 28/22/20, Headline 17, Body 17, Footnote 13, Caption 12/10).
private val System = FontFamily.SansSerif
private val JarvisTypography = Typography(
    displayLarge = TextStyle(fontFamily = System, fontWeight = FontWeight.Bold, fontSize = 40.sp, lineHeight = 46.sp),
    headlineLarge = TextStyle(fontFamily = System, fontWeight = FontWeight.Bold, fontSize = 34.sp, lineHeight = 41.sp),
    headlineMedium = TextStyle(fontFamily = System, fontWeight = FontWeight.Bold, fontSize = 28.sp, lineHeight = 34.sp),
    headlineSmall = TextStyle(fontFamily = System, fontWeight = FontWeight.SemiBold, fontSize = 22.sp, lineHeight = 28.sp),
    titleLarge = TextStyle(fontFamily = System, fontWeight = FontWeight.SemiBold, fontSize = 22.sp, lineHeight = 28.sp),
    titleMedium = TextStyle(fontFamily = System, fontWeight = FontWeight.SemiBold, fontSize = 17.sp, lineHeight = 22.sp),
    titleSmall = TextStyle(fontFamily = System, fontWeight = FontWeight.SemiBold, fontSize = 15.sp, lineHeight = 20.sp),
    bodyLarge = TextStyle(fontFamily = System, fontWeight = FontWeight.Normal, fontSize = 17.sp, lineHeight = 22.sp),
    bodyMedium = TextStyle(fontFamily = System, fontWeight = FontWeight.Normal, fontSize = 15.sp, lineHeight = 20.sp),
    bodySmall = TextStyle(fontFamily = System, fontWeight = FontWeight.Normal, fontSize = 13.sp, lineHeight = 18.sp),
    labelLarge = TextStyle(fontFamily = System, fontWeight = FontWeight.SemiBold, fontSize = 17.sp, lineHeight = 22.sp),
    labelMedium = TextStyle(fontFamily = System, fontWeight = FontWeight.Medium, fontSize = 12.sp, lineHeight = 16.sp),
    labelSmall = TextStyle(fontFamily = System, fontWeight = FontWeight.Medium, fontSize = 10.sp, lineHeight = 12.sp),
)

@Composable
fun JarvisTheme(dark: Boolean, content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = if (dark) JarvisDark else JarvisLight,
        shapes = JarvisShapes,
        typography = JarvisTypography,
        content = content,
    )
}

/** Barra a schede in stile iOS: nessun indicatore a pillola, solo icona ed etichetta in blu o grigio. */
@Composable
fun iosTabColors(): NavigationBarItemColors {
    val scheme = MaterialTheme.colorScheme
    return NavigationBarItemDefaults.colors(
        selectedIconColor = scheme.primary,
        selectedTextColor = scheme.primary,
        unselectedIconColor = scheme.onSurfaceVariant,
        unselectedTextColor = scheme.onSurfaceVariant,
        indicatorColor = Color.Transparent,
    )
}

/** Interruttore come in iOS: traccia verde da acceso, cursore sempre bianco. */
@Composable
fun iosSwitchColors(): SwitchColors = SwitchDefaults.colors(
    checkedThumbColor = Color.White,
    checkedTrackColor = SystemGreen,
    checkedBorderColor = SystemGreen,
    uncheckedThumbColor = Color.White,
    uncheckedTrackColor = MaterialTheme.colorScheme.outline,
    uncheckedBorderColor = MaterialTheme.colorScheme.outline,
)
