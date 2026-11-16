package fm.bae.app.widget

import fm.bae.app.playback.BaeCorePlayer
import fm.bae.app.playback.NowPlaying

/** What the home-screen widget draws. */
internal sealed interface WidgetContent {
    /**
     * The track the open library plays, [nowPlaying], with a pause button only while [isPlaying]. The widget
     * reads the track's cover from the open library's image store. A track the library no longer holds shows as
     * removed, with no artist and no cover.
     */
    data class Track(
        val nowPlaying: NowPlaying,
        val isPlaying: Boolean,
    ) : WidgetContent

    /** No library is open, or the open library plays nothing. */
    data object NothingPlaying : WidgetContent
}

/**
 * What the widget draws for the open library's [player], null when this process has no library open.
 * With no library open nothing plays, so the widget shows no track.
 */
internal fun widgetContent(player: BaeCorePlayer?): WidgetContent {
    val nowPlaying = player?.nowPlaying?.value ?: return WidgetContent.NothingPlaying
    return WidgetContent.Track(nowPlaying, player.isPlaying.value)
}
