package fm.bae.app.ui.appearance

import android.content.Context
import androidx.compose.runtime.staticCompositionLocalOf
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.io.File
import java.io.FileOutputStream
import java.nio.file.Files
import java.nio.file.StandardCopyOption

enum class AppearanceMode { SYSTEM, LIGHT, DARK }

data class AppearancePreferences(
    val mode: AppearanceMode = AppearanceMode.SYSTEM,
    val accent: AccentChoice = AccentChoice.BLUE,
    val tone: SurfaceTone = SurfaceTone.NEUTRAL,
)

val LocalAppearanceStore = staticCompositionLocalOf<AppearanceStore> { error("BaeTheme provides appearance") }

/** Where the app keeps the appearance preferences. */
fun appearanceFile(context: Context): File = File(context.filesDir, "appearance.json")

/** The preferences stored in [file], or the defaults when none are. */
fun readAppearancePreferences(file: File): AppearancePreferences {
    if (!file.exists()) return AppearancePreferences()
    val json = JSONObject(file.readBytes().toString(Charsets.UTF_8))
    return AppearancePreferences(
        mode = AppearanceMode.valueOf(json.getString("mode")),
        accent = AccentChoice.valueOf(json.getString("accent")),
        tone = SurfaceTone.valueOf(json.getString("tone")),
    )
}

/** Publishes a selection only after its atomic preference write succeeds. */
class AppearanceStore(
    initial: AppearancePreferences,
    private val persist: suspend (AppearancePreferences) -> Unit,
) {
    private val mutablePreferences = MutableStateFlow(initial)
    val preferences = mutablePreferences.asStateFlow()
    private val writes = Mutex()

    suspend fun setMode(mode: AppearanceMode) = update { it.copy(mode = mode) }

    suspend fun setAccent(accent: AccentChoice) = update { it.copy(accent = accent) }

    suspend fun setTone(tone: SurfaceTone) = update { it.copy(tone = tone) }

    private suspend fun update(change: (AppearancePreferences) -> AppearancePreferences) {
        writes.withLock {
            // Once accepted, publish the committed value even if its screen closes.
            withContext(NonCancellable) {
                val next = change(mutablePreferences.value)
                persist(next)
                mutablePreferences.value = next
            }
        }
    }

    companion object {
        fun fromFile(
            file: File,
            ioDispatcher: CoroutineDispatcher,
        ): AppearanceStore =
            AppearanceStore(readAppearancePreferences(file)) { preferences ->
                withContext(ioDispatcher) {
                    val bytes =
                        JSONObject()
                            .put("mode", preferences.mode.name)
                            .put("accent", preferences.accent.name)
                            .put("tone", preferences.tone.name)
                            .toString()
                            .toByteArray(Charsets.UTF_8)
                    val temporary = File(file.parentFile, "${file.name}.tmp")
                    try {
                        FileOutputStream(temporary).use { stream ->
                            stream.write(bytes)
                            stream.fd.sync()
                        }
                        Files.move(
                            temporary.toPath(),
                            file.toPath(),
                            StandardCopyOption.ATOMIC_MOVE,
                            StandardCopyOption.REPLACE_EXISTING,
                        )
                    } finally {
                        Files.deleteIfExists(temporary.toPath())
                    }
                }
            }
    }
}
