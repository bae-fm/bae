package fm.bae.app.data

class OpenLibraryStores(
    val library: LibraryStore,
    val config: ConfigStore,
    val syncStatus: SyncStatusStore,
    val syncActivity: SyncActivityStore,
    val transfers: LibraryTransferStores,
    val cast: CastStore,
)

class LibraryTransferStores(
    val downloads: DownloadStore,
    val outbox: OutboxStore,
)
