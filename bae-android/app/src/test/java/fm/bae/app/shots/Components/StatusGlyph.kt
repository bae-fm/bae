package fm.bae.app.shots

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.runtime.Composable
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.components.StatusGlyph
import fm.bae.app.ui.components.StatusGlyphSet
import fm.bae.app.ui.components.StatusTone
import org.junit.Test

/** The component shots of `StatusGlyph` on Android: its ring rows draw the ring a notice leads with. */
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
    fun androidComponentStatusGlyphRing() = component("android.component.status-glyph.ring") { RingRow(ring = 17.dp) }

    @Test
    fun androidComponentStatusGlyphInline() = component("android.component.status-glyph.inline") { RingRow(ring = INLINE_RING) }
}

/** The ring row as the preview draws it: four rings [ring] across with the preview cell's 16 between them. */
@Composable
private fun RingRow(ring: Dp) {
    Row(horizontalArrangement = Arrangement.spacedBy(ROW_GAP)) {
        listOf(StatusTone.INFO, StatusTone.SUCCESS, StatusTone.WARNING, StatusTone.DANGER).forEach { tone ->
            StatusGlyph(tone, size = ring, set = StatusGlyphSet.RING)
        }
    }
}

/** The inline row's ring is 1.19 times the 13 px text beside it. */
private val INLINE_RING = (13 * 1.19f).dp

private val ROW_GAP = 16.dp
