package fm.bae.app.data

class OpenLibraryStores(
    val library: LibraryStore,
    val config: ConfigStore,
    val downloads: DownloadStore,
    val outbox: OutboxStore,
    val cast: CastStore,
)
