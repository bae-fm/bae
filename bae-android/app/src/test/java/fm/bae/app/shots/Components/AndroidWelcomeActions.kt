package fm.bae.app.shots

import fm.bae.app.ui.onboarding.AndroidWelcomeActions
import org.junit.Test

/** The component shots of `AndroidWelcomeActions`, one for each row of its preview. */
class AndroidWelcomeActionsShots : BoardShot() {
    @Test
    fun androidComponentWelcomeActionsResting() =
        component("android.component.welcome-actions.resting", wide = true) { AndroidWelcomeActions(onScan = {}, onEnter = {}) }
}
