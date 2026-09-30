package fm.bae.app.playback

import uniffi.bae_bridge.BridgeImageRef
import uniffi.bae_bridge.BridgeMediaControlPlayback
import uniffi.bae_bridge.BridgeMediaControlValues
import uniffi.bae_bridge.BridgeNowPlayingTrack
import uniffi.bae_bridge.BridgePlaybackPauseReason
import uniffi.bae_bridge.BridgePlaybackPosition
import uniffi.bae_bridge.BridgePlaybackValueState
import uniffi.bae_bridge.BridgePlaybackValues
import uniffi.bae_bridge.BridgePlayingTrack
import uniffi.bae_bridge.BridgePreviewState
import uniffi.bae_bridge.BridgePreviewValues
import uniffi.bae_bridge.BridgeRepeatMode
import uniffi.bae_bridge.BridgeTrackDisplay

internal fun playbackValues(
    state: BridgePlaybackValueState,
    position: BridgePlaybackPosition? = null,
    seekRevision: ULong = 0u,
    volume: Float = 1f,
    isMuted: Boolean = false,
    repeatMode: BridgeRepeatMode = BridgeRepeatMode.OFF,
): BridgePlaybackValues =
    BridgePlaybackValues(
        state = state,
        position = position,
        seekRevision = seekRevision,
        volume = volume,
        isMuted = isMuted,
        repeatMode = repeatMode,
        remoteDevice = null,
        preview = BridgePreviewValues(BridgePreviewState.Idle, 0uL, 0.0),
        mediaControl =
            BridgeMediaControlValues(
                playback =
                    BridgeMediaControlPlayback.Library(
                        state,
                        null,
                        seekRevision,
                    ),
                volume = volume,
                isMuted = isMuted,
            ),
    )

internal fun nowPlayingTrack(
    trackId: String = "track-1",
    title: String = "Track Title",
    artistNames: String = "Artist Name",
    albumId: String = "album-1",
    releaseId: String = "release-1",
    albumTitle: String = "Album Title",
    coverImage: BridgeImageRef? = null,
    durationMs: ULong = 180_000uL,
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
    durationMs: ULong = 180_000uL,
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
    durationMs: ULong = 180_000uL,
    reason: BridgePlaybackPauseReason = BridgePlaybackPauseReason.Manual,
) = BridgePlaybackValueState.Paused(
    nowPlayingTrack(trackId, title, artistNames, albumId, releaseId, albumTitle, coverImage, durationMs),
    reason,
)

internal fun loadingState(
    trackId: String,
    prepared: BridgeNowPlayingTrack?,
) = BridgePlaybackValueState.Loading(trackId, prepared)

internal fun BaeCorePlayer.applyPlaybackState(state: BridgePlaybackValueState) {
    applyValues(playbackValues(state))
}
