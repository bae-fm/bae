extension ShotScene {
    /// Every macOS scene, one for each board it is shot for. A scene is
    /// present only when it has a real staging; a missing scene is a
    /// deliberate omission, never a swallowed failure.
    static let all: [ShotScene] =
        welcomeScenes + componentScenes + syncSettingsScenes
        + librarySettingsScenes
        + syncSetupScenes
        + storageSettingsScenes + storageManagerScenes + saveAsSettingsScenes
        + alertScenes + [importSettingsScene]
        + importListScenes
        + importPaneScenes + importLightboxScenes
        + adminChangeScenes + menuScenes + playlistScenes + historyScenes
        + personMenuScenes + castingScenes + settingsPlaybackScenes
        + peopleScenes + albumCoverScenes + albumLightboxScenes + playbackScenes
        + albumEditorScenes + pickedTracksScenes
        + albumRatingScenes + albumCreditsScenes + albumStorageScenes
        + searchScenes
        + [groupedAlbumsScene, inactiveLibraryScene, albumCatalogLinksScene]
}
