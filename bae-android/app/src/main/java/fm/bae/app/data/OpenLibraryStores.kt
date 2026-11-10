package fm.bae.app.data

class OpenLibraryStores(
    val config: ConfigStore,
    val syncStatus: SyncStatusStore,
    val syncActivity: SyncActivityStore,
    val downloads: DownloadStore,
    val cast: CastStore,
)
