package fm.bae.app.playback

import uniffi.bae_bridge.BridgeImageRef
import uniffi.bae_bridge.BridgeLoadingTrack
import uniffi.bae_bridge.BridgeMediaControlPlayback
import uniffi.bae_bridge.BridgeMediaControlValues
import uniffi.bae_bridge.BridgeNowPlayingTrack
import uniffi.bae_bridge.BridgePlaybackPauseReason
import uniffi.bae_bridge.BridgePlaybackPosition
import uniffi.bae_bridge.BridgePlaybackPositions
import uniffi.bae_bridge.BridgePlaybackValueState
import uniffi.bae_bridge.BridgePlaybackValues
import uniffi.bae_bridge.BridgePlayingTrack
import uniffi.bae_bridge.BridgePreviewState
import uniffi.bae_bridge.BridgeRepeatMode
import uniffi.bae_bridge.BridgeTrackDisplay

internal fun playbackValues(
    state: BridgePlaybackValueState,
    volume: Float = 1f,
    isMuted: Boolean = false,
    repeatMode: BridgeRepeatMode = BridgeRepeatMode.OFF,
): BridgePlaybackValues =
    BridgePlaybackValues(
        state = state,
        volume = volume,
        isMuted = isMuted,
        repeatMode = repeatMode,
        remote = uniffi.bae_bridge.BridgeRemotePlayback.Local,
        preview = BridgePreviewState.Idle,
        mediaControl =
            BridgeMediaControlValues(
                playback = BridgeMediaControlPlayback.Library(state),
                volume = volume,
                isMuted = isMuted,
            ),
    )

/** Positions holding [library]; the media controls' timeline is not read on Android. */
internal fun playbackPositions(
    library: BridgePlaybackPosition?,
    seekRevision: ULong = 0u,
): BridgePlaybackPositions =
    BridgePlaybackPositions(
        library = library,
        seekRevision = seekRevision,
        preview = null,
        mediaControl = null,
    )

internal fun nowPlayingTrack(
    trackId: String = "track-1",
    title: String = "Track Title",
    artistNames: String = "Artist Name",
    albumId: String = "album-1",
    releaseId: String = "release-1",
    albumTitle: String = "Album Title",
    coverImage: BridgeImageRef? = null,
    durationMs: ULong? = 180_000uL,
) = BridgeNowPlayingTrack(
    track = BridgePlayingTrack(trackId = trackId, durationMs = durationMs),
    display =
        BridgeTrackDisplay(
            title = title,
            artistNames = artistNames,
            albumId = albumId,
            releaseId = releaseId,
            albumTitle = albumTitle,
            coverImage = coverImage,
        ),
)

internal fun playingState(
    trackId: String = "track-1",
    title: String = "Track Title",
    artistNames: String = "Artist Name",
    albumId: String = "album-1",
    releaseId: String = "release-1",
    albumTitle: String = "Album Title",
    coverImage: BridgeImageRef? = null,
    durationMs: ULong? = 180_000uL,
) = BridgePlaybackValueState.Playing(
    nowPlayingTrack(trackId, title, artistNames, albumId, releaseId, albumTitle, coverImage, durationMs),
)

internal fun pausedState(
    trackId: String = "track-1",
    title: String = "Track Title",
    artistNames: String = "Artist Name",
    albumId: String = "album-1",
    releaseId: String = "release-1",
    albumTitle: String = "Album Title",
    coverImage: BridgeImageRef? = null,
    durationMs: ULong? = 180_000uL,
    reason: BridgePlaybackPauseReason = BridgePlaybackPauseReason.Manual,
) = BridgePlaybackValueState.Paused(
    nowPlayingTrack(trackId, title, artistNames, albumId, releaseId, albumTitle, coverImage, durationMs),
    reason,
)

internal fun loadingState(trackId: String) = BridgePlaybackValueState.Loading(BridgeLoadingTrack.Unprepared(trackId))

internal fun loadingState(prepared: BridgeNowPlayingTrack) = BridgePlaybackValueState.Loading(BridgeLoadingTrack.Prepared(prepared))

internal fun BaeCorePlayer.applyPlaybackState(state: BridgePlaybackValueState) {
    applyValues(playbackValues(state))
}
