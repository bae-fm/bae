package fm.bae.app.ui.appearance

import androidx.compose.material3.ColorScheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.ui.graphics.Color
import org.json.JSONObject
import java.io.InputStream
import java.util.Locale

/** Reads the same palette resource bundled by BaeKit and Avalonia. */
class AppearancePalette(
    stream: InputStream,
) {
    private val json = stream.bufferedReader().use { JSONObject(it.readText()) }

    fun accentFill(accent: AccentChoice): Color = accents(accent).color("fill")

    fun colors(
        preferences: AppearancePreferences,
        dark: Boolean,
    ): ColorScheme {
        val mode = if (dark) "dark" else "light"
        val surfaces = json.getJSONObject("tones").getJSONObject(preferences.tone.key).getJSONObject(mode)
        val semantics = json.getJSONObject("semantics").getJSONObject(mode)
        val inverseMode = if (dark) "light" else "dark"
        val inverseSurfaces = json.getJSONObject("tones").getJSONObject(preferences.tone.key).getJSONObject(inverseMode)
        val inverseSemantics = json.getJSONObject("semantics").getJSONObject(inverseMode)
        val accent = accents(preferences.accent).color(mode)
        val base = if (dark) darkColorScheme() else lightColorScheme()
        return base.copy(
            primary = accent,
            onPrimary = if (dark) surfaces.color("background") else Color.White,
            primaryContainer = surfaces.color("elevated"),
            onPrimaryContainer = accent,
            secondary = semantics.color("textSecondary"),
            onSecondary = if (dark) surfaces.color("background") else Color.White,
            secondaryContainer = surfaces.color("tile"),
            onSecondaryContainer = semantics.color("textPrimary"),
            tertiary = semantics.color("textSecondary"),
            onTertiary = if (dark) surfaces.color("background") else Color.White,
            tertiaryContainer = surfaces.color("tile"),
            onTertiaryContainer = semantics.color("textPrimary"),
            background = surfaces.color("background"),
            onBackground = semantics.color("textPrimary"),
            surface = surfaces.color("surface"),
            onSurface = semantics.color("textPrimary"),
            surfaceVariant = surfaces.color("elevated"),
            onSurfaceVariant = semantics.color("textSecondary"),
            surfaceDim = surfaces.color("background"),
            surfaceBright = surfaces.color("elevated"),
            surfaceContainerLowest = surfaces.color("background"),
            surfaceContainerLow = surfaces.color("surface"),
            surfaceContainer = surfaces.color("surface"),
            surfaceContainerHigh = surfaces.color("elevated"),
            surfaceContainerHighest = surfaces.color("tile"),
            surfaceTint = accent,
            outline = semantics.color("textSecondary"),
            outlineVariant = semantics.color("hairline"),
            inverseSurface = inverseSurfaces.color("surface"),
            inverseOnSurface = inverseSemantics.color("textPrimary"),
            inversePrimary = accents(preferences.accent).color(inverseMode),
            scrim = Color.Black,
            errorContainer = surfaces.color("elevated"),
            onErrorContainer = semantics.color("danger"),
            error = semantics.color("danger"),
            onError = if (dark) surfaces.color("background") else Color.White,
        )
    }

    private fun accents(accent: AccentChoice) = json.getJSONObject("accents").getJSONObject(accent.key)

    private val Enum<*>.key: String get() = name.lowercase(Locale.ROOT)

    private fun JSONObject.color(key: String) = Color(android.graphics.Color.parseColor(getString(key)))
}
