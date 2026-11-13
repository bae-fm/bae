package fm.bae.app

import android.app.Application
import android.content.res.Configuration
import android.os.LocaleList
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import java.util.Locale

/**
 * The open library is told the language the app is shown in when its session
 * starts and each time the device's language changes, and stops being told
 * once its session ends.
 */
@OptIn(ExperimentalCoroutinesApi::class)
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class SortLanguageTest {
    private val application: Application = RuntimeEnvironment.getApplication()

    private fun changeLanguage(tag: String) {
        application.onConfigurationChanged(
            Configuration().apply { setLocales(LocaleList(Locale.forLanguageTag(tag))) },
        )
    }

    @Test
    fun theLanguageIsToldAtStartAndAfterEachChange() =
        runTest(StandardTestDispatcher()) {
            val tags = mutableListOf<String>()
            val sortLanguage = SortLanguage(application, backgroundScope) { tags += it }
            sortLanguage.start()
            runCurrent()
            assertEquals(
                listOf(
                    application.resources.configuration.locales
                        .get(0)
                        .toLanguageTag(),
                ),
                tags,
            )

            changeLanguage("sv-SE")
            runCurrent()
            changeLanguage("ru-RU")
            runCurrent()
            assertEquals(listOf("sv-SE", "ru-RU"), tags.drop(1))

            sortLanguage.stop()
            changeLanguage("de-DE")
            runCurrent()
            assertEquals(3, tags.size)
        }
}
