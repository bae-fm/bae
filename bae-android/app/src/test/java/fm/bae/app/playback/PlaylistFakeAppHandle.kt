package fm.bae.app.playback

import uniffi.bae_bridge.BridgeEntryPlace
import uniffi.bae_bridge.BridgePlayOutcome
import uniffi.bae_bridge.BridgeSharing
import uniffi.bae_bridge.PlaylistDetailSubscription
import uniffi.bae_bridge.PlaylistsSubscription

/** The bridge handle's playlist calls: the list of playlists sends nothing, and no test fakes the rest. */
internal abstract class PlaylistFakeAppHandle : EditionFakeAppHandle() {
    override suspend fun appendPickedToPlaylist(
        playlist: String,
        trackIds: List<String>,
    ): Unit = unfaked("appendPickedToPlaylist")

    override suspend fun appendPlaylistEntry(
        playlist: String,
        track: String,
    ): String = unfaked("appendPlaylistEntry")

    override suspend fun createPlaylist(
        name: String,
        sharing: BridgeSharing,
        trackIds: List<String>,
    ): String = unfaked("createPlaylist")

    override suspend fun createPlaylistOfPicked(
        name: String,
        sharing: BridgeSharing,
        trackIds: List<String>,
    ): String = unfaked("createPlaylistOfPicked")

    override suspend fun editPlaylist(
        id: String,
        name: String,
        sharing: BridgeSharing,
    ) {
        unfaked("editPlaylist")
    }

    override suspend fun deletePlaylist(id: String) {
        unfaked("deletePlaylist")
    }

    override suspend fun playPlaylist(
        playlistId: String,
        startPositionId: String?,
        shuffle: Boolean,
    ): BridgePlayOutcome = unfaked("playPlaylist")

    override suspend fun removePlaylistEntry(
        playlist: String,
        position: String,
    ) {
        unfaked("removePlaylistEntry")
    }

    override suspend fun movePlaylistEntry(
        playlist: String,
        position: String,
        place: BridgeEntryPlace,
    ) {
        unfaked("movePlaylistEntry")
    }

    override fun subscribePlaylistDetail(): PlaylistDetailSubscription = unfaked("subscribePlaylistDetail")

    override fun subscribePlaylists(): PlaylistsSubscription = FakePlaylistsSubscription()
}
