package fm.bae.app.shots

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.components.StatusGlyph
import fm.bae.app.ui.components.StatusGlyphSet
import fm.bae.app.ui.components.StatusTone
import org.junit.Test

/** The component shots of `StatusGlyph` on Android: its macOS and muted rows draw the ring a notice leads with. */
class StatusGlyphShots : BoardShot() {
    @Test
    fun androidComponentStatusGlyphAndroid() =
        component("android.component.status-glyph.android") {
            // The preview's cell sets the glyphs side by side, 16 apart.
            Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                listOf(StatusTone.INFO, StatusTone.SUCCESS, StatusTone.WARNING, StatusTone.DANGER).forEach { tone ->
                    StatusGlyph(tone, size = 24.dp)
                }
            }
        }

    @Test
    fun androidComponentStatusGlyphMacos() =
        component("android.component.status-glyph.macos") {
            Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                listOf(StatusTone.INFO, StatusTone.SUCCESS, StatusTone.WARNING, StatusTone.DANGER).forEach { tone ->
                    // The preview sets the ring 20 across; the icon's box is 1.2 times that.
                    StatusGlyph(tone, size = 24.dp, set = StatusGlyphSet.RING)
                }
            }
        }

    @Test
    fun androidComponentStatusGlyphInline() =
        component("android.component.status-glyph.inline") {
            Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                listOf(StatusTone.INFO, StatusTone.SUCCESS, StatusTone.WARNING, StatusTone.DANGER).forEach { tone ->
                    StatusGlyph(tone, size = 24.dp)
                }
            }
        }

    @Test
    fun androidComponentStatusGlyphMuted() =
        component("android.component.status-glyph.muted") {
            Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                listOf(StatusTone.INFO, StatusTone.SUCCESS, StatusTone.WARNING, StatusTone.DANGER).forEach { tone ->
                    StatusGlyph(tone, size = 24.dp, muted = true, set = StatusGlyphSet.RING)
                }
            }
        }
}
