package fm.bae.app.ui.appearance

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test
import java.io.IOException

class AppearanceStoreTest {
    @Test
    fun refusedWriteKeepsThePreviousSelection() {
        val store = AppearanceStore(AppearancePreferences()) { throw IOException("refused") }
        assertThrows(IOException::class.java) { runBlocking { store.setAccent(AccentChoice.TEAL) } }
        assertEquals(AppearancePreferences(), store.preferences.value)
    }

    @Test
    fun independentSelectionsAreSerializedWithoutOverwritingEachOther() =
        runBlocking {
            val entered = CompletableDeferred<Unit>()
            val release = CompletableDeferred<Unit>()
            val saved = mutableListOf<AppearancePreferences>()
            val store =
                AppearanceStore(AppearancePreferences()) {
                    entered.complete(Unit)
                    release.await()
                    saved.add(it)
                }
            val accent = launch { store.setAccent(AccentChoice.TEAL) }
            entered.await()
            val tone = launch { store.setTone(SurfaceTone.PLUM) }
            release.complete(Unit)
            accent.join()
            tone.join()
            val expected = AppearancePreferences(accent = AccentChoice.TEAL, tone = SurfaceTone.PLUM)
            assertEquals(expected, store.preferences.value)
            assertEquals(expected, saved.last())
        }

    @Test
    fun closingTheScreenDoesNotInterruptAnAcceptedWrite() =
        runBlocking {
            val entered = CompletableDeferred<Unit>()
            val release = CompletableDeferred<Unit>()
            var persisted = AppearancePreferences()
            val store =
                AppearanceStore(persisted) {
                    entered.complete(Unit)
                    release.await()
                    persisted = it
                }
            val save = launch { store.setMode(AppearanceMode.DARK) }
            entered.await()
            save.cancel()
            release.complete(Unit)
            save.cancelAndJoin()
            assertEquals(AppearanceMode.DARK, persisted.mode)
            assertEquals(persisted, store.preferences.value)
        }
}
