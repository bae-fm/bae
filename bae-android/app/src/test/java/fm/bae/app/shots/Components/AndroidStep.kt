package fm.bae.app.shots

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Text
import androidx.compose.ui.Modifier
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidStep
import fm.bae.app.ui.components.AndroidTextField
import org.junit.Test

/** The component shots of `AndroidStep`, one for each row of its preview. */
class AndroidStepShots : BoardShot() {
    @Test
    fun androidComponentStepWithAField() =
        component("android.component.step.with-a-field") {
            AndroidStep(title = "You are invited to Home music") {
                AndroidTextField(value = "Dan", onValueChange = {}, label = "Your name", helper = "Everyone in the library sees this name.")
                AndroidButton(onClick = {}, modifier = Modifier.fillMaxWidth(), large = true) { Text("Join") }
            }
        }

    @Test
    fun androidComponentStepWithALine() =
        component("android.component.step.with-a-line") {
            AndroidStep(title = "This invite has ended", text = "Ask Ben for a new one.") {
                AndroidButton(onClick = {}, modifier = Modifier.fillMaxWidth(), large = true) { Text("OK") }
            }
        }

    @Test
    fun androidComponentStepTop() =
        component("android.component.step.top") {
            AndroidStep(title = "You are invited to Home music", top = true) {
                AndroidTextField(value = "Dan", onValueChange = {}, label = "Your name", helper = "Everyone in the library sees this name.")
                AndroidButton(onClick = {}, modifier = Modifier.fillMaxWidth(), large = true) { Text("Join") }
            }
        }
}
