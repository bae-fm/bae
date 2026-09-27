package fm.bae.app.ui

import android.app.Activity
import android.graphics.drawable.ColorDrawable
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.LocalTonalElevationEnabled
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.ReadOnlyComposable
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
import fm.bae.app.ui.appearance.AppearanceMode
import fm.bae.app.ui.appearance.AppearancePreferences
import fm.bae.app.ui.appearance.AppearanceStore
import fm.bae.app.ui.appearance.LocalAppearanceStore
import fm.bae.app.ui.appearance.SemanticColors
import fm.bae.app.ui.appearance.ToneSurfaces
import fm.bae.app.ui.appearance.appearanceColorScheme
import fm.bae.app.ui.appearance.colors
import fm.bae.app.ui.appearance.semanticColors
import fm.bae.app.ui.appearance.surfaces
import kotlinx.coroutines.Dispatchers
import java.io.File

val LocalPrimaryFill = staticCompositionLocalOf<Color> { error("BaeTheme provides primary button colors") }
private val LocalSemanticColors = staticCompositionLocalOf<SemanticColors> { error("BaeTheme provides colours") }
private val LocalToneSurfaces = staticCompositionLocalOf<ToneSurfaces> { error("BaeTheme provides surfaces") }

/** The shared theme's roles for the chosen appearance. */
object BaeTheme {
    val colors: SemanticColors
        @Composable
        @ReadOnlyComposable
        get() = LocalSemanticColors.current

    val surfaces: ToneSurfaces
        @Composable
        @ReadOnlyComposable
        get() = LocalToneSurfaces.current
}

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
    val preferences by appearance.preferences.collectAsState()
    val isDark =
        when (preferences.mode) {
            AppearanceMode.SYSTEM -> isSystemInDarkTheme()
            AppearanceMode.LIGHT -> false
            AppearanceMode.DARK -> true
        }
    val colorScheme = remember(preferences, isDark) { appearanceColorScheme(preferences, isDark) }
    val view = LocalView.current
    if (!view.isInEditMode) {
        val activity = view.context as Activity
        SideEffect {
            val background = colorScheme.background.toArgb()
            activity.window.setBackgroundDrawable(ColorDrawable(background))
            // Only releases before Android 15 use these bar colors.
            activity.window.statusBarColor = background
            activity.window.navigationBarColor = background
            val insetsController = WindowCompat.getInsetsController(activity.window, view)
            insetsController.isAppearanceLightStatusBars = !isDark
            insetsController.isAppearanceLightNavigationBars = !isDark
        }
    }
    CompositionLocalProvider(
        LocalAppearanceStore provides appearance,
        LocalPrimaryFill provides preferences.accent.colors.fill,
        LocalSemanticColors provides semanticColors(isDark),
        LocalToneSurfaces provides preferences.tone.surfaces(isDark),
        LocalTonalElevationEnabled provides false,
    ) {
        MaterialTheme(colorScheme = colorScheme, content = content)
    }
}
