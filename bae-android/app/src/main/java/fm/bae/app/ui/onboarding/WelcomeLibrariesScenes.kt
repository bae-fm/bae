package fm.bae.app.ui.onboarding

import androidx.compose.runtime.Composable
import fm.bae.app.ui.PreviewData
import fm.bae.fastvalues.LatestStream
import fm.bae.fastvalues.only
import uniffi.bae_bridge.BridgeClosingProgress
import uniffi.bae_bridge.BridgeClosingStep
import uniffi.bae_bridge.BridgeDropboxLocation
import uniffi.bae_bridge.BridgeLibrary
import uniffi.bae_bridge.BridgeLibraryState
import uniffi.bae_bridge.BridgeStorageLocation

/**
 * The welcome on a phone that holds a synced library and one that was never
 * synced, for the `welcome-libraries` screenshot scene.
 */
@Composable
internal fun WelcomeLibrariesScene() {
    AndroidWelcomeHome(libraries = WelcomeLibrariesView(listed = true) { onAdd -> WelcomeLibrariesSceneContent(onAdd) })
}

/**
 * The same welcome while the synced library is being removed and is at its
 * leaving step, for the `welcome-removing` screenshot scene.
 */
@Composable
internal fun WelcomeRemovingScene() {
    AndroidWelcomeHome(
        libraries =
            WelcomeLibrariesView(listed = true) { onAdd -> WelcomeLibrariesSceneContent(onAdd, sceneLeavingRemoval()) },
    )
}

/** The removal of the scenes' synced library, at its step that leaves the library. */
internal fun sceneLeavingRemoval() =
    RunningRemoval(
        libraryId = "lib-1",
        removing = true,
        opening = null,
        closing =
            LatestStream
                .closingSteps(
                    BridgeClosingProgress(
                        BridgeClosingStep.LEAVING_LIBRARY,
                        "core.library.closing_step.leaving_library",
                    ),
                ).only,
    )

/** The library whose unfinished join the damaged-join screenshot scenes show as unreadable. */
internal const val SCENE_UNREADABLE_JOIN = "3b9e07d4-51a2-4c8f-b6e3-0f2d9a7c4e81"

/**
 * The same welcome with an unfinished join this phone can't read after the
 * libraries, for the `welcome-libraries-damaged-join` screenshot scenes.
 */
@Composable
internal fun WelcomeDamagedJoinScene() {
    AndroidWelcomeHome(
        libraries =
            WelcomeLibrariesView(listed = true) { onAdd ->
                WelcomeLibrariesSceneContent(onAdd, unreadableInstallation = SCENE_UNREADABLE_JOIN)
            },
    )
}

/**
 * The scenes' list of a synced library and one that was never synced, then the
 * [extra] libraries, with the [running] removal and the
 * [unreadableInstallation] it is given.
 */
@Composable
internal fun WelcomeLibrariesSceneContent(
    onAdd: () -> Unit,
    running: RunningRemoval? = null,
    unreadableInstallation: String? = null,
    extra: List<BridgeLibrary> = emptyList(),
) {
    val dropbox =
        BridgeLibraryState.Openable(
            storage =
                BridgeStorageLocation.Dropbox(
                    location = BridgeDropboxLocation.AppFolder,
                    accountLabel = "name@example.com",
                    folderLabel = null,
                ),
        )
    WelcomeLibrariesContent(
        list =
            WelcomeLibrariesList.Loaded(
                listOf(
                    BridgeLibrary(
                        "lib-1",
                        "Home music",
                        "/tmp/lib-1",
                        dropbox,
                        isActive = false,
                    ),
                    PreviewData.library(id = "lib-2", name = "Odile’s records", isActive = false),
                ) + extra,
            ),
        running = running,
        callbacks =
            WelcomeLibrariesCallbacks(
                onOpen = {},
                onDetails = {},
                onRemove = {},
                onAdd = onAdd,
            ),
        unreadableInstallation = unreadableInstallation,
    )
}
