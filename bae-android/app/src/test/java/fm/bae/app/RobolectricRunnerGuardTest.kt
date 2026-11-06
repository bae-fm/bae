package fm.bae.app

import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * Every Robolectric test runs under [BaeRobolectricTestRunner], which paces
 * frames; a class on a bare `RobolectricTestRunner` or `AndroidJUnit4` runs
 * unpaced, and a frame-following view can then spin its idle wait forever.
 * The test reads the module's test sources, since the runner is chosen by an
 * annotation nothing else checks.
 */
class RobolectricRunnerGuardTest {
    private val unpacedRunner = Regex("""@RunWith\(\s*(?:[\w.]*\.)?(RobolectricTestRunner|AndroidJUnit4)::class""")

    @Test
    fun noTestClassRunsOnABareRobolectricRunner() {
        val sources = File("src").listFiles { dir -> dir.name.startsWith("test") }.orEmpty()
        val tests = sources.flatMap { it.walkTopDown().filter { f -> f.extension == "kt" }.toList() }
        assertTrue("found no test sources from ${File(".").absolutePath}", tests.isNotEmpty())
        val unpaced = tests.filter { unpacedRunner.containsMatchIn(it.readText()) }
        assertTrue(
            "run these under BaeRobolectricTestRunner so frames are paced: ${unpaced.joinToString { it.path }}",
            unpaced.isEmpty(),
        )
    }
}
