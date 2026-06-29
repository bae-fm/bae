package fm.bae.app.playback

import android.os.Looper
import androidx.media3.common.Player
import fm.bae.app.AppSessionHolder
import fm.bae.app.BridgeFixtures
import fm.bae.app.OpenLibrary
import fm.bae.app.data.ConfigStore
import fm.bae.app.data.LibraryStore
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.AppHandle
import uniffi.bae_bridge.BridgeUiEvent
import uniffi.bae_bridge.NoHandle
import uniffi.bae_bridge.UiEventCallback

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class PlaybackServiceTest {
    @After
    fun tearDown() {
        setCurrentSession(null)
    }

    @Test
    fun onCreateAddsMediaSessionForPlaybackPlayer() {
        val context = RuntimeEnvironment.getApplication()
        val looper = Looper.getMainLooper()
        val session = openLibrary(context, looper)
        setCurrentSession(session)

        val controller = Robolectric.buildService(PlaybackService::class.java).create()
        val service = controller.get()

        assertEquals(1, service.sessions.size)
        assertEquals(session.playback, service.sessions.single().player)

        controller.destroy()
    }

    @Test
    fun destroyingTheServiceDoesNotReleaseThePlayer() {
        val context = RuntimeEnvironment.getApplication()
        val looper = Looper.getMainLooper()
        val session = openLibrary(context, looper)
        setCurrentSession(session)

        Robolectric.buildService(PlaybackService::class.java).create().destroy()

        // The player is owned by OpenLibrary and must outlive the service: the
        // service can stop and restart (a fresh session over the same player) or
        // be killed by the system. Had onDestroy released the player, projecting a
        // new event would throw "Player is released".
        session.playback.onPlaying(
            BridgeUiEvent.PlaybackPlaying(
                "t1",
                "Track Title",
                "Artist Name",
                "artist-1",
                "album-1",
                "Album Title",
                null,
                200_000uL,
            ),
        )
        shadowOf(looper).idle()
        assertEquals(Player.STATE_READY, session.playback.playbackState)
    }

    @Test
    fun wireUpDoesNotStartTheServiceBeforePlayback() {
        val context = RuntimeEnvironment.getApplication()
        val session = openLibrary(context, Looper.getMainLooper())

        session.wireUp(CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate))

        // The service is started when playback begins on screen, not at library
        // open — a non-foreground service started here is reclaimed before the
        // first track plays.
        assertNull(shadowOf(context).nextStartedService)
    }

    private fun openLibrary(
        context: android.content.Context,
        looper: Looper,
    ): OpenLibrary {
        val handle = FakeAppHandle()
        return OpenLibrary(
            libraryId = "lib-1",
            appHandle = handle,
            libraryStore = LibraryStore(),
            configStore = ConfigStore(BridgeFixtures.config(), initialSyncReady = false),
            playback =
                BaeCorePlayer(
                    applicationLooper = looper,
                    appHandle = handle,
                    context = context,
                    scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
                    isAppForeground = { false },
                ),
            appContext = context,
        )
    }

    private fun setCurrentSession(session: OpenLibrary?) {
        val field = AppSessionHolder::class.java.getDeclaredField("current")
        field.isAccessible = true
        field.set(AppSessionHolder, session)
    }
}

internal class FakeAppHandle(
    private val imageBytes: Map<String, ByteArray> = emptyMap(),
) : AppHandle(NoHandle) {
    var pauseCount = 0
    var resumeCount = 0

    override fun pause() {
        pauseCount++
    }

    override fun resume() {
        resumeCount++
    }

    override fun savePlaybackState() {}

    override fun subscribeUiEvents(callback: UiEventCallback) {}

    override fun triggerSync() {}

    override suspend fun fetchCoverImageBytes(releaseId: String): ByteArray? = imageBytes[releaseId]
}
