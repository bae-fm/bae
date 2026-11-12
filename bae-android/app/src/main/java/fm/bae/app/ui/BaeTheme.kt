package fm.bae.app.ui

import android.graphics.drawable.ColorDrawable
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.LocalTonalElevationEnabled
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.glance.appwidget.updateAll
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
import fm.bae.app.widget.NowPlayingWidget

val LocalPrimaryFill = staticCompositionLocalOf<Color> { error("BaeTheme provides primary button colors") }
private val LocalSemanticColors = staticCompositionLocalOf<SemanticColors> { error("BaeTheme provides colours") }
private val LocalToneSurfaces = staticCompositionLocalOf<ToneSurfaces> { error("BaeTheme provides surfaces") }
private val LocalIsDark = staticCompositionLocalOf<Boolean> { error("BaeTheme provides the appearance") }

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

    /** Whether the chosen appearance is dark. */
    val isDark: Boolean
        @Composable
        @ReadOnlyComposable
        get() = LocalIsDark.current
}

/**
 * The theme for the chosen [appearance]. The app opens it once, at the activity's root, with the store read from
 * the person's appearance file; every screen below inherits it.
 */
@Composable
fun BaeTheme(
    appearance: AppearanceStore,
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
    CompositionLocalProvider(
        LocalAppearanceStore provides appearance,
        LocalPrimaryFill provides preferences.accent.colors.selection,
        LocalSemanticColors provides semanticColors(isDark),
        LocalIsDark provides isDark,
        LocalToneSurfaces provides preferences.tone.surfaces(isDark),
        LocalTonalElevationEnabled provides false,
    ) {
        MaterialTheme(colorScheme = colorScheme, content = content)
    }
}

/**
 * What the activity does for the chosen appearance, placed once under the root [BaeTheme]: the window draws the
 * theme's background and the bars' icons follow its mode, and the home-screen widget redraws with the same look.
 */
@Composable
fun ActivityAppearance() {
    val context = LocalContext.current
    val activity = LocalView.current.context as ComponentActivity
    val preferences by LocalAppearanceStore.current.preferences.collectAsState()
    val background = MaterialTheme.colorScheme.background
    val isDark = BaeTheme.isDark
    LaunchedEffect(preferences) { NowPlayingWidget().updateAll(context.applicationContext) }
    SideEffect {
        activity.window.setBackgroundDrawable(ColorDrawable(background.toArgb()))
        // The app draws edge to edge on every release, as Android 15 and
        // later require: the bars are clear over the window's background,
        // with icons dark or light for the chosen appearance.
        val bars = SystemBarStyle.auto(Color.Transparent.toArgb(), Color.Transparent.toArgb()) { isDark }
        activity.enableEdgeToEdge(statusBarStyle = bars, navigationBarStyle = bars)
    }
}

/** A shot's theme: the default appearance, nothing read from the person's files. */
@Composable
fun PreviewTheme(content: @Composable () -> Unit) {
    val appearance = remember { AppearanceStore(AppearancePreferences()) {} }
    BaeTheme(appearance, content)
}
