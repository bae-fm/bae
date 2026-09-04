package fm.bae.app.playback

import uniffi.bae_bridge.BridgeImageRef
import uniffi.bae_bridge.BridgeLoadingTrackInfo
import uniffi.bae_bridge.BridgeMediaControlPlayback
import uniffi.bae_bridge.BridgeMediaControlValues
import uniffi.bae_bridge.BridgePlaybackPauseReason
import uniffi.bae_bridge.BridgePlaybackPosition
import uniffi.bae_bridge.BridgePlaybackValueState
import uniffi.bae_bridge.BridgePlaybackValues
import uniffi.bae_bridge.BridgePreviewState
import uniffi.bae_bridge.BridgePreviewValues
import uniffi.bae_bridge.BridgeRepeatMode

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
        remoteDeviceName = null,
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

internal fun playingState(
    trackId: String = "track-1",
    trackTitle: String = "Track Title",
    artistNames: String = "Artist Name",
    artistId: String = "artist-1",
    albumId: String = "album-1",
    albumTitle: String = "Album Title",
    coverImage: BridgeImageRef? = null,
    durationMs: ULong = 180_000uL,
) = BridgePlaybackValueState.Playing(
    trackId = trackId,
    trackTitle = trackTitle,
    artistNames = artistNames,
    artistId = artistId,
    albumId = albumId,
    albumTitle = albumTitle,
    coverImage = coverImage,
    durationMs = durationMs,
)

internal fun pausedState(
    trackId: String = "track-1",
    trackTitle: String = "Track Title",
    artistNames: String = "Artist Name",
    artistId: String = "artist-1",
    albumId: String = "album-1",
    albumTitle: String = "Album Title",
    coverImage: BridgeImageRef? = null,
    durationMs: ULong = 180_000uL,
    reason: BridgePlaybackPauseReason = BridgePlaybackPauseReason.Manual,
) = BridgePlaybackValueState.Paused(
    trackId = trackId,
    trackTitle = trackTitle,
    artistNames = artistNames,
    artistId = artistId,
    albumId = albumId,
    albumTitle = albumTitle,
    coverImage = coverImage,
    durationMs = durationMs,
    reason = reason,
)

internal fun loadingState(
    trackId: String,
    track: BridgeLoadingTrackInfo?,
) = BridgePlaybackValueState.Loading(trackId, track)

internal fun BaeCorePlayer.applyPlaybackState(state: BridgePlaybackValueState) {
    applyValues(playbackValues(state))
}
