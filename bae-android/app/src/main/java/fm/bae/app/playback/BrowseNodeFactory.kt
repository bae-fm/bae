package fm.bae.app.playback

import android.net.Uri
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import uniffi.bae_bridge.BridgeComposerSummary
import uniffi.bae_bridge.BridgeImageRef
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeTrack
import uniffi.bae_bridge.BridgeWorkSummary

/**
 * Builds the [MediaItem] browse nodes the [LibraryBrowseTree] serves, from the
 * bridge library records it reads. This is the tree's presentation layer: it
 * turns a release/composer/work/track into the browsable or playable node a
 * head unit renders, and tags each node's artwork with the content URI its
 * bytes are fetched from lazily.
 */
internal class BrowseNodeFactory(
    /** Maps a library image reference (a release/composer/work cover) to the
     *  content URI the browse client fetches its bytes from — the same bytes the
     *  bridge's `fetchLibraryImageBytes` serves. */
    private val artworkUri: (image: BridgeImageRef) -> Uri,
) {
    /** A browsable (non-playable) node: a category, album, composer, or work the
     *  client drills into. */
    fun browsable(
        id: BrowseId,
        title: String,
        cover: BridgeImageRef?,
        mediaType: Int,
        subtitle: String? = null,
    ): MediaItem {
        val metadata =
            baseMetadata(title, cover)
                .setSubtitle(subtitle)
                .setIsBrowsable(true)
                .setIsPlayable(false)
                .setMediaType(mediaType)
                .build()
        return MediaItem
            .Builder()
            .setMediaId(id.mediaId)
            .setMediaMetadata(metadata)
            .build()
    }

    fun album(
        albumId: String,
        title: String,
        cover: BridgeImageRef?,
    ): MediaItem = browsable(BrowseId.Album(albumId), title, cover, MediaMetadata.MEDIA_TYPE_ALBUM)

    fun composer(composer: BridgeComposerSummary): MediaItem =
        browsable(
            id = BrowseId.Composer(composer.artistId),
            title = composer.name,
            cover = composer.image,
            mediaType = MediaMetadata.MEDIA_TYPE_ARTIST,
        )

    fun work(work: BridgeWorkSummary): MediaItem =
        browsable(
            id = BrowseId.Work(work.workId),
            title = work.title,
            cover = work.representativeCover,
            mediaType = MediaMetadata.MEDIA_TYPE_FOLDER_MIXED,
            subtitle = work.composerNames,
        )

    /** A playable track node, carrying its release-wide flat [index] so the
     *  player starts the release at that track. */
    fun track(
        release: BridgeRelease,
        track: BridgeTrack,
        index: Int,
    ): MediaItem {
        val metadata =
            baseMetadata(track.title, release.cover)
                .setArtist(track.artistNames)
                .setDurationMs(track.durationMs)
                .setIsBrowsable(false)
                .setIsPlayable(true)
                .setMediaType(MediaMetadata.MEDIA_TYPE_MUSIC)
                .build()
        return MediaItem
            .Builder()
            .setMediaId(BrowseId.Track(release.id, index).mediaId)
            .setMediaMetadata(metadata)
            .build()
    }

    private fun baseMetadata(
        title: String,
        cover: BridgeImageRef?,
    ): MediaMetadata.Builder {
        val builder = MediaMetadata.Builder().setTitle(title)
        cover?.let { builder.setArtworkUri(artworkUri(it)) }
        return builder
    }
}
