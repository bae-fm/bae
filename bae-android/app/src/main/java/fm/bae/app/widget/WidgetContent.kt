package fm.bae.app.widget

import android.content.Context
import fm.bae.app.playback.BaeCorePlayer
import fm.bae.app.playback.artist
import fm.bae.app.playback.library
import fm.bae.app.playback.title
import uniffi.bae_bridge.BridgeImageRef

/** What the home-screen widget draws. */
internal sealed interface WidgetContent {
    /**
     * The track the open library plays, with a pause button only while [isPlaying]. [artist] is null for a track
     * the library no longer holds. The widget reads the cover by [coverImage] from the open library's image store.
     */
    data class Track(
        val title: String,
        val artist: String?,
        val coverImage: BridgeImageRef?,
        val isPlaying: Boolean,
    ) : WidgetContent

    /** No library is open, or the open library plays nothing. */
    data object NothingPlaying : WidgetContent
}

/**
 * What the widget draws for the open library's [player], null when this process has no library open.
 * With no library open nothing plays, so the widget shows no track.
 */
internal fun widgetContent(
    player: BaeCorePlayer?,
    context: Context,
): WidgetContent {
    val nowPlaying = player?.nowPlaying?.value ?: return WidgetContent.NothingPlaying
    val display = nowPlaying.display
    return WidgetContent.Track(
        title = display.title(context),
        artist = display.artist(context),
        coverImage = display.library?.coverImage,
        isPlaying = player.isPlaying.value,
    )
}
