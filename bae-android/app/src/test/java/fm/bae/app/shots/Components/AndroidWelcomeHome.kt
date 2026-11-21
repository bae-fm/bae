package fm.bae.app.shots

import fm.bae.app.ui.onboarding.AndroidWelcomeHome
import fm.bae.app.ui.onboarding.WelcomeDamagedJoinScene
import fm.bae.app.ui.onboarding.WelcomeLibrariesScene
import fm.bae.app.ui.onboarding.WelcomeLibrariesView
import fm.bae.app.ui.onboarding.WelcomeRemovingScene
import fm.bae.app.ui.onboarding.WelcomeSavedCodesScene
import org.junit.Test

/** The component shots of `AndroidWelcomeHome`, one for each row of its preview. */
class AndroidWelcomeHomeShots : BoardShot() {
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
}
