package fm.bae.app.ui.playback

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.clockText
import fm.bae.app.currentLocale
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.playback.NowPlaying
import fm.bae.app.ui.BaeAppChrome
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.PreviewTheme
import fm.bae.app.ui.library.LibraryGridSceneAlbums
import fm.bae.fastvalues.LatestStream
import uniffi.bae_bridge.BridgeDurationClock
import uniffi.bae_bridge.BridgeRepeatMode
import uniffi.bae_bridge.BridgeTrackBuffered
import uniffi.bae_bridge.BridgeTrackPosition
import uniffi.bae_bridge.BridgeTrackSpan
import java.util.Locale

private const val SCENE_TRACK_ID = "scene-track"
private const val SCENE_DURATION_MS = 222_000L
private const val SCENE_VOLUME = 0.7f

private const val MILLIS_PER_SECOND = 1_000L
private const val SECONDS_PER_MINUTE = 60L

// Where each scene's track is, and which parts of it are here: the start
// while its load waits; a gap a seek ahead left while it plays; all of it.
private const val WAITING_PROGRESS = 0.0
private val waitingHeld = listOf(BridgeTrackSpan(0.0, 0.08))
private const val GAPPED_PROGRESS = 0.3
private val gappedHeld = listOf(BridgeTrackSpan(0.0, 0.45), BridgeTrackSpan(0.62, 0.85))
private const val ALL_HERE_PROGRESS = 0.55
private val allHeld = listOf(BridgeTrackSpan(0.0, 1.0))

/**
 * What a player scene shows of its track: where it is, which parts of it
 * are here, and whether its load waits on storage.
 */
internal enum class PlayerSceneState(
    val progress: Double,
    val held: List<BridgeTrackSpan>,
    val waitsOnStorage: Boolean,
) {
    /** The load waits on storage for its first sound, with the start of the track here. */
    WAITING_ON_STORAGE(WAITING_PROGRESS, waitingHeld, waitsOnStorage = true),

    /** Playing, with a gap a seek ahead left in what is here. */
    PLAYING_WITH_A_GAP(GAPPED_PROGRESS, gappedHeld, waitsOnStorage = false),

    /** Playing, with every byte of the track here. */
    ALL_HERE(ALL_HERE_PROGRESS, allHeld, waitsOnStorage = false),
}

/** The scene's streams, holding [state]'s values under the scene track's id as core would. */
private class PlayerSceneStreams(
    state: PlayerSceneState,
) {
    private val position =
        BridgeTrackPosition(
            positionMs = (state.progress * SCENE_DURATION_MS).toLong(),
            durationMs = SCENE_DURATION_MS.toULong(),
            progress = state.progress,
            seekRevision = 0uL,
        )
    val positions = LatestStream.trackPositions(mapOf(SCENE_TRACK_ID to position)).source(SCENE_TRACK_ID)
    val buffered =
        LatestStream.trackBuffered(mapOf(SCENE_TRACK_ID to BridgeTrackBuffered(state.held))).source(SCENE_TRACK_ID)
    val storageWait =
        LatestStream
            .trackStorageWaits(if (state.waitsOnStorage) setOf(SCENE_TRACK_ID) else emptySet())
            .source(SCENE_TRACK_ID)

    /** The seek row's words, elapsed and total, as the bar's clocks give them. */
    fun seekBarState(
        shown: BridgeTrackPosition?,
        locale: Locale,
    ): SeekBarState =
        SeekBarState(
            progress = shown?.progress ?: 0.0,
            leading = clockText(clock(shown?.positionMs ?: 0L), locale),
            trailing = clockText(clock(SCENE_DURATION_MS), locale),
            seekable = true,
        )

    private fun clock(ms: Long): BridgeDurationClock {
        val seconds = ms / MILLIS_PER_SECOND
        return BridgeDurationClock(
            negative = false,
            hours = null,
            minutes = (seconds / SECONDS_PER_MINUTE).toUInt(),
            seconds = (seconds % SECONDS_PER_MINUTE).toUInt(),
        )
    }
}

private val sceneTrack =
    NowPlaying(
        trackId = SCENE_TRACK_ID,
        title = "Track Title",
        artist = "Artist Name",
        coverImage = PreviewData.imageRef("rel-1"),
        sidePausePrompt = null,
    )

/** The mini player at the foot of the library grid, its track in [state], needing no session. */
@Composable
internal fun MiniPlayerScene(state: PlayerSceneState) {
    val streams = remember(state) { PlayerSceneStreams(state) }
    CompositionLocalProvider(LocalImageStore provides ImageStore.unresolved()) {
        Column(modifier = Modifier.fillMaxSize()) {
            Box(modifier = Modifier.fillMaxWidth().weight(1f)) {
                LibraryGridSceneAlbums(rememberLazyGridState())
            }
            AndroidNowPlayingBar(
                track = sceneTrack,
                failure = null,
                onExpand = {},
                transport = {
                    NowPlayingTransport(
                        isPlaying = true,
                        storageWait = streams.storageWait,
                        repeatMode = BridgeRepeatMode.OFF,
                        onPrevious = {},
                        onPlayPause = {},
                        onNext = {},
                        // Casting is off: the bar has no Cast control.
                        castButton = {},
                        onOpenQueue = {},
                        onCycleRepeat = {},
                    )
                },
                progress = { SceneSeekRow(streams) },
                castEnded = {},
            )
        }
    }
}

/** The full player, its track in [state], needing no session. */
@Composable
internal fun FullPlayerScene(state: PlayerSceneState) {
    val streams = remember(state) { PlayerSceneStreams(state) }
    CompositionLocalProvider(LocalImageStore provides ImageStore.unresolved()) {
        Column(modifier = Modifier.verticalScroll(rememberScrollState())) {
            AndroidFullPlayer(
                track = sceneTrack,
                failure = null,
                onCollapse = {},
                seek = { SceneSeekRow(streams, SeekLineLook.LARGE) },
                transport = {
                    ExpandedTransportRow(
                        isPlaying = true,
                        storageWait = streams.storageWait,
                        onPrevious = {},
                        onPlayPause = {},
                        onNext = {},
                    )
                },
                secondaryControls = {
                    ExpandedSecondaryControls(repeatMode = BridgeRepeatMode.OFF, onCycleRepeat = {})
                },
                volumeRow = {
                    ExpandedVolumeRow(
                        volume = SCENE_VOLUME,
                        isMuted = false,
                        onSetMuted = {},
                        onDragVolume = {},
                        onSetVolume = {},
                    )
                },
            )
        }
    }
}

@Composable
private fun SceneSeekRow(
    streams: PlayerSceneStreams,
    look: SeekLineLook = SeekLineLook.BAR,
) {
    val locale = LocalContext.current.currentLocale()
    AndroidSeekBar(
        position = streams.positions,
        buffered = streams.buffered,
        state = { streams.seekBarState(it, locale) },
        onSeekRatio = {},
        onToggleRemaining = {},
        look = look,
    )
}

@Preview(showBackground = true)
@Composable
private fun MiniPlayerScenePreview() {
    PreviewTheme { BaeAppChrome { MiniPlayerScene(PlayerSceneState.PLAYING_WITH_A_GAP) } }
}

@Preview(showBackground = true)
@Composable
private fun FullPlayerScenePreview() {
    PreviewTheme { BaeAppChrome { FullPlayerScene(PlayerSceneState.WAITING_ON_STORAGE) } }
}
