package fm.bae.app.ui.appearance

import androidx.compose.material3.ColorScheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.ui.graphics.Color

/** The Material colour scheme for the chosen appearance, from the shared theme. */
fun appearanceColorScheme(
    preferences: AppearancePreferences,
    dark: Boolean,
): ColorScheme {
    val surfaces = preferences.tone.surfaces(dark)
    val semantics = semanticColors(dark)
    val inverseSurfaces = preferences.tone.surfaces(!dark)
    val inverseSemantics = semanticColors(!dark)
    val accentColors = preferences.accent.colors
    val accent = if (dark) accentColors.dark else accentColors.light
    val onFilled = if (dark) surfaces.background else Color.White
    val base = if (dark) darkColorScheme() else lightColorScheme()
    return base.copy(
        primary = accent,
        onPrimary = onFilled,
        primaryContainer = surfaces.elevated,
        onPrimaryContainer = accent,
        secondary = semantics.textSecondary,
        onSecondary = onFilled,
        secondaryContainer = surfaces.tile,
        onSecondaryContainer = semantics.textPrimary,
        tertiary = semantics.textSecondary,
        onTertiary = onFilled,
        tertiaryContainer = surfaces.tile,
        onTertiaryContainer = semantics.textPrimary,
        background = surfaces.background,
        onBackground = semantics.textPrimary,
        surface = surfaces.surface,
        onSurface = semantics.textPrimary,
        surfaceVariant = surfaces.elevated,
        onSurfaceVariant = semantics.textSecondary,
        surfaceDim = surfaces.background,
        surfaceBright = surfaces.elevated,
        surfaceContainerLowest = surfaces.background,
        surfaceContainerLow = surfaces.surface,
        surfaceContainer = surfaces.surface,
        surfaceContainerHigh = surfaces.elevated,
        surfaceContainerHighest = surfaces.tile,
        surfaceTint = accent,
        outline = semantics.textSecondary,
        outlineVariant = semantics.hairline,
        inverseSurface = inverseSurfaces.surface,
        inverseOnSurface = inverseSemantics.textPrimary,
        inversePrimary = if (dark) accentColors.light else accentColors.dark,
        scrim = Color.Black,
        errorContainer = surfaces.elevated,
        onErrorContainer = semantics.danger,
        error = semantics.danger,
        onError = onFilled,
    )
}
