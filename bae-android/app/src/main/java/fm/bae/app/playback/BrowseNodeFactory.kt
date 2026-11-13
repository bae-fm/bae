package fm.bae.app.playback

import android.net.Uri
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import uniffi.bae_bridge.BridgeComposerDetailItem
import uniffi.bae_bridge.BridgeComposerDetailRow
import uniffi.bae_bridge.BridgeComposerSummary
import uniffi.bae_bridge.BridgeCreditLine
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
    /** The credit line in the person's language, for a track node's artist. */
    private val creditLineText: (BridgeCreditLine) -> String,
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

    /**
     * A row of a composer's page: a work opens its parts and releases, and a credit opens its
     * album.
     */
    fun composerPageRow(row: BridgeComposerDetailRow): MediaItem =
        when (val item = row.item) {
            is BridgeComposerDetailItem.Work -> work(item.work)
            is BridgeComposerDetailItem.ReleaseCredit -> album(item.credit.albumId, item.credit.albumTitle, null)
            is BridgeComposerDetailItem.TrackCredit -> album(item.credit.albumId, item.credit.albumTitle, null)
        }

    /** A playable track node, carrying its release and track ids so the player
     *  starts the release at that track. */
    fun track(
        release: BridgeRelease,
        track: BridgeTrack,
    ): MediaItem {
        val metadata =
            baseMetadata(track.title, release.cover)
                .setArtist(creditLineText(track.artistLine))
                .setDurationMs(track.durationMs.toLong())
                .setIsBrowsable(false)
                .setIsPlayable(true)
                .setMediaType(MediaMetadata.MEDIA_TYPE_MUSIC)
                .build()
        return MediaItem
            .Builder()
            .setMediaId(BrowseId.Track(release.id, track.id).mediaId)
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
