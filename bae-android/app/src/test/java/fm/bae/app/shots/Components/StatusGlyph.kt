package fm.bae.app.shots

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
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

/**
 * The ring row as the preview draws it: four rings [ring] across with the preview cell's 16 between them, the first
 * at the frame's top left. The glyph's box is 1.2 times its ring, which leaves a tenth of it empty on each side.
 */
@Composable
private fun RingRow(ring: Dp) {
    val box = ring * RING_BOX_SHARE
    val margin = (box - ring) / 2
    Row(
        horizontalArrangement = Arrangement.spacedBy(ring + ROW_GAP - box),
        modifier = Modifier.offset(-margin, -margin),
    ) {
        listOf(StatusTone.INFO, StatusTone.SUCCESS, StatusTone.WARNING, StatusTone.DANGER).forEach { tone ->
            // The frame is shorter than the glyph's box, which a plain size would shrink to fit.
            StatusGlyph(tone, modifier = Modifier.requiredSize(box), size = box, set = StatusGlyphSet.RING)
        }
    }
}

private const val RING_BOX_SHARE = 1.2f

/** The inline row's ring is 1.19 times the 13 px text beside it. */
private val INLINE_RING = (13 * 1.19f).dp

private val ROW_GAP = 16.dp
