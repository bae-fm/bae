package fm.bae.app.shots

import androidx.compose.material3.Text
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.AssumptionViolatedException
import org.junit.Test
import java.nio.file.Files

/** A board outside the run's selection draws nothing; one inside it writes both of its files. */
class BoardShotSelectionTest : BoardShot() {
    @Test
    fun androidSelectionOutside() = drawn("android.selection.outside")

    @Test
    fun androidSelectionInside() = drawn("android.selection.inside")

    /** [board]'s files in a temporary shots directory with `android.selection.in` selected, and whether it was skipped. */
    private fun drawn(board: String) {
        val directory = Files.createTempDirectory("shots").toFile()
        val before = listOf("bae.shotsDir", "bae.shotsSelect", "roborazzi.test.record").associateWith { System.getProperty(it) }
        System.setProperty("bae.shotsDir", directory.path)
        System.setProperty("bae.shotsSelect", "android.selection.in")
        System.setProperty("roborazzi.test.record", "true")
        try {
            val skipped =
                try {
                    shot(board) { Text(board) }
                    false
                } catch (_: AssumptionViolatedException) {
                    true
                }
            val written = directory.list().orEmpty().sorted()
            if (board.endsWith("inside")) {
                assertFalse(skipped)
                assertEquals(listOf("$board@android.json", "$board@android.png"), written)
            } else {
                assertTrue(skipped)
                assertEquals(emptyList<String>(), written)
            }
        } finally {
            before.forEach { (key, value) -> if (value == null) System.clearProperty(key) else System.setProperty(key, value) }
            directory.deleteRecursively()
        }
    }

    @Test
    fun aSelectionIsThePrefixesItListsOrEverything() {
        assertTrue(isSelected("android.welcome.libraries", null))
        assertTrue(isSelected("android.welcome.libraries", ""))
        assertTrue(isSelected("android.welcome.libraries", "android.welcome."))
        assertTrue(isSelected("android.component.row.title", "android.welcome.,android.component.row."))
        assertFalse(isSelected("android.component.rows.title", "android.component.row."))
        assertFalse(isSelected("android.settings.library", "android.welcome."))
    }
}
