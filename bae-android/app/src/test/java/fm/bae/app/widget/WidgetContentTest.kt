package fm.bae.app.widget

import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.plainCreditText
import fm.bae.app.playback.AudioFocus
import fm.bae.app.playback.BaeCorePlayer
import fm.bae.app.playback.FakeAppHandle
import fm.bae.app.playback.PlaybackSystem
import fm.bae.app.playback.applyPlaybackState
import fm.bae.app.playback.pausedState
import fm.bae.app.playback.playbackValues
import fm.bae.app.playback.playingState
import fm.bae.app.playback.removedPlayingState
import fm.bae.app.testCoverRef
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgePlaybackValueState

/**
 * The widget draws what the open library's player plays: its track, shown as
 * playing only while the player plays it, a track the library no longer holds
 * as removed, and no track when no library is open or the player plays
 * nothing.
 */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class WidgetContentTest {
    private val context = RuntimeEnvironment.getApplication()

    @Test
    fun aPlayingPlayerShowsItsTrackAsPlaying() {
        assertEquals(
            WidgetContent.Track(
                title = "Track Title",
                artist = "Artist Name",
                coverImage = testCoverRef("rel-1"),
                isPlaying = true,
            ),
            widgetContent(playingPlayer(), context),
        )
    }

    @Test
    fun aTrackTheLibraryNoLongerHoldsShowsAsRemovedWithNoArtistOrCover() {
        val player = player()
        player.applyPlaybackState(removedPlayingState(trackId = "t1"))

        assertEquals(
            WidgetContent.Track(title = "Removed from library", artist = null, coverImage = null, isPlaying = true),
            widgetContent(player, context),
        )
    }

    @Test
    fun aPausedPlayerShowsItsTrackAsStopped() {
        val player = playingPlayer()
        player.applyPlaybackState(pausedState(trackId = "t1"))

        val content = widgetContent(player, context) as WidgetContent.Track
        assertFalse(content.isPlaying)
    }

    @Test
    fun aStoppedPlayerShowsNothingPlaying() {
        val player = player()
        player.applyValues(playbackValues(BridgePlaybackValueState.Stopped))

        assertEquals(WidgetContent.NothingPlaying, widgetContent(player, context))
    }

    @Test
    fun noOpenLibraryShowsNothingPlaying() {
        assertEquals(WidgetContent.NothingPlaying, widgetContent(player = null, context))
    }

    private fun playingPlayer(): BaeCorePlayer =
        player().apply {
            applyPlaybackState(
                playingState(
                    trackId = "t1",
                    title = "Track Title",
                    artistNames = "Artist Name",
                    albumId = "album-1",
                    albumTitle = "Album Title",
                    coverImage = testCoverRef("rel-1"),
                    durationMs = 185_000uL,
                ),
            )
        }

    private fun player(): BaeCorePlayer {
        val context = RuntimeEnvironment.getApplication()
        return BaeCorePlayer(
            applicationLooper = android.os.Looper.getMainLooper(),
            appHandle = FakeAppHandle(),
            system = PlaybackSystem(context, AudioFocus(context), { false }),
            creditLineText = ::plainCreditText,
            scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
        )
    }
}
