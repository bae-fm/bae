package fm.bae.app.shots

import androidx.compose.runtime.Composable
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.onboarding.RunningRemoval
import fm.bae.app.ui.onboarding.SCENE_UNREADABLE_JOIN
import fm.bae.app.ui.onboarding.WelcomeLibrariesCallbacks
import fm.bae.app.ui.onboarding.WelcomeLibrariesContent
import fm.bae.app.ui.onboarding.WelcomeLibrariesList
import fm.bae.fastvalues.LatestStream
import fm.bae.fastvalues.only
import org.junit.Test
import uniffi.bae_bridge.BridgeClosingProgress
import uniffi.bae_bridge.BridgeClosingStep
import uniffi.bae_bridge.BridgeLibrary
import uniffi.bae_bridge.BridgeLibraryState

/** The component shots of `AndroidWelcomeLibraries`, one for each row of its preview. */
class AndroidWelcomeLibrariesShots : BoardShot() {
    @Test
    fun androidComponentWelcomeLibrariesOpen() = component("android.component.welcome-libraries.open", wide = true) { Libraries() }

    @Test
    fun androidComponentWelcomeLibrariesRemoving() =
        component("android.component.welcome-libraries.removing", wide = true) { Libraries(running = leavingHomeMusic()) }

    @Test
    fun androidComponentWelcomeLibrariesUnreadable() =
        component("android.component.welcome-libraries.unreadable", wide = true) { Libraries(extra = listOf(nameUnreadable)) }

    @Test
    fun androidComponentWelcomeLibrariesDamagedJoin() =
        component("android.component.welcome-libraries.damaged-join", wide = true) {
            Libraries(unreadableInstallation = SCENE_UNREADABLE_JOIN)
        }
}

/** The removal of Home music, at its step that leaves the library. */
private fun leavingHomeMusic() =
    RunningRemoval(
        libraryId = sceneLibrary.id,
        removing = true,
        opening = null,
        closing =
            LatestStream
                .closingSteps(
                    BridgeClosingProgress(BridgeClosingStep.LEAVING_LIBRARY, "core.library.closing_step.leaving_library"),
                ).only,
    )

/**
 * The welcome's list of Home music and Odile’s records, then the [extra] libraries, with the [running] removal and the
 * [unreadableInstallation] it is given.
 */
@Composable
internal fun Libraries(
    running: RunningRemoval? = null,
    unreadableInstallation: String? = null,
    extra: List<BridgeLibrary> = emptyList(),
    onAdd: () -> Unit = {},
) = WelcomeLibrariesContent(
    list =
        WelcomeLibrariesList.Loaded(
            listOf(sceneLibrary, PreviewData.library(id = "lib-2", name = "Odile’s records", isActive = false)) + extra,
        ),
    running = running,
    callbacks = WelcomeLibrariesCallbacks(onOpen = {}, onDetails = {}, onRemove = {}, onAdd = onAdd),
    unreadableInstallation = unreadableInstallation,
)

/** A library whose name bae cannot read: the welcome lists it by its id. */
internal val nameUnreadable =
    BridgeLibrary(
        id = "6f1c2a9e-4b7d-4e21-9a0c-3d5e8b2f7c14",
        name = null,
        path = "/libraries/6f1c2a9e-4b7d-4e21-9a0c-3d5e8b2f7c14",
        state = BridgeLibraryState.NameUnreadable,
        isActive = false,
    )
