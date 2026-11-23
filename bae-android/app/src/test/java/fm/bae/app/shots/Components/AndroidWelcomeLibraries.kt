package fm.bae.app.shots

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import fm.bae.app.ui.appearance.ThemeSpace
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
        component("android.component.welcome-libraries.open") {
            OnTheWelcome { WelcomeLibrariesSceneContent(onAdd = {}) }
        }

    @Test
    fun androidComponentWelcomeLibrariesRemoving() =
        component("android.component.welcome-libraries.removing") {
            OnTheWelcome { WelcomeLibrariesSceneContent(onAdd = {}, running = sceneLeavingRemoval()) }
        }

    @Test
    fun androidComponentWelcomeLibrariesUnreadable() =
        component("android.component.welcome-libraries.unreadable") {
            OnTheWelcome { WelcomeLibrariesSceneContent(onAdd = {}, extra = listOf(nameUnreadable)) }
        }

    @Test
    fun androidComponentWelcomeLibrariesDamagedJoin() =
        component("android.component.welcome-libraries.damaged-join") {
            OnTheWelcome { WelcomeLibrariesSceneContent(onAdd = {}, unreadableInstallation = SCENE_UNREADABLE_JOIN) }
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

/** The list as the welcome screen holds it: inside the screen's side padding. */
@Composable
private fun OnTheWelcome(content: @Composable () -> Unit) {
    Box(modifier = Modifier.padding(horizontal = ThemeSpace.page)) { content() }
}
