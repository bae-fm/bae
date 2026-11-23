package fm.bae.app

import android.graphics.Paint
import android.graphics.Typeface
import android.graphics.text.TextRunShaper
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(BaeRobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35])
class ShotFontWeightsTest {
    @Test
    fun robotoGlyphsUseTheRequestedWeight() {
        val label = "Catalog"
        for (weight in listOf(400, 500, 600, 700, 800)) {
            val paint = Paint().apply { typeface = Typeface.create(Typeface.DEFAULT, weight, false) }
            val glyphs = TextRunShaper.shapeTextRun(label, 0, label.length, 0, label.length, 0f, 0f, false, paint)
            val glyphWeights =
                (0 until glyphs.glyphCount())
                    .map { index ->
                        val font = glyphs.getFont(index)
                        when (weight) {
                            600, 800 -> font.axes?.singleOrNull { it.tag == "wght" }?.styleValue
                            else -> font.style.weight.toFloat()
                        }
                    }.toSet()
            assertEquals("Roboto glyph weights for a $weight label", setOf(weight.toFloat()), glyphWeights)
        }
    }
}
