package fm.bae.app.data

class OpenLibraryStores(
    val library: LibraryStore,
    val config: ConfigStore,
    val syncStatus: SyncStatusStore,
    val downloads: DownloadStore,
    val outbox: OutboxStore,
    val cast: CastStore,
)
