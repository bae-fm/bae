package fm.bae.app.ui

import android.app.Activity
import android.os.Build
import android.view.View
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.SideEffect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalView

private val baeLightColorScheme = lightColorScheme(
    primary = Color(0xFFB8543A),
    onPrimary = Color(0xFFFFFFFF),
    primaryContainer = Color(0xFFFFDAD0),
    onPrimaryContainer = Color(0xFF3B0900),
    secondary = Color(0xFF386A6A),
    onSecondary = Color(0xFFFFFFFF),
    secondaryContainer = Color(0xFFBCECEB),
    onSecondaryContainer = Color(0xFF002020),
    tertiary = Color(0xFF4E5F88),
    onTertiary = Color(0xFFFFFFFF),
    tertiaryContainer = Color(0xFFD9E2FF),
    onTertiaryContainer = Color(0xFF071A42),
    background = Color(0xFFFCFCFF),
    onBackground = Color(0xFF1A1C1E),
    surface = Color(0xFFFCFCFF),
    onSurface = Color(0xFF1A1C1E),
    surfaceVariant = Color(0xFFDDE5E8),
    onSurfaceVariant = Color(0xFF41484C),
    error = Color(0xFFBA1A1A),
    onError = Color(0xFFFFFFFF),
    errorContainer = Color(0xFFFFDAD6),
    onErrorContainer = Color(0xFF410002),
)

private val baeDarkColorScheme = darkColorScheme(
    primary = Color(0xFFFFB5A0),
    onPrimary = Color(0xFF5F1708),
    primaryContainer = Color(0xFF833020),
    onPrimaryContainer = Color(0xFFFFDAD0),
    secondary = Color(0xFFA0CFCE),
    onSecondary = Color(0xFF003737),
    secondaryContainer = Color(0xFF1E4E4E),
    onSecondaryContainer = Color(0xFFBCECEB),
    tertiary = Color(0xFFB7C7F7),
    onTertiary = Color(0xFF1F2F57),
    tertiaryContainer = Color(0xFF36466F),
    onTertiaryContainer = Color(0xFFD9E2FF),
    background = Color(0xFF0F1117),
    onBackground = Color(0xFFE4E8ED),
    surface = Color(0xFF171922),
    onSurface = Color(0xFFE4E8ED),
    surfaceVariant = Color(0xFF40484C),
    onSurfaceVariant = Color(0xFFC0C8CC),
    error = Color(0xFFFFB4AB),
    onError = Color(0xFF690005),
    errorContainer = Color(0xFF93000A),
    onErrorContainer = Color(0xFFFFDAD6),
)

@Suppress("DEPRECATION")
@Composable
fun BaeTheme(content: @Composable () -> Unit) {
    val isDark = isSystemInDarkTheme()
    val colorScheme = if (isDark) baeDarkColorScheme else baeLightColorScheme
    val view = LocalView.current
    if (!view.isInEditMode) {
        val activity = view.context as Activity
        val lightNavigationBarFlag = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O_MR1) {
            View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR
        } else {
            0
        }
        SideEffect {
            val window = activity.window
            window.statusBarColor = colorScheme.background.toArgb()
            window.navigationBarColor = colorScheme.background.toArgb()

            val lightBarFlags = View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR or lightNavigationBarFlag
            window.decorView.systemUiVisibility = if (isDark) {
                window.decorView.systemUiVisibility and lightBarFlags.inv()
            } else {
                window.decorView.systemUiVisibility or lightBarFlags
            }
        }
    }
    MaterialTheme(
        colorScheme = colorScheme,
        content = content,
    )
}
