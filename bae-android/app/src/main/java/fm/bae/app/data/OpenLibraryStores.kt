package fm.bae.app.data

class OpenLibraryStores(
    val library: LibraryStore,
    val config: ConfigStore,
    val syncStatus: SyncStatusStore,
    val transfers: LibraryTransferStores,
    val cast: CastStore,
)

class LibraryTransferStores(
    val artworkLoading: ArtworkLoadingStore,
    val downloads: DownloadStore,
    val outbox: OutboxStore,
)
