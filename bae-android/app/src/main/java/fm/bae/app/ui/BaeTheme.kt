package fm.bae.app.ui

import android.app.Activity
import android.graphics.drawable.ColorDrawable
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.LocalTonalElevationEnabled
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalInspectionMode
import androidx.compose.ui.platform.LocalView
import androidx.core.view.WindowCompat
import fm.bae.app.R
import fm.bae.app.ui.appearance.AppearanceMode
import fm.bae.app.ui.appearance.AppearancePalette
import fm.bae.app.ui.appearance.AppearancePreferences
import fm.bae.app.ui.appearance.AppearanceStore
import fm.bae.app.ui.appearance.LocalAppearanceStore
import kotlinx.coroutines.Dispatchers
import java.io.File

val LocalPrimaryFill = staticCompositionLocalOf<Color> { error("BaeTheme provides primary button colors") }
val LocalAppearancePalette = staticCompositionLocalOf<AppearancePalette> { error("BaeTheme provides the palette") }

@Composable
private fun rememberAppearanceStore(): AppearanceStore {
    val context = LocalContext.current
    val preview = LocalInspectionMode.current
    return remember(context, preview) {
        if (preview) {
            AppearanceStore(AppearancePreferences()) {}
        } else {
            AppearanceStore.fromFile(File(context.filesDir, "appearance.json"), Dispatchers.IO)
        }
    }
}

@Composable
fun BaeTheme(
    appearance: AppearanceStore = rememberAppearanceStore(),
    content: @Composable () -> Unit,
) {
    val context = LocalContext.current
    val preferences by appearance.preferences.collectAsState()
    val palette = remember(context) { AppearancePalette(context.resources.openRawResource(R.raw.appearance_palette)) }
    val isDark =
        when (preferences.mode) {
            AppearanceMode.SYSTEM -> isSystemInDarkTheme()
            AppearanceMode.LIGHT -> false
            AppearanceMode.DARK -> true
        }
    val colorScheme = remember(preferences, isDark, palette) { palette.colors(preferences, isDark) }
    val view = LocalView.current
    if (!view.isInEditMode) {
        val activity = view.context as Activity
        SideEffect {
            val background = colorScheme.background.toArgb()
            activity.window.setBackgroundDrawable(ColorDrawable(background))
            // Android 15 draws enforced edge-to-edge bars over the window;
            // earlier releases still use these explicit bar colors.
            activity.window.statusBarColor = background
            activity.window.navigationBarColor = background
            val insetsController = WindowCompat.getInsetsController(activity.window, view)
            insetsController.isAppearanceLightStatusBars = !isDark
            insetsController.isAppearanceLightNavigationBars = !isDark
        }
    }
    CompositionLocalProvider(
        LocalAppearanceStore provides appearance,
        LocalAppearancePalette provides palette,
        LocalPrimaryFill provides palette.accentFill(preferences.accent),
        LocalTonalElevationEnabled provides false,
    ) {
        MaterialTheme(colorScheme = colorScheme, content = content)
    }
}
