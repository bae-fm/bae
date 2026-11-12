package fm.bae.app.ui.appearance

import fm.bae.app.BaeRobolectricTestRunner
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import java.io.File

@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class AppearanceFileTest {
    @Test
    fun aStoredAccentTheAppDoesNotHaveReadsAsTheStandardOne() {
        val file = File.createTempFile("appearance", ".json")
        file.writeText("""{"mode":"DARK","accent":"TEAL","tone":"SAND"}""")
        val read = readAppearancePreferences(file)
        file.delete()
        assertEquals(AppearancePreferences(AppearanceMode.DARK, AccentChoice.STANDARD, SurfaceTone.SAND), read)
    }
}
