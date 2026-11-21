package fm.bae.app.shots

import fm.bae.app.ui.onboarding.SCENE_UNREADABLE_JOIN
import fm.bae.app.ui.onboarding.WelcomeLibrariesSceneContent
import fm.bae.app.ui.onboarding.sceneLeavingRemoval
import org.junit.Test
import uniffi.bae_bridge.BridgeLibrary
import uniffi.bae_bridge.BridgeLibraryState

/** The component shots of `AndroidWelcomeLibraries`, one for each row of its preview. */
class AndroidWelcomeLibrariesShots : BoardShot() {
    @Test
    fun androidComponentWelcomeLibrariesOpen() =
        component("android.component.welcome-libraries.open", wide = true) {
            WelcomeLibrariesSceneContent(onAdd = {})
        }

    @Test
    fun androidComponentWelcomeLibrariesRemoving() =
        component("android.component.welcome-libraries.removing", wide = true) {
            WelcomeLibrariesSceneContent(onAdd = {}, running = sceneLeavingRemoval())
        }

    @Test
    fun androidComponentWelcomeLibrariesUnreadable() =
        component("android.component.welcome-libraries.unreadable", wide = true) {
            WelcomeLibrariesSceneContent(onAdd = {}, extra = listOf(nameUnreadable))
        }

    @Test
    fun androidComponentWelcomeLibrariesDamagedJoin() =
        component("android.component.welcome-libraries.damaged-join", wide = true) {
            WelcomeLibrariesSceneContent(onAdd = {}, unreadableInstallation = SCENE_UNREADABLE_JOIN)
        }
}

/** A library whose name bae cannot read: the welcome lists it by its id. */
internal val nameUnreadable =
    BridgeLibrary(
        id = "6f1c2a9e-4b7d-4e21-9a0c-3d5e8b2f7c14",
        name = null,
        path = "/libraries/6f1c2a9e-4b7d-4e21-9a0c-3d5e8b2f7c14",
        state = BridgeLibraryState.NameUnreadable,
        isActive = false,
    )
