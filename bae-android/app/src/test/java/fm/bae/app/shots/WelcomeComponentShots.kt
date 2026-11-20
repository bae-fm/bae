package fm.bae.app.shots

import androidx.compose.runtime.Composable
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.components.AndroidScanCode
import fm.bae.app.ui.onboarding.AndroidWelcomeActions
import fm.bae.app.ui.onboarding.AndroidWelcomeHome
import fm.bae.app.ui.onboarding.RunningRemoval
import fm.bae.app.ui.onboarding.SCENE_UNREADABLE_JOIN
import fm.bae.app.ui.onboarding.WelcomeDamagedJoinScene
import fm.bae.app.ui.onboarding.WelcomeLibrariesCallbacks
import fm.bae.app.ui.onboarding.WelcomeLibrariesContent
import fm.bae.app.ui.onboarding.WelcomeLibrariesList
import fm.bae.app.ui.onboarding.WelcomeLibrariesScene
import fm.bae.app.ui.onboarding.WelcomeLibrariesView
import fm.bae.app.ui.onboarding.WelcomeRemovingScene
import fm.bae.app.ui.onboarding.WelcomeSavedCodesScene
import fm.bae.fastvalues.LatestStream
import fm.bae.fastvalues.only
import org.junit.Test
import uniffi.bae_bridge.BridgeClosingProgress
import uniffi.bae_bridge.BridgeClosingStep
import uniffi.bae_bridge.BridgeLibrary
import uniffi.bae_bridge.BridgeLibraryState

/**
 * The component shots of the welcome and adding a device: the welcome's content in each state, its list of libraries, its
 * first choices, and the code another device scans.
 */
class WelcomeComponentShots : BoardShot() {
    @Test
    fun androidComponentWelcomeHomeLibraries() =
        shot("android.component.welcome-home.libraries") { ScreenScene { WelcomeLibrariesScene() } }

    @Test
    fun androidComponentWelcomeHomeEmpty() = shot("android.component.welcome-home.empty") { ScreenScene { AndroidWelcomeHome() } }

    @Test
    fun androidComponentWelcomeHomeChildren() = shot("android.component.welcome-home.children") { ScreenScene { WelcomeSavedCodesScene() } }

    @Test
    fun androidComponentWelcomeHomeRemoving() = shot("android.component.welcome-home.removing") { ScreenScene { WelcomeRemovingScene() } }

    @Test
    fun androidComponentWelcomeHomeUnreadable() =
        shot("android.component.welcome-home.unreadable") {
            ScreenScene {
                AndroidWelcomeHome(
                    libraries = WelcomeLibrariesView(listed = true) { onAdd -> Libraries(extra = listOf(nameUnreadable), onAdd = onAdd) },
                )
            }
        }

    @Test
    fun androidComponentWelcomeHomeDamagedJoin() =
        shot("android.component.welcome-home.damaged-join") { ScreenScene { WelcomeDamagedJoinScene() } }

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

    @Test
    fun androidComponentWelcomeActionsResting() =
        component("android.component.welcome-actions.resting", wide = true) { AndroidWelcomeActions(onScan = {}, onEnter = {}) }

    @Test
    fun androidComponentScanCodeDefault() =
        component("android.component.scan-code.default") {
            AndroidScanCode(instruction = "Scan the QR code on their device, or share the code.", qr = INVITE_CODE, code = INVITE_CODE)
        }
}

/**
 * The welcome's list of Home music and Odile’s records, then the [extra] libraries, with the [running] removal and the
 * [unreadableInstallation] it is given.
 */
@Composable
private fun Libraries(
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

/** A library whose name bae cannot read: the welcome lists it by its id. */
private val nameUnreadable =
    BridgeLibrary(
        id = "6f1c2a9e-4b7d-4e21-9a0c-3d5e8b2f7c14",
        name = null,
        path = "/libraries/6f1c2a9e-4b7d-4e21-9a0c-3d5e8b2f7c14",
        state = BridgeLibraryState.NameUnreadable,
        isActive = false,
    )

/** A short invite code, which the boards draw: one with no S3 key. */
private const val INVITE_CODE = "0000 0000"
